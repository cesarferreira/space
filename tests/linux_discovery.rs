#![cfg(target_os = "linux")]

use space::{
    platform::linux::parse_mountinfo,
    platform::{Discovery, FilesystemStats},
    storage::topology::normalize,
};

fn fixture(input: &str) -> Discovery {
    let mut mounts = parse_mountinfo(input).unwrap();
    for m in &mut mounts {
        let (t, u) = if m.device_id == "259:2" {
            (1_000, 650)
        } else {
            (2_000, 1_000)
        };
        m.stats = Some(FilesystemStats {
            total_bytes: t,
            used_bytes: u,
            available_bytes: t - u,
        });
        m.backing_id = Some(m.device_id.clone());
    }
    Discovery {
        mounts,
        warnings: vec![],
    }
}
#[test]
fn virtual_and_duplicate_views_are_not_disks() {
    let text = "29 1 259:2 / / rw - ext4 /dev/nvme0n1p2 rw\n30 29 0:5 / /proc rw - proc proc rw\n31 29 0:45 / /docker rw - overlay overlay rw\n32 29 259:2 /home /home rw - ext4 /dev/nvme0n1p2 rw\n40 29 8:1 / /mnt/data rw - mysteryfs /dev/sda1 rw";
    let r = normalize(fixture(text));
    assert_eq!(r.storage.len(), 2);
    assert_eq!(r.hidden_mounts.len(), 2);
    assert_eq!(
        r.storage
            .iter()
            .find(|v| v.id.contains("259:2"))
            .unwrap()
            .used_percent,
        65.0
    );
    assert!(r.storage.iter().any(|v| v.filesystem == "mysteryfs"));
}
#[test]
fn decodes_proc_mount_escapes() {
    let m = parse_mountinfo("1 0 8:1 / /mnt/My\\040Disk rw - ext4 /dev/sda1 rw").unwrap();
    assert_eq!(m[0].target.to_string_lossy(), "/mnt/My Disk");
}
