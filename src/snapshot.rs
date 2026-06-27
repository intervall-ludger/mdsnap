use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub source: String,
    pub created_at: String,
    /// true when every asset's exact bytes are captured by the recorded commit
    pub reproducible: bool,
    pub assets: Vec<AssetEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitMeta>,
}

#[derive(Serialize, Deserialize)]
pub struct AssetEntry {
    pub bundled: String,
    /// per-asset git status: clean / modified / untracked / outside-repo
    pub git_status: String,
    /// SHA-256 of the bundled file, for integrity verification
    pub sha256: String,
    /// how the asset is produced: external / generated (heuristic)
    pub provenance: String,
    /// repo-relative path of the python script that generates the asset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    /// the generator changed after the image was last committed (or is dirty)
    #[serde(default, skip_serializing_if = "is_false")]
    pub generator_stale: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Serialize, Deserialize)]
pub struct GitMeta {
    pub commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// true when the working tree had uncommitted changes: the commit alone does
    /// not reproduce this bundle, see `diff_file`.
    pub dirty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_file: Option<String>,
}
