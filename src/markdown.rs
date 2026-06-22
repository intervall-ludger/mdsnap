use pulldown_cmark::{Event, Parser, Tag};

/// Extract every local reference: markdown image/link destinations plus the
/// `src` of HTML `<img>`/embeds. Skips external URLs, anchors and mailto;
/// de-duplicated and in document order.
pub fn extract_local_refs(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for event in Parser::new(content) {
        match event {
            Event::Start(Tag::Image { dest_url, .. })
            | Event::Start(Tag::Link { dest_url, .. }) => {
                add_local(&mut refs, dest_url.to_string());
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                for src in html_srcs(&html) {
                    add_local(&mut refs, src);
                }
            }
            _ => {}
        }
    }
    refs
}

fn add_local(refs: &mut Vec<String>, url: String) {
    if is_local(&url) && !refs.contains(&url) {
        refs.push(url);
    }
}

pub fn is_local(url: &str) -> bool {
    !url.is_empty() && !url.starts_with('#') && !url.starts_with("//") && !has_url_scheme(url)
}

/// True if `url` starts with a URL scheme like `http:`, `data:`, `file:`, `ftp:`
/// (also a Windows drive letter `C:`); such references are never local paths.
fn has_url_scheme(url: &str) -> bool {
    let mut chars = url.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    for c in chars {
        if c == ':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) {
            return false;
        }
    }
    false
}

/// Every `src="..."` / `src='...'` value in an HTML fragment.
fn html_srcs(html: &str) -> Vec<String> {
    let mut srcs = Vec::new();
    let mut rest = html;
    while let Some(idx) = rest.find("src=") {
        let after = &rest[idx + 4..];
        let quote = after.chars().next();
        if let Some(quote @ ('"' | '\'')) = quote {
            if let Some(end) = after[1..].find(quote) {
                srcs.push(after[1..1 + end].to_string());
                rest = &after[1 + end..];
                continue;
            }
        }
        rest = after;
    }
    srcs
}

/// Replace one reference inside markdown destinations and HTML `src` attributes.
pub fn rewrite_ref(content: &str, old: &str, new: &str) -> String {
    content
        .replace(&format!("]({old})"), &format!("]({new})"))
        .replace(&format!("]({old} "), &format!("]({new} "))
        .replace(&format!("src=\"{old}\""), &format!("src=\"{new}\""))
        .replace(&format!("src='{old}'"), &format!("src='{new}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_markdown_image() {
        assert_eq!(
            extract_local_refs("![alt](plots/chart.png)"),
            vec!["plots/chart.png".to_string()]
        );
    }

    #[test]
    fn extracts_html_img_src() {
        assert_eq!(
            extract_local_refs("<img src=\"plots/diagram.png\" width=\"50%\"/>"),
            vec!["plots/diagram.png".to_string()]
        );
        assert_eq!(
            extract_local_refs("<img src='a.png'>"),
            vec!["a.png".to_string()]
        );
    }

    #[test]
    fn skips_external_and_anchors() {
        let refs = extract_local_refs(
            "![x](https://e.com/a.png) [y](#sec) <img src=\"http://e.com/b.png\">",
        );
        assert!(refs.is_empty());
    }

    #[test]
    fn dedups_references() {
        assert_eq!(
            extract_local_refs("![a](x.png) again ![b](x.png)"),
            vec!["x.png".to_string()]
        );
    }

    #[test]
    fn rewrites_markdown_and_html() {
        let out = rewrite_ref(
            "![a](old.png) <img src=\"old.png\"/>",
            "old.png",
            "assets/old.png",
        );
        assert_eq!(out, "![a](assets/old.png) <img src=\"assets/old.png\"/>");
    }

    #[test]
    fn is_local_classifies() {
        assert!(is_local("plots/a.png"));
        assert!(is_local("./a.png"));
        assert!(is_local("../up.png")); // relative escape is caught later by containment
        assert!(!is_local("https://x.com/a.png"));
        assert!(!is_local("data:image/png;base64,AAAA"));
        assert!(!is_local("file:///etc/passwd"));
        assert!(!is_local("ftp://h/a.png"));
        assert!(!is_local("mailto:x@y.com"));
        assert!(!is_local("#anchor"));
        assert!(!is_local(""));
    }
}
