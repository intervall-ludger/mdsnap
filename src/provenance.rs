use crate::assets::CopiedAsset;
use crate::git_info;
use git2::{DiffOptions, Repository, Sort};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Where an asset comes from and whether the recorded commit can still produce
/// it. This is a heuristic, not a proof: we grep the python sources for the
/// asset's file name, so dynamic names (f-strings, loops) are missed.
#[derive(Clone)]
pub struct Provenance {
    pub kind: Kind,
    /// repo-relative path of the python script that mentions the asset
    pub generator: Option<String>,
    /// the generator changed after the image was last committed (or is dirty)
    pub stale: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    External,
    Generated,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::External => "external",
            Kind::Generated => "generated",
        }
    }
}

const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    "node_modules",
    "target",
    ".mypy_cache",
    ".pytest_cache",
    "site-packages",
];

/// One provenance entry per copied asset, in the same order. Without a git repo
/// every asset is reported as external (provenance needs history to reason).
pub fn analyze(md_dir: &Path, assets: &[CopiedAsset]) -> Vec<Provenance> {
    let external = || Provenance {
        kind: Kind::External,
        generator: None,
        stale: false,
        reason: None,
    };
    let Ok(repo) = Repository::discover(md_dir) else {
        return assets.iter().map(|_| external()).collect();
    };
    let Some(workdir) = repo.workdir().and_then(|w| w.canonicalize().ok()) else {
        return assets.iter().map(|_| external()).collect();
    };

    let basenames: HashSet<String> = assets
        .iter()
        .filter_map(|asset| {
            asset
                .source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .collect();
    let mentions = scan_python(&workdir, &basenames);

    let mut cache: HashMap<PathBuf, Provenance> = HashMap::new();
    assets
        .iter()
        .map(|asset| {
            cache
                .entry(asset.source.clone())
                .or_insert_with(|| classify(&repo, &workdir, &asset.source, &mentions))
                .clone()
        })
        .collect()
}

fn classify(
    repo: &Repository,
    workdir: &Path,
    source: &Path,
    mentions: &HashMap<String, Vec<PathBuf>>,
) -> Provenance {
    let external = Provenance {
        kind: Kind::External,
        generator: None,
        stale: false,
        reason: None,
    };
    let Some(name) = source.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return external;
    };
    let scripts = match mentions.get(&name) {
        Some(scripts) if !scripts.is_empty() => scripts,
        _ => return external,
    };

    let asset_time = source
        .strip_prefix(workdir)
        .ok()
        .and_then(|rel| last_commit_time(repo, rel));
    let mut first_generator = None;
    for script in scripts {
        let rel = script.strip_prefix(workdir).unwrap_or(script);
        let rel_str = rel.to_string_lossy().into_owned();
        first_generator.get_or_insert_with(|| rel_str.clone());

        if git_info::asset_status(workdir, script).is_uncommitted() {
            return generated(
                rel_str.clone(),
                format!("{rel_str} has uncommitted changes"),
            );
        }
        if let (Some(script_time), Some(asset_time)) = (last_commit_time(repo, rel), asset_time) {
            if script_time > asset_time {
                return generated(
                    rel_str.clone(),
                    format!(
                        "{rel_str} changed on {} but the image dates to {}",
                        fmt_date(script_time),
                        fmt_date(asset_time)
                    ),
                );
            }
        }
    }
    Provenance {
        kind: Kind::Generated,
        generator: first_generator,
        stale: false,
        reason: None,
    }
}

fn generated(generator: String, reason: String) -> Provenance {
    Provenance {
        kind: Kind::Generated,
        generator: Some(generator),
        stale: true,
        reason: Some(reason),
    }
}

/// Walk the work tree once and record, per asset name, which python files
/// mention it as a literal substring.
fn scan_python(root: &Path, basenames: &HashSet<String>) -> HashMap<String, Vec<PathBuf>> {
    let mut map: HashMap<String, Vec<PathBuf>> = HashMap::new();
    if basenames.is_empty() {
        return map;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let excluded = path
                    .file_name()
                    .is_some_and(|name| EXCLUDED_DIRS.contains(&&*name.to_string_lossy()));
                if !excluded {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "py") {
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for name in basenames {
                    if content.contains(name.as_str()) {
                        map.entry(name.clone()).or_default().push(path.clone());
                    }
                }
            }
        }
    }
    map
}

/// Unix time of the most recent commit that touched `rel` (repo-relative path).
fn last_commit_time(repo: &Repository, rel: &Path) -> Option<i64> {
    let mut walk = repo.revwalk().ok()?;
    walk.push_head().ok()?;
    walk.set_sorting(Sort::TIME).ok()?;
    for oid in walk {
        let commit = repo.find_commit(oid.ok()?).ok()?;
        if commit_touches(repo, &commit, rel) {
            return Some(commit.time().seconds());
        }
    }
    None
}

fn commit_touches(repo: &Repository, commit: &git2::Commit, rel: &Path) -> bool {
    let Ok(tree) = commit.tree() else {
        return false;
    };
    let parent_tree = commit.parent(0).ok().and_then(|parent| parent.tree().ok());
    let mut options = DiffOptions::new();
    options.pathspec(rel);
    match repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut options)) {
        Ok(diff) => diff.deltas().len() > 0,
        Err(_) => false,
    }
}

fn fmt_date(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| seconds.to_string())
}
