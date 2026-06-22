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
    let diff = repo.diff_tree_to_workdir_with_index(Some(&head_tree), None)?;
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
