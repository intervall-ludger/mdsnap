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

#[test]
fn rejects_path_traversal() {
    let tmp = std::env::temp_dir().join("mdsnap_traversal");
    let report_dir = tmp.join("report");
    let out = tmp.join("bundle");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&report_dir).unwrap();
    // a secret OUTSIDE the report directory
    std::fs::write(tmp.join("secret.txt"), "TOPSECRET").unwrap();
    std::fs::write(report_dir.join("report.md"), "![x](../secret.txt)\n").unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_mdsnap"))
        .arg(report_dir.join("report.md"))
        .arg("-o")
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success()); // escaping ref is skipped, not a hard failure

    // the secret must never be copied into the bundle
    assert!(!out.join("assets/secret.txt").exists());
    let assets = out.join("assets");
    if assets.exists() {
        assert_eq!(std::fs::read_dir(&assets).unwrap().count(), 0);
    }
}

#[test]
fn gate_blocks_uncommitted_asset() {
    let tmp = std::env::temp_dir().join("mdsnap_gate");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("img")).unwrap();
    run_git(&tmp, &["init", "-q"]);
    run_git(&tmp, &["config", "user.email", "t@t"]);
    run_git(&tmp, &["config", "user.name", "t"]);
    std::fs::write(tmp.join("report.md"), "![x](img/a.svg)\n").unwrap();
    run_git(&tmp, &["add", "report.md"]);
    run_git(&tmp, &["commit", "-qm", "init"]);
    // the asset exists but is untracked -> not captured by the commit
    std::fs::write(tmp.join("img/a.svg"), "<svg/>").unwrap();

    let out = tmp.join("bundle");
    let blocked = Command::new(env!("CARGO_BIN_EXE_mdsnap"))
        .arg(tmp.join("report.md"))
        .arg("-o")
        .arg(&out)
        .status()
        .unwrap();
    assert!(!blocked.success(), "gate must block an untracked asset");

    let forced = Command::new(env!("CARGO_BIN_EXE_mdsnap"))
        .arg(tmp.join("report.md"))
        .arg("-o")
        .arg(&out)
        .arg("--allow-dirty")
        .status()
        .unwrap();
    assert!(forced.success(), "--allow-dirty must override the gate");
    let snap = std::fs::read_to_string(out.join("snapshot.json")).unwrap();
    assert!(snap.contains("\"reproducible\": false"));
    assert!(snap.contains("untracked"));
}

fn run_git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}
