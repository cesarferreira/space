#![cfg(target_os = "macos")]

use space::platform::macos::parse_apfs_list;
use space::platform::{Discovery, FilesystemStats, RawMount};
use space::storage::topology::normalize_for_environment;
use std::path::PathBuf;

const APFS_LIST: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Containers</key>
  <array>
    <dict>
      <key>APFSContainerUUID</key><string>AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE</string>
      <key>ContainerReference</key><string>disk3</string>
      <key>Volumes</key>
      <array>
        <dict>
          <key>DeviceIdentifier</key><string>disk3s1</string>
          <key>Name</key><string>Macintosh HD</string>
          <key>Roles</key><array><string>System</string></array>
        </dict>
        <dict>
          <key>DeviceIdentifier</key><string>disk3s5</string>
          <key>Roles</key><array><string>Data</string></array>
        </dict>
      </array>
    </dict>
  </array>
</dict>
</plist>"#;

#[test]
fn apfs_views_share_their_container_identity() {
    let volumes = parse_apfs_list(APFS_LIST).expect("parse diskutil plist");

    assert_eq!(
        volumes["disk3s1"].backing_id,
        "apfs:AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"
    );
    assert_eq!(volumes["disk3s5"].backing_id, volumes["disk3s1"].backing_id);
    assert_eq!(volumes["disk3s5"].roles, vec!["Data"]);
    assert_eq!(volumes["disk3s1"].name.as_deref(), Some("Macintosh HD"));
}

#[test]
fn native_discovery_returns_the_root_mount() {
    let discovery = space::platform::macos::discover().expect("discover macOS mounts");

    assert!(
        discovery
            .mounts
            .iter()
            .any(|mount| mount.target == std::path::Path::new("/"))
    );
}

fn mount(source: &str, target: &str, filesystem: &str, backing_id: &str) -> RawMount {
    RawMount {
        mount_id: 1,
        parent_id: 0,
        device_id: source.trim_start_matches("/dev/").to_owned(),
        root: "/".to_owned(),
        target: PathBuf::from(target),
        mount_options: vec![],
        filesystem: filesystem.to_owned(),
        source: source.to_owned(),
        super_options: vec![],
        stats: Some(FilesystemStats {
            total_bytes: 1_000,
            used_bytes: 600,
            available_bytes: 400,
        }),
        stats_error: None,
        backing_id: Some(backing_id.to_owned()),
        volume_name: None,
        removable: false,
    }
}

#[test]
fn derived_macos_mounts_are_hidden_from_primary_storage() {
    let discovery = Discovery {
        mounts: vec![
            mount("/dev/disk3s1s1", "/", "apfs", "apfs:main"),
            mount(
                "/dev/disk1s2",
                "/System/Volumes/xarts",
                "apfs",
                "apfs:firmware",
            ),
            mount(
                "/dev/disk9s1",
                "/private/var/run/com.apple.security.cryptexd/mnt/runtime",
                "apfs",
                "apfs:cryptex",
            ),
            mount(
                "/Users/me/Downloads/App.app",
                "/private/var/folders/xx/T/AppTranslocation/ABC",
                "nullfs",
                "/Users/me/Downloads/App.app",
            ),
        ],
        warnings: vec![],
    };

    let report = normalize_for_environment(discovery, false);

    assert_eq!(report.storage.len(), 1);
    assert_eq!(report.storage[0].representative_mount, PathBuf::from("/"));
    assert_eq!(report.hidden_mounts.len(), 3);
}

#[test]
fn native_root_label_names_the_primary_volume() {
    let mut root = mount("/dev/disk3s1s1", "/", "apfs", "apfs:main");
    root.volume_name = Some("Macintosh HD".to_owned());
    let mut external = mount("/dev/disk36s1", "/Volumes/External", "hfs", "/dev/disk36s1");
    external.volume_name = Some("External".to_owned());

    let report = normalize_for_environment(
        Discovery {
            mounts: vec![external, root],
            warnings: vec![],
        },
        false,
    );

    assert_eq!(report.storage[0].name, "Macintosh HD");
    assert_eq!(report.storage[1].name, "External");
}
