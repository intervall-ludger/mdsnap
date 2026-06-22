use std::path::Path;
use std::process::Command;

fn mdsnap_snap(report: &Path, out: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mdsnap"));
    cmd.arg("snap").arg(report).arg("-o").arg(out);
    cmd
}

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

    assert!(mdsnap_snap(&src.join("report.md"), &out)
        .status()
        .unwrap()
        .success());

    assert!(out.join("assets/a.svg").exists());
    assert!(out.join("assets/b.svg").exists());
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
    std::fs::write(tmp.join("secret.txt"), "TOPSECRET").unwrap();
    std::fs::write(report_dir.join("report.md"), "![x](../secret.txt)\n").unwrap();

    assert!(mdsnap_snap(&report_dir.join("report.md"), &out)
        .status()
        .unwrap()
        .success());

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
    std::fs::write(tmp.join("img/a.svg"), "<svg/>").unwrap();

    let out = tmp.join("bundle");
    let blocked = mdsnap_snap(&tmp.join("report.md"), &out).status().unwrap();
    assert!(!blocked.success(), "gate must block an untracked asset");

    let forced = mdsnap_snap(&tmp.join("report.md"), &out)
        .arg("--allow-dirty")
        .status()
        .unwrap();
    assert!(forced.success(), "--allow-dirty must override the gate");
    let snap = std::fs::read_to_string(out.join("snapshot.json")).unwrap();
    assert!(snap.contains("\"reproducible\": false"));
    assert!(snap.contains("untracked"));
}

#[test]
fn verify_detects_tampering() {
    let tmp = std::env::temp_dir().join("mdsnap_verify");
    let src = tmp.join("src");
    let out = tmp.join("bundle");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(src.join("img")).unwrap();
    std::fs::write(src.join("img/a.svg"), "<svg/>").unwrap();
    std::fs::write(src.join("report.md"), "![x](img/a.svg)\n").unwrap();
    assert!(mdsnap_snap(&src.join("report.md"), &out)
        .status()
        .unwrap()
        .success());

    let intact = Command::new(env!("CARGO_BIN_EXE_mdsnap"))
        .arg("verify")
        .arg(&out)
        .status()
        .unwrap();
    assert!(intact.success(), "intact bundle must verify");

    std::fs::write(out.join("assets/a.svg"), "TAMPERED").unwrap();
    let tampered = Command::new(env!("CARGO_BIN_EXE_mdsnap"))
        .arg("verify")
        .arg(&out)
        .status()
        .unwrap();
    assert!(!tampered.success(), "verify must detect a changed asset");
}

fn run_git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}
