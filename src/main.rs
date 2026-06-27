mod assets;
mod git_info;
mod markdown;
mod provenance;
mod snapshot;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use git_info::AssetStatus;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "mdsnap",
    version,
    about = "Bundle a Markdown report with its assets and a reproducible git snapshot"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Bundle a markdown report with its assets and a git snapshot
    Snap {
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
        /// overwrite the output directory if it is not empty
        #[arg(short, long)]
        force: bool,
        /// fail if a referenced asset is missing
        #[arg(long)]
        strict: bool,
        /// write the bundle as a single <out>.zip and remove the directory
        #[arg(long)]
        zip: bool,
        /// fail if an image looks out of date with the python code that makes it
        #[arg(long)]
        strict_provenance: bool,
    },
    /// Verify a bundle's assets against its snapshot.json
    Verify {
        /// the bundle directory to verify
        bundle: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Snap {
            input,
            out,
            diff,
            allow_dirty,
            force,
            strict,
            zip,
            strict_provenance,
        } => snap(
            &input,
            &out,
            diff,
            allow_dirty,
            force,
            strict,
            zip,
            strict_provenance,
        ),
        Command::Verify { bundle } => verify(&bundle),
    }
}

#[allow(clippy::too_many_arguments)]
fn snap(
    input: &Path,
    out: &Path,
    diff: bool,
    allow_dirty: bool,
    force: bool,
    strict: bool,
    zip: bool,
    strict_provenance: bool,
) -> Result<()> {
    if !input.exists() {
        bail!("input markdown not found: {}", input.display());
    }
    let content =
        std::fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;
    let md_dir = match input.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let report_name = input
        .file_name()
        .context("input has no file name")?
        .to_os_string();

    // refuse a non-empty output directory unless --force (then start clean)
    let non_empty = out
        .read_dir()
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if non_empty {
        if force {
            std::fs::remove_dir_all(out)?;
        } else {
            bail!(
                "output directory {} is not empty; use --force to overwrite",
                out.display()
            );
        }
    }
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    // 1. copy referenced assets into the bundle
    let refs = markdown::find_refs(&content);
    let (copied, skipped) = assets::copy_assets(&refs, &md_dir, &out.join("assets"))?;
    if strict && !skipped.is_empty() {
        bail!(
            "{} referenced asset(s) missing (--strict): {skipped:?}",
            skipped.len()
        );
    }

    // 2. per-asset git status + reproducibility gate
    let statuses: Vec<AssetStatus> = copied
        .iter()
        .map(|asset| git_info::asset_status(&md_dir, &asset.source))
        .collect();
    let reproducible = !statuses.iter().any(|status| status.is_uncommitted());
    if !reproducible && !allow_dirty {
        eprintln!("error: referenced asset(s) are not captured by the commit:");
        for (asset, status) in copied.iter().zip(&statuses) {
            if status.is_uncommitted() {
                eprintln!("  {} ({})", asset.original, status.as_str());
            }
        }
        let _ = std::fs::remove_dir_all(out.join("assets")); // don't leave a half-bundle
        bail!("bundle would not be reproducible; commit the assets or re-run with --allow-dirty");
    }

    // 2b. provenance: does the recorded commit still produce each image?
    let provenances = provenance::analyze(&md_dir, &copied);
    let mut warned = std::collections::HashSet::new();
    let stale: Vec<_> = copied
        .iter()
        .zip(&provenances)
        .filter(|(asset, prov)| prov.stale && warned.insert(asset.bundled.clone()))
        .collect();
    if !stale.is_empty() {
        eprintln!("warning: image(s) may be out of date with the code that generates them:");
        for (asset, prov) in &stale {
            let reason = prov.reason.as_deref().unwrap_or("generator changed");
            eprintln!("  {} ({reason})", asset.original);
        }
        if strict_provenance {
            let _ = std::fs::remove_dir_all(out.join("assets"));
            bail!("{} image(s) look stale (--strict-provenance)", stale.len());
        }
    }

    // 3. rewrite the markdown to point at the bundled assets (in place, by span)
    let edits = copied
        .iter()
        .map(|asset| (asset.span.clone(), asset.bundled.clone()))
        .collect();
    let rewritten = markdown::apply_rewrites(&content, edits);
    std::fs::write(out.join(&report_name), &rewritten)?;

    // 4. git snapshot (+ optional diff when dirty)
    let git_meta = match git_info::inspect(&md_dir)? {
        Some(info) => {
            let diff_file = if diff && info.dirty {
                match git_info::diff(&md_dir)? {
                    Some(patch) => {
                        std::fs::write(out.join("diff.patch"), patch)?;
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
        .zip(&provenances)
        .filter(|((asset, _), _)| seen.insert(asset.bundled.clone()))
        .map(|((asset, status), prov)| snapshot::AssetEntry {
            bundled: asset.bundled.clone(),
            git_status: status.as_str().to_string(),
            sha256: asset.sha256.clone(),
            provenance: prov.kind.as_str().to_string(),
            generator: prov.generator.clone(),
            generator_stale: prov.stale,
        })
        .collect();
    let snap = snapshot::Snapshot {
        source: report_name.to_string_lossy().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        reproducible,
        assets: asset_entries,
        git: git_meta,
    };
    std::fs::write(
        out.join("snapshot.json"),
        serde_json::to_string_pretty(&snap)?,
    )?;

    print_summary(input, out, diff, &snap);
    if zip {
        let zip_path = out.with_extension("zip");
        zip_dir(out, &zip_path)?;
        std::fs::remove_dir_all(out)?;
        println!("  zipped -> {}", zip_path.display());
    }
    Ok(())
}

/// Write every file under `dir` into a single zip archive at `zip_path`.
fn zip_dir(dir: &Path, zip_path: &Path) -> Result<()> {
    use std::io::Write;
    let file = std::fs::File::create(zip_path)?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(dir)?.to_string_lossy().into_owned();
                writer.start_file(rel, options)?;
                writer.write_all(&std::fs::read(&path)?)?;
            }
        }
    }
    writer.finish()?;
    Ok(())
}

fn verify(bundle: &Path) -> Result<()> {
    let snapshot_path = bundle.join("snapshot.json");
    let content = std::fs::read_to_string(&snapshot_path)
        .with_context(|| format!("reading {}", snapshot_path.display()))?;
    let snap: snapshot::Snapshot = serde_json::from_str(&content)?;

    let mut problems = 0;
    for asset in &snap.assets {
        match assets::sha256_hex(&bundle.join(&asset.bundled)) {
            Ok(hash) if hash == asset.sha256 => {}
            Ok(_) => {
                eprintln!("CHANGED  {}", asset.bundled);
                problems += 1;
            }
            Err(_) => {
                eprintln!("MISSING  {}", asset.bundled);
                problems += 1;
            }
        }
    }
    if problems > 0 {
        bail!(
            "{problems} of {} asset(s) failed verification",
            snap.assets.len()
        );
    }
    println!("ok: {} asset(s) intact", snap.assets.len());
    if let Some(git) = &snap.git {
        let short = git.commit.get(..12).unwrap_or(&git.commit);
        println!("  commit {short}");
    }
    if !snap.reproducible {
        println!("  note: snapshot is marked not reproducible (assets were uncommitted)");
    }
    Ok(())
}

fn print_summary(input: &Path, out: &Path, diff: bool, snap: &snapshot::Snapshot) {
    println!("bundled {} -> {}", input.display(), out.display());
    println!("  {} asset(s)", snap.assets.len());
    println!(
        "  reproducible: {}",
        if snap.reproducible { "yes" } else { "no" }
    );
    match &snap.git {
        Some(git) => {
            let short = git.commit.get(..12).unwrap_or(&git.commit);
            let dirty = if git.dirty { " (dirty)" } else { "" };
            println!("  commit {short}{dirty}");
            if git.dirty && !diff {
                println!("  note: working tree is dirty; re-run with --diff to capture changes");
            }
        }
        None => println!("  no git repo found (not reproducible via commit)"),
    }
}
