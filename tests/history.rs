use space::history::{Snapshot, VolumeSnapshot, relative_time, render_diff};
use std::collections::BTreeMap;

fn volume(id: &str, name: &str, used: u64, categories: &[(&str, u64)]) -> VolumeSnapshot {
    VolumeSnapshot {
        id: id.into(),
        name: name.into(),
        used_bytes: used,
        categories: categories
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn relative_times_read_naturally() {
    assert_eq!(relative_time(5), "just now");
    assert_eq!(relative_time(60), "1 minute ago");
    assert_eq!(relative_time(7_200), "2 hours ago");
    assert_eq!(relative_time(3 * 86_400), "3 days ago");
}

#[test]
fn diff_shows_volume_and_category_changes() {
    let before = Snapshot {
        schema_version: 1,
        taken_at: 0,
        volumes: vec![
            volume(
                "a",
                "Home",
                10_000_000_000,
                &[
                    ("Developer/Gradle", 1_000_000_000),
                    ("Developer/npm", 500_000_000),
                ],
            ),
            volume("b", "Workspace", 5_000_000_000, &[]),
            volume("gone", "USB", 1, &[]),
        ],
    };
    let after = Snapshot {
        schema_version: 1,
        taken_at: 2 * 86_400,
        volumes: vec![
            volume(
                "a",
                "Home",
                13_000_000_000,
                &[
                    ("Developer/Gradle", 3_000_000_000),
                    ("Projects/node_modules", 400_000_000),
                ],
            ),
            volume("b", "Workspace", 5_000_000_100, &[]),
            volume("new", "Backup", 1, &[]),
        ],
    };
    let text = render_diff(&before, &after);
    assert!(text.starts_with("Changes since 2 days ago\n"), "{text}");
    assert!(
        text.contains("Home       +3.0 GB used  (10.0 GB → 13.0 GB)"),
        "{text}"
    );
    assert!(text.contains("  Gradle           +2.0 GB\n"), "{text}");
    assert!(text.contains("  npm            -500.0 MB\n"), "{text}");
    assert!(text.contains("  node_modules   +400.0 MB\n"), "{text}");
    assert!(text.contains("Workspace  no change"), "{text}");
    assert!(
        text.contains("Backup     new since the last snapshot"),
        "{text}"
    );
    assert!(text.contains("USB  no longer present"), "{text}");
    let gradle = text.find("Gradle").unwrap();
    let npm = text.find("npm").unwrap();
    assert!(gradle < npm, "largest change first: {text}");
}
