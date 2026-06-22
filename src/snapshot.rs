use serde::Serialize;

#[derive(Serialize)]
pub struct Snapshot {
    pub source: String,
    pub created_at: String,
    /// true when every asset's exact bytes are captured by the recorded commit
    pub reproducible: bool,
    pub assets: Vec<AssetEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitMeta>,
}

#[derive(Serialize)]
pub struct AssetEntry {
    pub bundled: String,
    /// per-asset git status: clean / modified / untracked / outside-repo
    pub git_status: String,
}

#[derive(Serialize)]
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
