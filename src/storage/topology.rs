use crate::{
    model::{MountView, StorageReport, StorageVolume, Warning, used_percent},
    platform::{Discovery, RawMount},
};
use std::collections::BTreeMap;
const VIRTUAL: &[&str] = &[
    "proc",
    "sysfs",
    "tmpfs",
    "devtmpfs",
    "devpts",
    "cgroup",
    "cgroup2",
    "securityfs",
    "overlay",
    "squashfs",
    "tracefs",
    "debugfs",
    "pstore",
    "efivarfs",
    "devfs",
    "configfs",
    "mqueue",
    "bpf",
    "autofs",
    "hugetlbfs",
    "fusectl",
    "binfmt_misc",
    "nsfs",
    "rpc_pipefs",
];
pub fn hidden_reason(m: &RawMount) -> Option<&'static str> {
    let target = m.target.to_string_lossy();
    if (m.target.starts_with("/System/Volumes")
        && m.target != std::path::Path::new("/System/Volumes/Data"))
        || target.contains("/com.apple.security.cryptexd/mnt/")
        || target.contains("/AppTranslocation/")
    {
        Some("macOS system-derived mount")
    } else if m.filesystem == "overlay" {
        Some("container overlay")
    } else if VIRTUAL.contains(&m.filesystem.as_str()) {
        Some("virtual filesystem")
    } else {
        None
    }
}

fn hidden_reason_for(m: &RawMount, container: bool) -> Option<&'static str> {
    if container && (m.target == std::path::Path::new("/boot") || m.target.starts_with("/boot/")) {
        return Some("system partition");
    }
    if m.target.to_string_lossy().contains("azure_session_dir") {
        return Some("temporary session mount");
    }
    hidden_reason(m)
}

fn volume_name(mount: &RawMount, container: bool) -> String {
    let target = mount.target.as_path();
    if !container && let Some(name) = &mount.volume_name {
        return name.clone();
    }
    if target == std::path::Path::new("/") {
        return if container {
            "Container root"
        } else {
            "Main disk"
        }
        .into();
    }
    if target.starts_with("/home/") {
        return "Home".into();
    }
    if target == std::path::Path::new("/workspace") {
        return "Workspace".into();
    }
    if target == std::path::Path::new("/mnt") {
        return "Mounted data".into();
    }
    target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into()
}
fn view(m: &RawMount, reason: Option<&str>) -> MountView {
    let s = m.stats.clone().unwrap_or(crate::platform::FilesystemStats {
        total_bytes: 0,
        used_bytes: 0,
        available_bytes: 0,
    });
    MountView {
        source: m.source.clone(),
        target: m.target.clone(),
        filesystem: m.filesystem.clone(),
        device_id: Some(m.device_id.clone()),
        backing_storage_id: m.backing_id.clone(),
        independent: reason.is_none(),
        hidden_reason: reason.map(Into::into),
        total_bytes: s.total_bytes,
        used_bytes: s.used_bytes,
        available_bytes: s.available_bytes,
    }
}
pub fn normalize(d: Discovery) -> StorageReport {
    let marker = std::path::Path::new("/.dockerenv").exists()
        || std::path::Path::new("/run/.containerenv").exists();
    let container = is_container_environment(&d, marker);
    normalize_for_environment(d, container)
}

pub fn is_container_environment(discovery: &Discovery, marker: bool) -> bool {
    if marker {
        return true;
    }
    let has = |target: &str| {
        discovery
            .mounts
            .iter()
            .any(|mount| mount.target == std::path::Path::new(target))
    };
    let azure_session = discovery
        .mounts
        .iter()
        .any(|mount| mount.target.to_string_lossy().contains("azure_session_dir"));
    azure_session
        || (has("/")
            && has("/workspace")
            && discovery
                .mounts
                .iter()
                .any(|mount| mount.target.starts_with("/home/")))
}

pub fn normalize_for_environment(d: Discovery, container: bool) -> StorageReport {
    let count = d.mounts.len();
    let mut hidden = vec![];
    let mut groups: BTreeMap<String, Vec<RawMount>> = BTreeMap::new();
    let mut errors = 0;
    for m in d.mounts {
        if m.stats_error.is_some() {
            errors += 1
        }
        if let Some(r) = hidden_reason_for(&m, container) {
            hidden.push(view(&m, Some(r)))
        } else {
            let id = m.backing_id.clone().unwrap_or_else(|| {
                if !m.device_id.is_empty() {
                    m.device_id.clone()
                } else {
                    m.source.clone()
                }
            });
            groups.entry(id).or_default().push(m)
        }
    }
    let mut storage = vec![];
    for (id, mut ms) in groups {
        ms.sort_by_key(|m| {
            (
                m.target != std::path::Path::new("/"),
                m.target.components().count(),
            )
        });
        let m = &ms[0];
        let s = m.stats.clone().unwrap_or(crate::platform::FilesystemStats {
            total_bytes: 0,
            used_bytes: 0,
            available_bytes: 0,
        });
        storage.push(StorageVolume {
            id: id.clone(),
            name: volume_name(m, container),
            filesystem: m.filesystem.clone(),
            representative_mount: m.target.clone(),
            total_bytes: s.total_bytes,
            used_bytes: s.used_bytes,
            available_bytes: s.available_bytes,
            reserved_bytes: Some(
                s.total_bytes
                    .saturating_sub(s.used_bytes)
                    .saturating_sub(s.available_bytes),
            ),
            used_percent: used_percent(s.used_bytes, s.total_bytes),
            physical: true,
            removable: m.removable,
            categories: vec![],
            mounts: ms.iter().map(|x| x.target.clone()).collect(),
        })
    }
    storage.sort_by_key(|volume| {
        if !container && volume.representative_mount == std::path::Path::new("/") {
            return 0;
        }
        match volume.name.as_str() {
            "Home" => 0,
            "Workspace" => 1,
            "Container root" | "Main disk" => 2,
            "Mounted data" => 3,
            _ => 4,
        }
    });
    let n = storage.len();
    StorageReport {
        schema_version: 1,
        storage,
        hidden_mounts: hidden,
        mount_count: count,
        independent_storage_count: n,
        inaccessible_paths: 0,
        warnings: if errors > 0 {
            vec![Warning {
                kind: "mount_stats".into(),
                count: errors,
                message: "Some mounts could not be inspected".into(),
            }]
        } else {
            vec![]
        },
    }
}
