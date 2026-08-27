use std::path::PathBuf;

use space::{
    model::{Confidence, Estimate, Reclaimability, SemanticCategory},
    output::terminal::{RenderOptions, render_with_options},
    platform::{Discovery, FilesystemStats, RawMount},
    storage::topology::normalize_for_environment,
};

fn mount(device: &str, filesystem: &str, target: &str, total: u64, used: u64) -> RawMount {
    RawMount {
        mount_id: 1,
        parent_id: 0,
        device_id: device.into(),
        root: "/".into(),
        target: PathBuf::from(target),
        mount_options: vec![],
        filesystem: filesystem.into(),
        source: device.into(),
        super_options: vec![],
        stats: Some(FilesystemStats {
            total_bytes: total,
            used_bytes: used,
            available_bytes: total - used,
        }),
        stats_error: None,
        backing_id: Some(device.into()),
        volume_name: None,
        removable: false,
    }
}

#[test]
fn workspace_and_separate_home_identify_a_container_environment() {
    assert!(space::storage::topology::is_container_environment(
        &container_discovery(),
        false,
    ));
}

#[test]
fn semantic_paths_belong_to_their_longest_mount_prefix() {
    let report = normalize_for_environment(container_discovery(), true);
    let index = space::scan::owning_volume_index(
        &report,
        std::path::Path::new("/home/cesarferreira/.cargo/registry"),
    );

    assert_eq!(index.map(|i| report.storage[i].name.as_str()), Some("Home"));
}

fn container_discovery() -> Discovery {
    Discovery {
        mounts: vec![
            mount("0:20", "configfs", "/sys/kernel/config", 0, 0),
            mount("0:22", "mqueue", "/dev/mqueue", 0, 0),
            mount(
                "0:51",
                "fuse",
                "/mnt/remote/azure_session_dir",
                173_200_000_000,
                4_100,
            ),
            mount(
                "0:53",
                "nfs4",
                "/home/cesarferreira",
                150_300_000_000,
                70_800_000_000,
            ),
            mount("259:0", "ext4", "/boot", 923_000_000, 122_000_000),
            mount("8:1", "ext4", "/", 230_800_000_000, 14_500_000_000),
            mount("8:17", "ext4", "/mnt", 315_900_000_000, 41_000),
            mount("8:32", "xfs", "/workspace", 549_600_000_000, 95_700_000_000),
        ],
        warnings: vec![],
    }
}

fn category(name: &str, parent: &str, bytes: u64) -> SemanticCategory {
    SemanticCategory {
        name: name.into(),
        path: vec![parent.into(), name.into()],
        estimate: Estimate::exact(bytes),
        confidence: Confidence::Exact,
        reclaimability: Reclaimability::Unknown,
        children: vec![],
    }
}

#[test]
fn hybrid_output_is_compact_and_groups_semantic_children() {
    let mut report = normalize_for_environment(container_discovery(), true);
    report.storage[0].categories = vec![
        category("Rust", "Developer", 1_900_000_000),
        category("Gradle", "Developer", 900_700_000),
        category("Android", "Developer", 2_700_000_000),
        category("Other", "Other", 65_300_000_000),
    ];
    let text = render_with_options(
        &report,
        false,
        RenderOptions {
            ascii: false,
            color: false,
            width: 80,
        },
    );

    let expected = concat!(
        "Home               70.8 / 150.3 GB  47.1%  ██████░░░░░░  79.5 GB free\n\n",
        "  Developer     5.5 GB\n",
        "    Android     2.7 GB\n",
        "    Rust        1.9 GB\n",
        "  Other         65.3 GB\n\n",
        "Workspace          95.7 / 549.6 GB  17.4%  ██░░░░░░░░░░  453.9 GB free\n",
        "Container root     14.5 / 230.8 GB   6.3%  █░░░░░░░░░░░  216.3 GB free\n",
        "Mounted data    41.0 KB / 315.9 GB   0.0%  ░░░░░░░░░░░░  315.9 GB free\n\n",
        "4 system mounts hidden · --all details · --why explain\n",
        "⚠ Container environment: visible storage may not be physical disks.\n",
    );
    assert_eq!(text, expected);
}

#[test]
fn usage_bars_use_green_yellow_and_red_thresholds() {
    let mut report = normalize_for_environment(container_discovery(), true);
    report.storage[0].used_percent = 47.1;
    report.storage[1].used_percent = 75.0;
    report.storage[2].used_percent = 90.0;
    let text = render_with_options(
        &report,
        false,
        RenderOptions {
            ascii: false,
            color: true,
            width: 80,
        },
    );

    assert!(text.contains("\u{1b}[32m"));
    assert!(text.contains("\u{1b}[33m"));
    assert!(text.contains("\u{1b}[31m"));
    assert!(text.contains("\u{1b}[32m██████\u{1b}[0m\u{1b}[2m░░░░░░\u{1b}[0m"));
}

#[test]
fn narrow_dashboard_never_exceeds_the_terminal_width() {
    let mut report = normalize_for_environment(container_discovery(), true);
    report.storage[0].categories = vec![
        category("Android development tools", "Developer", 2_700_000_000),
        category("Other", "Other", 68_100_000_000),
    ];

    let text = render_with_options(
        &report,
        false,
        RenderOptions {
            ascii: false,
            color: false,
            width: 48,
        },
    );

    assert!(
        text.lines().all(|line| line.chars().count() <= 48),
        "{text}"
    );
    assert!(text.contains("Developer"), "{text}");
    assert!(text.contains("\n    Android development tools"), "{text}");
    assert!(text.contains('…'), "{text}");
}

#[test]
fn container_output_contains_only_meaningful_storage_roles() {
    let report = normalize_for_environment(container_discovery(), true);
    let names: Vec<_> = report
        .storage
        .iter()
        .map(|volume| volume.name.as_str())
        .collect();

    assert_eq!(
        names,
        ["Home", "Workspace", "Container root", "Mounted data"]
    );
    assert_eq!(report.hidden_mounts.len(), 4);
}
