//! Windows drive discovery: each lettered drive is a mount.

use super::{Discovery, FilesystemStats, RawMount};
use anyhow::Result;
use std::path::PathBuf;
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// The drive letters set in a `GetLogicalDrives` bit mask.
pub fn drive_letters(mask: u32) -> Vec<char> {
    (0..26u8)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| (b'A' + bit) as char)
        .collect()
}

pub fn discover() -> Result<Discovery> {
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut mounts = vec![];
    for (index, letter) in drive_letters(mask).into_iter().enumerate() {
        let root = format!("{letter}:\\");
        let root_w = wide(&root);
        let kind = unsafe { GetDriveTypeW(root_w.as_ptr()) };
        if !matches!(
            kind,
            DRIVE_REMOVABLE | DRIVE_FIXED | DRIVE_REMOTE | DRIVE_CDROM | DRIVE_RAMDISK
        ) {
            continue;
        }

        let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
        let stats_ok =
            unsafe { GetDiskFreeSpaceExW(root_w.as_ptr(), &mut available, &mut total, &mut free) }
                != 0;
        if !stats_ok && kind == DRIVE_CDROM {
            continue; // An empty optical drive.
        }

        let mut label = [0u16; 261];
        let mut fs_name = [0u16; 261];
        let mut serial = 0u32;
        let info_ok = unsafe {
            GetVolumeInformationW(
                root_w.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                &mut serial,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs_name.as_mut_ptr(),
                fs_name.len() as u32,
            )
        } != 0;
        let label = from_wide(&label);
        let filesystem = if info_ok {
            from_wide(&fs_name)
        } else {
            "unknown".into()
        };
        let name = if label.is_empty() {
            format!("Local Disk ({letter}:)")
        } else {
            format!("{label} ({letter}:)")
        };

        mounts.push(RawMount {
            mount_id: index as u64 + 1,
            parent_id: 0,
            device_id: format!("{letter}:"),
            root: root.clone(),
            target: PathBuf::from(&root),
            mount_options: vec![],
            filesystem,
            source: root.clone(),
            super_options: vec![],
            stats: stats_ok.then(|| FilesystemStats {
                total_bytes: total,
                used_bytes: total.saturating_sub(free),
                available_bytes: available,
            }),
            stats_error: (!stats_ok).then(|| std::io::Error::last_os_error().to_string()),
            backing_id: Some(if info_ok && serial != 0 {
                format!("volume:{serial:08X}")
            } else {
                format!("drive:{letter}")
            }),
            volume_name: Some(name),
            removable: kind == DRIVE_REMOVABLE || kind == DRIVE_CDROM,
        });
    }
    Ok(Discovery {
        mounts,
        warnings: vec![],
    })
}
