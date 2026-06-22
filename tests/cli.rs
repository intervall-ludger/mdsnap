use std::process::Command;

#[test]
fn bundles_markdown_and_html_images() {
    let tmp = std::env::temp_dir().join("mdsnap_integration");
    let src = tmp.join("src");
    let out = tmp.join("bundle");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(src.join("img")).unwrap();
    std::fs::write(src.join("img/a.svg"), "<svg/>").unwrap();
    std::fs::write(src.join("img/b.svg"), "<svg/>").unwrap();
    std::fs::write(
        src.join("report.md"),
        "![chart](img/a.svg)\n<img src=\"img/b.svg\"/>\n[ext](https://e.com)\n",
    )
    .unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_mdsnap"))
        .arg(src.join("report.md"))
        .arg("-o")
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success());

    // both the markdown image and the HTML <img> were copied
    assert!(out.join("assets/a.svg").exists());
    assert!(out.join("assets/b.svg").exists());

    // paths rewritten to assets/, external link left untouched
    let md = std::fs::read_to_string(out.join("report.md")).unwrap();
    assert!(md.contains("](assets/a.svg)"));
    assert!(md.contains("src=\"assets/b.svg\""));
    assert!(md.contains("https://e.com"));

    assert!(out.join("snapshot.json").exists());
}
