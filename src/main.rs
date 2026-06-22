mod assets;
mod git_info;
mod markdown;
mod snapshot;

use anyhow::{bail, Context, Result};
use clap::Parser;
use git_info::AssetStatus;
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
    /// also store the uncommitted diff (diff.patch)
    #[arg(long)]
    diff: bool,
    /// bundle even when referenced assets are uncommitted (not reproducible)
    #[arg(long)]
    allow_dirty: bool,
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
    let refs = markdown::find_refs(&content);
    let copied = assets::copy_assets(&refs, &md_dir, &cli.out.join("assets"))?;

    // 2. per-asset git status + reproducibility gate
    let statuses: Vec<AssetStatus> = copied
        .iter()
        .map(|asset| git_info::asset_status(&md_dir, &asset.source))
        .collect();
    let reproducible = !statuses.iter().any(|status| status.is_uncommitted());
    if !reproducible && !cli.allow_dirty {
        eprintln!("error: referenced asset(s) are not captured by the commit:");
        for (asset, status) in copied.iter().zip(&statuses) {
            if status.is_uncommitted() {
                eprintln!("  {} ({})", asset.original, status.as_str());
            }
        }
        bail!("bundle would not be reproducible; commit the assets or re-run with --allow-dirty");
    }

    // 3. rewrite the markdown to point at the bundled assets (in place, by span)
    let edits = copied
        .iter()
        .map(|asset| (asset.span.clone(), asset.bundled.clone()))
        .collect();
    let rewritten = markdown::apply_rewrites(&content, edits);
    std::fs::write(cli.out.join(&report_name), &rewritten)?;

    // 4. git snapshot (+ optional diff when dirty)
    let git_meta = match git_info::inspect(&md_dir)? {
        Some(info) => {
            let diff_file = if cli.diff && info.dirty {
                match git_info::diff(&md_dir)? {
                    Some(patch) => {
                        std::fs::write(cli.out.join("diff.patch"), patch)?;
                        eprintln!(
                            "warning: diff.patch captures all uncommitted changes \
                             (incl. untracked); review before sharing, it may hold secrets"
                        );
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

    // 5. write snapshot.json (one entry per bundled asset, deduplicated)
    let mut seen = std::collections::HashSet::new();
    let asset_entries: Vec<snapshot::AssetEntry> = copied
        .iter()
        .zip(&statuses)
        .filter(|(asset, _)| seen.insert(asset.bundled.clone()))
        .map(|(asset, status)| snapshot::AssetEntry {
            bundled: asset.bundled.clone(),
            git_status: status.as_str().to_string(),
        })
        .collect();
    let snap = snapshot::Snapshot {
        source: cli.input.display().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        reproducible,
        assets: asset_entries,
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
    println!(
        "  reproducible: {}",
        if snap.reproducible { "yes" } else { "no" }
    );
    match &snap.git {
        Some(git) => {
            let short = git.commit.get(..12).unwrap_or(&git.commit);
            let dirty = if git.dirty { " (dirty)" } else { "" };
            println!("  commit {short}{dirty}");
            if git.dirty && !cli.diff {
                println!("  note: working tree is dirty; re-run with --diff to capture changes");
            }
        }
        None => println!("  no git repo found (not reproducible via commit)"),
    }
}
