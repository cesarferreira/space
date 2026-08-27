use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Exact,
    High,
    Medium,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reclaimability {
    Required,
    UserOwned,
    Cache,
    Generated,
    Temporary,
    Stale,
    PossiblyRemovable,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Estimate {
    pub bytes: u64,
    pub incomplete: bool,
    pub inaccessible_paths: usize,
}
impl Estimate {
    pub fn exact(bytes: u64) -> Self {
        Self {
            bytes,
            incomplete: false,
            inaccessible_paths: 0,
        }
    }
    pub fn lower_bound(bytes: u64, inaccessible_paths: usize) -> Self {
        Self {
            bytes,
            incomplete: true,
            inaccessible_paths,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticCategory {
    pub name: String,
    pub path: Vec<String>,
    pub estimate: Estimate,
    pub confidence: Confidence,
    pub reclaimability: Reclaimability,
    pub children: Vec<Self>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MountView {
    pub source: String,
    pub target: PathBuf,
    pub filesystem: String,
    pub device_id: Option<String>,
    pub backing_storage_id: Option<String>,
    pub independent: bool,
    pub hidden_reason: Option<String>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StorageVolume {
    pub id: String,
    pub name: String,
    pub filesystem: String,
    pub representative_mount: PathBuf,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub reserved_bytes: Option<u64>,
    pub used_percent: f64,
    pub physical: bool,
    pub removable: bool,
    pub categories: Vec<SemanticCategory>,
    pub mounts: Vec<PathBuf>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub kind: String,
    pub count: usize,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StorageReport {
    pub schema_version: u32,
    pub storage: Vec<StorageVolume>,
    pub hidden_mounts: Vec<MountView>,
    pub mount_count: usize,
    pub independent_storage_count: usize,
    pub inaccessible_paths: usize,
    pub warnings: Vec<Warning>,
}
impl StorageReport {
    pub fn empty() -> Self {
        Self {
            schema_version: 1,
            storage: vec![],
            hidden_mounts: vec![],
            mount_count: 0,
            independent_storage_count: 0,
            inaccessible_paths: 0,
            warnings: vec![],
        }
    }
}
pub fn used_percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}
