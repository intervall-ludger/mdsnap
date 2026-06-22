use anyhow::Result;
use git2::{DiffFormat, Repository, StatusOptions};
use std::path::Path;

pub struct GitInfo {
    pub commit: String,
    pub branch: Option<String>,
    pub dirty: bool,
    pub remote_url: Option<String>,
}

/// Inspect the git repository containing `start`. Returns `None` if there is no
/// repository (the bundle is then simply not reproducible via git).
pub fn inspect(start: &Path) -> Result<Option<GitInfo>> {
    let repo = match Repository::discover(start) {
        Ok(repo) => repo,
        Err(_) => return Ok(None),
    };
    let head = match repo.head() {
        Ok(head) => head,
        Err(_) => return Ok(None), // unborn branch: repo without any commit yet
    };
    let commit = head.peel_to_commit()?.id().to_string();
    let branch = head.shorthand().map(str::to_string);
    let mut options = StatusOptions::new();
    options.include_untracked(true);
    let dirty = !repo.statuses(Some(&mut options))?.is_empty();
    let remote_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(str::to_string));
    Ok(Some(GitInfo {
        commit,
        branch,
        dirty,
        remote_url,
    }))
}

/// The uncommitted changes (working tree + index vs HEAD) as a unified diff.
pub fn diff(start: &Path) -> Result<Option<String>> {
    let repo = match Repository::discover(start) {
        Ok(repo) => repo,
        Err(_) => return Ok(None),
    };
    let head_tree = match repo.head() {
        Ok(head) => head.peel_to_tree()?,
        Err(_) => return Ok(None),
    };
    let mut options = git2::DiffOptions::new();
    options.include_untracked(true);
    options.show_untracked_content(true);
    options.show_binary(true);
    let diff = repo.diff_tree_to_workdir_with_index(Some(&head_tree), Some(&mut options))?;
    let mut buffer = String::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        if matches!(line.origin(), '+' | '-' | ' ') {
            buffer.push(line.origin());
        }
        buffer.push_str(&String::from_utf8_lossy(line.content()));
        true
    })?;
    Ok(Some(buffer))
}

/// Git status of a single asset relative to HEAD. This per-asset signal is what
/// the reproducibility gate uses, not the repo-wide dirty flag (which trips on
/// any unrelated change).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AssetStatus {
    Clean,
    Modified,
    Untracked,
    OutsideRepo,
}

impl AssetStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AssetStatus::Clean => "clean",
            AssetStatus::Modified => "modified",
            AssetStatus::Untracked => "untracked",
            AssetStatus::OutsideRepo => "outside-repo",
        }
    }

    /// Whether the asset is not captured by the commit (modified or untracked).
    /// `clean` and `outside-repo` are not blocking.
    pub fn is_uncommitted(self) -> bool {
        matches!(self, AssetStatus::Modified | AssetStatus::Untracked)
    }
}

/// Status of `asset` (an absolute path) in the repo containing `start`.
pub fn asset_status(start: &Path, asset: &Path) -> AssetStatus {
    let Ok(repo) = Repository::discover(start) else {
        return AssetStatus::OutsideRepo;
    };
    let Some(workdir) = repo.workdir().and_then(|w| w.canonicalize().ok()) else {
        return AssetStatus::OutsideRepo;
    };
    let Ok(rel) = asset.strip_prefix(&workdir) else {
        return AssetStatus::OutsideRepo;
    };
    match repo.status_file(rel) {
        Ok(status) if status.is_empty() => AssetStatus::Clean,
        Ok(status) if status.contains(git2::Status::WT_NEW) => AssetStatus::Untracked,
        Ok(_) => AssetStatus::Modified,
        Err(_) => AssetStatus::OutsideRepo,
    }
}
