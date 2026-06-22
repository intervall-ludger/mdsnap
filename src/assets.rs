use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;

pub struct CopiedAsset {
    /// the original reference as written in the markdown
    pub original: String,
    /// the new relative path inside the bundle, e.g. "assets/plot.png"
    pub bundled: String,
}

/// Copy every referenced file that exists into `assets_dir`, returning the
/// mapping from original reference to bundled path. Missing files are warned
/// about and skipped. Name collisions get a numeric suffix.
pub fn copy_assets(refs: &[String], md_dir: &Path, assets_dir: &Path) -> Result<Vec<CopiedAsset>> {
    let mut copied = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();
    for reference in refs {
        let source = md_dir.join(reference);
        if !source.exists() {
            eprintln!("warning: referenced file not found, skipping: {reference}");
            continue;
        }
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
        while !used_names.insert(name.clone()) {
            name = format!("{stem}-{counter}{ext}");
            counter += 1;
        }
        std::fs::create_dir_all(assets_dir)?;
        std::fs::copy(&source, assets_dir.join(&name))
            .with_context(|| format!("copying {}", source.display()))?;
        copied.push(CopiedAsset {
            original: reference.clone(),
            bundled: format!("assets/{name}"),
        });
    }
    Ok(copied)
}
