use crate::markdown::Reference;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

pub struct CopiedAsset {
    /// resolved path, for display / status messages
    pub original: String,
    /// byte span of the destination in the source content, for rewriting
    pub span: Range<usize>,
    /// the new relative path inside the bundle, e.g. "assets/plot.png"
    pub bundled: String,
    /// the resolved (canonical) source path, for git-status checks
    pub source: PathBuf,
}

/// Copy every referenced file into `assets_dir`, returning one entry per
/// reference (so each occurrence can be rewritten). The same source file is
/// copied once and shares a bundled name. References that are missing, escape
/// the report directory, or are not regular files are warned about and skipped.
pub fn copy_assets(
    refs: &[Reference],
    md_dir: &Path,
    assets_dir: &Path,
) -> Result<Vec<CopiedAsset>> {
    let base = md_dir
        .canonicalize()
        .unwrap_or_else(|_| md_dir.to_path_buf());
    let mut copied = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();
    let mut bundled_by_source: HashMap<PathBuf, String> = HashMap::new();
    for reference in refs {
        let Some(source) = resolve_within(&base, &reference.path) else {
            continue;
        };
        let bundled = match bundled_by_source.get(&source) {
            Some(existing) => existing.clone(),
            None => {
                let name = unique_name(&source, &mut used_names);
                std::fs::create_dir_all(assets_dir)?;
                std::fs::copy(&source, assets_dir.join(&name))
                    .with_context(|| format!("copying {}", source.display()))?;
                let bundled = format!("assets/{name}");
                bundled_by_source.insert(source.clone(), bundled.clone());
                bundled
            }
        };
        copied.push(CopiedAsset {
            original: reference.path.clone(),
            span: reference.span.clone(),
            bundled,
            source,
        });
    }
    Ok(copied)
}

fn unique_name(source: &Path, used: &mut HashSet<String>) -> String {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "asset".to_string());
    let ext = source
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut name = format!("{stem}{ext}");
    let mut counter = 1;
    while !used.insert(name.clone()) {
        name = format!("{stem}-{counter}{ext}");
        counter += 1;
    }
    name
}

/// Resolve a reference against `base` and ensure it is a regular file that stays
/// inside `base`. Canonicalizing collapses `..` and symlinks, so an escaping
/// reference (`../../etc/passwd`, an absolute path, or a symlink pointing out)
/// fails the containment check. Returns `None` (with a warning) otherwise.
fn resolve_within(base: &Path, reference: &str) -> Option<PathBuf> {
    let canonical = match base.join(reference).canonicalize() {
        Ok(path) => path,
        Err(_) => {
            eprintln!("warning: referenced file not found, skipping: {reference}");
            return None;
        }
    };
    if !canonical.starts_with(base) {
        eprintln!("warning: reference escapes the report directory, skipping: {reference}");
        return None;
    }
    match std::fs::metadata(&canonical) {
        Ok(meta) if meta.is_file() => Some(canonical),
        _ => {
            eprintln!("warning: not a regular file, skipping: {reference}");
            None
        }
    }
}
