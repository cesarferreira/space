use super::{Discovery, FilesystemStats, RawMount};
use anyhow::{Context, Result};
use plist::Value;
use std::{collections::BTreeMap, ffi::CStr, io::Cursor, path::PathBuf, process::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApfsVolumeInfo {
    pub backing_id: String,
    pub roles: Vec<String>,
    pub name: Option<String>,
}

pub fn parse_apfs_list(input: &[u8]) -> Result<BTreeMap<String, ApfsVolumeInfo>> {
    let value = Value::from_reader(Cursor::new(input)).context("parse diskutil APFS plist")?;
    let containers = value
        .as_dictionary()
        .and_then(|root| root.get("Containers"))
        .and_then(Value::as_array)
        .context("diskutil plist has no Containers array")?;
    let mut result = BTreeMap::new();

    for container in containers {
        let Some(container) = container.as_dictionary() else {
            continue;
        };
        let identity = container
            .get("APFSContainerUUID")
            .and_then(Value::as_string)
            .or_else(|| {
                container
                    .get("ContainerReference")
                    .and_then(Value::as_string)
            });
        let Some(identity) = identity else { continue };
        let Some(volumes) = container.get("Volumes").and_then(Value::as_array) else {
            continue;
        };

        for volume in volumes {
            let Some(volume) = volume.as_dictionary() else {
                continue;
            };
            let Some(device) = volume.get("DeviceIdentifier").and_then(Value::as_string) else {
                continue;
            };
            let roles = volume
                .get("Roles")
                .and_then(Value::as_array)
                .map(|roles| {
                    roles
                        .iter()
                        .filter_map(Value::as_string)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            result.insert(
                device.to_owned(),
                ApfsVolumeInfo {
                    backing_id: format!("apfs:{identity}"),
                    roles,
                    name: volume
                        .get("Name")
                        .and_then(Value::as_string)
                        .map(str::to_owned),
                },
            );
        }
    }
    Ok(result)
}

fn apfs_volumes() -> Result<BTreeMap<String, ApfsVolumeInfo>> {
    let output = Command::new("diskutil")
        .args(["apfs", "list", "-plist"])
        .output()
        .context("run diskutil")?;
    if !output.status.success() {
        anyhow::bail!("diskutil exited with {}", output.status)
    }
    parse_apfs_list(&output.stdout)
}

fn field(value: &[libc::c_char]) -> String {
    unsafe { CStr::from_ptr(value.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn apfs_info<'a>(
    device: &str,
    volumes: &'a BTreeMap<String, ApfsVolumeInfo>,
) -> Option<&'a ApfsVolumeInfo> {
    volumes.get(device).or_else(|| {
        volumes.iter().find_map(|(candidate, info)| {
            device
                .strip_prefix(candidate)
                .filter(|suffix| {
                    suffix.strip_prefix('s').is_some_and(|digits| {
                        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
                    })
                })
                .map(|_| info)
        })
    })
}

pub fn discover() -> Result<Discovery> {
    let mut buffer = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut buffer, libc::MNT_NOWAIT) };
    if count <= 0 || buffer.is_null() {
        return Err(std::io::Error::last_os_error()).context("read macOS mount table");
    }
    let entries = unsafe { std::slice::from_raw_parts(buffer, count as usize) };
    let enrichment = apfs_volumes();
    let apfs = enrichment.as_ref().ok();
    let mut mounts = Vec::with_capacity(entries.len());

    for (index, entry) in entries.iter().enumerate() {
        let source = field(&entry.f_mntfromname);
        let filesystem = field(&entry.f_fstypename);
        let target = PathBuf::from(field(&entry.f_mntonname));
        let block_size = u64::from(entry.f_bsize);
        let total_bytes = entry.f_blocks.saturating_mul(block_size);
        let free_bytes = entry.f_bfree.saturating_mul(block_size);
        let available_bytes = entry.f_bavail.saturating_mul(block_size);
        let device = source.strip_prefix("/dev/").unwrap_or(&source);
        let volume_info = apfs.and_then(|volumes| apfs_info(device, volumes));
        let backing_id = volume_info
            .map(|info| info.backing_id.clone())
            .or_else(|| (!source.is_empty()).then(|| source.clone()));

        mounts.push(RawMount {
            mount_id: index as u64 + 1,
            parent_id: 0,
            device_id: device.to_owned(),
            root: "/".to_owned(),
            target,
            mount_options: Vec::new(),
            filesystem,
            source,
            super_options: Vec::new(),
            stats: Some(FilesystemStats {
                total_bytes,
                used_bytes: total_bytes.saturating_sub(free_bytes),
                available_bytes,
            }),
            stats_error: None,
            backing_id,
            volume_name: volume_info.and_then(|info| info.name.clone()),
            removable: false,
        });
    }

    let warnings = enrichment
        .err()
        .map(|error| vec![format!("APFS enrichment unavailable: {error}")])
        .unwrap_or_default();
    Ok(Discovery { mounts, warnings })
}
