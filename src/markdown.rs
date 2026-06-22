use pulldown_cmark::{Event, Parser, Tag};
use std::ops::Range;

/// A local asset reference: the on-disk path to copy and the byte span of the
/// destination text in the original markdown, so it can be rewritten in place
/// (without ever touching code blocks or prose, unlike a global string replace).
pub struct Reference {
    /// resolved path: percent-decoded, without #fragment / ?query
    pub path: String,
    /// byte range of the raw destination in the source content
    pub span: Range<usize>,
}

/// Find every local reference (markdown image/link destinations and HTML `src`)
/// with the byte span of its destination, in document order.
pub fn find_refs(content: &str) -> Vec<Reference> {
    let mut refs = Vec::new();
    for (event, range) in Parser::new(content).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { dest_url, .. })
            | Event::Start(Tag::Link { dest_url, .. }) => {
                if is_local(&dest_url) {
                    if let Some(span) = locate(content, &range, &dest_url) {
                        refs.push(Reference {
                            path: clean_path(&dest_url),
                            span,
                        });
                    }
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => locate_html_srcs(content, &range, &mut refs),
            _ => {}
        }
    }
    refs
}

/// Apply (span, replacement) edits to the content. Edits are applied
/// right-to-left so earlier byte offsets stay valid.
pub fn apply_rewrites(content: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by(|a, b| b.0.start.cmp(&a.0.start));
    let mut out = content.to_string();
    for (range, replacement) in edits {
        out.replace_range(range, &replacement);
    }
    out
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

/// Byte span of `needle` inside `range`, preferring the last occurrence (the
/// destination comes after the link text).
fn locate(content: &str, range: &Range<usize>, needle: &str) -> Option<Range<usize>> {
    let slice = content.get(range.clone())?;
    let pos = slice.rfind(needle)?;
    let start = range.start + pos;
    Some(start..start + needle.len())
}

/// Collect `src="..."` / `src='...'` references (with spans) inside an HTML range.
fn locate_html_srcs(content: &str, range: &Range<usize>, refs: &mut Vec<Reference>) {
    let Some(slice) = content.get(range.clone()) else {
        return;
    };
    let mut offset = 0;
    while let Some(idx) = slice[offset..].find("src=") {
        let after_eq = offset + idx + 4;
        let rest = &slice[after_eq..];
        let quote = rest.chars().next();
        if let Some(quote @ ('"' | '\'')) = quote {
            if let Some(end) = rest[1..].find(quote) {
                let url = &rest[1..1 + end];
                if is_local(url) {
                    let start = range.start + after_eq + 1;
                    refs.push(Reference {
                        path: clean_path(url),
                        span: start..start + url.len(),
                    });
                }
                offset = after_eq + 1 + end;
                continue;
            }
        }
        offset = after_eq;
    }
}

/// Strip a `#fragment` / `?query` and percent-decode, giving the on-disk path.
fn clean_path(url: &str) -> String {
    let stem = url.split(['#', '?']).next().unwrap_or(url);
    percent_decode(stem)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(byte) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(content: &str) -> Vec<String> {
        find_refs(content).into_iter().map(|r| r.path).collect()
    }

    #[test]
    fn finds_markdown_image() {
        let content = "![alt](plots/chart.png)";
        let refs = find_refs(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].path, "plots/chart.png");
        assert_eq!(&content[refs[0].span.clone()], "plots/chart.png");
    }

    #[test]
    fn finds_html_img_src() {
        assert_eq!(
            paths("<img src=\"plots/diagram.png\" width=\"50%\"/>"),
            vec!["plots/diagram.png".to_string()]
        );
        assert_eq!(paths("<img src='a.png'>"), vec!["a.png".to_string()]);
    }

    #[test]
    fn skips_external_and_anchors() {
        assert!(paths("![x](https://e.com/a.png) [y](#sec) <img src=\"data:img\">").is_empty());
    }

    #[test]
    fn decodes_and_strips_fragment() {
        assert_eq!(
            paths("![a](my%20plot.png)"),
            vec!["my plot.png".to_string()]
        );
        assert_eq!(paths("[a](doc.pdf#page=3)"), vec!["doc.pdf".to_string()]);
    }

    #[test]
    fn does_not_touch_code_blocks() {
        // a path inside inline code must not be rewritten
        let content = "real ![a](a.png) and code `![a](a.png)`";
        let refs = find_refs(content);
        assert_eq!(refs.len(), 1); // only the real image, not the one in backticks
    }

    #[test]
    fn rewrites_only_destination_spans() {
        let content = "![a](old.png) <img src=\"old.png\"/>";
        let edits = find_refs(content)
            .into_iter()
            .map(|r| (r.span, "assets/old.png".to_string()))
            .collect();
        assert_eq!(
            apply_rewrites(content, edits),
            "![a](assets/old.png) <img src=\"assets/old.png\"/>"
        );
    }

    #[test]
    fn is_local_classifies() {
        assert!(is_local("plots/a.png"));
        assert!(is_local("./a.png"));
        assert!(is_local("../up.png"));
        assert!(!is_local("https://x.com/a.png"));
        assert!(!is_local("data:image/png;base64,AAAA"));
        assert!(!is_local("file:///etc/passwd"));
        assert!(!is_local("mailto:x@y.com"));
        assert!(!is_local("#anchor"));
        assert!(!is_local(""));
    }
}
