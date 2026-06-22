mod assets;
mod git_info;
mod markdown;
mod snapshot;

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "mdsnap",
    version,
    about = "Bundle a Markdown report with its assets and a reproducible git snapshot"
)]
struct Cli {
    /// the markdown file to snapshot
    input: PathBuf,
    /// target directory for the bundle
    #[arg(short, long)]
    out: PathBuf,
    /// also store the uncommitted diff (diff.patch) for full reproducibility
    #[arg(long)]
    diff: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if !cli.input.exists() {
        bail!("input markdown not found: {}", cli.input.display());
    }
    let content = std::fs::read_to_string(&cli.input)
        .with_context(|| format!("reading {}", cli.input.display()))?;
    let md_dir = match cli.input.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let report_name = cli
        .input
        .file_name()
        .context("input has no file name")?
        .to_os_string();

    std::fs::create_dir_all(&cli.out).with_context(|| format!("creating {}", cli.out.display()))?;

    // 1. copy referenced assets into the bundle
    let refs = markdown::extract_local_refs(&content);
    let copied = assets::copy_assets(&refs, &md_dir, &cli.out.join("assets"))?;

    // 2. rewrite the markdown to point at the bundled assets
    let mut rewritten = content.clone();
    for asset in &copied {
        rewritten = markdown::rewrite_ref(&rewritten, &asset.original, &asset.bundled);
    }
    std::fs::write(cli.out.join(&report_name), &rewritten)?;

    // 3. git snapshot (+ optional diff when dirty)
    let git_meta = match git_info::inspect(&md_dir)? {
        Some(info) => {
            let diff_file = if cli.diff && info.dirty {
                match git_info::diff(&md_dir)? {
                    Some(patch) => {
                        std::fs::write(cli.out.join("diff.patch"), patch)?;
                        Some("diff.patch".to_string())
                    }
                    None => None,
                }
            } else {
                None
            };
            Some(snapshot::GitMeta {
                commit: info.commit,
                branch: info.branch,
                dirty: info.dirty,
                remote_url: info.remote_url,
                diff_file,
            })
        }
        None => None,
    };

    // 4. write snapshot.json
    let snap = snapshot::Snapshot {
        source: cli.input.display().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        assets: copied.iter().map(|asset| asset.bundled.clone()).collect(),
        git: git_meta,
    };
    std::fs::write(
        cli.out.join("snapshot.json"),
        serde_json::to_string_pretty(&snap)?,
    )?;

    print_summary(&cli, &copied, &snap);
    Ok(())
}

fn print_summary(cli: &Cli, copied: &[assets::CopiedAsset], snap: &snapshot::Snapshot) {
    println!("bundled {} -> {}", cli.input.display(), cli.out.display());
    println!("  {} asset(s)", copied.len());
    match &snap.git {
        Some(git) => {
            let short = &git.commit[..git.commit.len().min(12)];
            let dirty = if git.dirty { " (dirty)" } else { "" };
            println!("  commit {short}{dirty}");
            if git.dirty && !cli.diff {
                println!("  note: working tree is dirty; re-run with --diff to capture changes");
            }
        }
        None => println!("  no git repo found (not reproducible via commit)"),
    }
}
