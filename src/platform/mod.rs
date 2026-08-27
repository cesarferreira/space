use anyhow::Result;
use std::path::PathBuf;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMount {
    pub mount_id: u64,
    pub parent_id: u64,
    pub device_id: String,
    pub root: String,
    pub target: PathBuf,
    pub mount_options: Vec<String>,
    pub filesystem: String,
    pub source: String,
    pub super_options: Vec<String>,
    pub stats: Option<FilesystemStats>,
    pub stats_error: Option<String>,
    pub backing_id: Option<String>,
    pub volume_name: Option<String>,
    pub removable: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    pub mounts: Vec<RawMount>,
    pub warnings: Vec<String>,
}
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub fn discover() -> Result<Discovery> {
    linux::discover()
}
#[cfg(target_os = "macos")]
pub fn discover() -> Result<Discovery> {
    macos::discover()
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn discover() -> Result<Discovery> {
    anyhow::bail!("unsupported operating system")
}
