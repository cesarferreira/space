#![cfg(unix)]

use space::trash::{Trash, confirm_and_empty, volume_trashes};
use std::{fs, io::Cursor, path::PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("space-trash-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn freedesktop_fixture(name: &str) -> (PathBuf, Trash) {
    let data_home = temp_dir(name);
    let trash = Trash::freedesktop(&data_home);
    fs::create_dir_all(trash.path.join("files/dir")).unwrap();
    fs::create_dir_all(trash.path.join("info")).unwrap();
    fs::write(trash.path.join("files/a.txt"), vec![0; 100]).unwrap();
    fs::write(trash.path.join("files/dir/b.bin"), vec![0; 200]).unwrap();
    fs::write(trash.path.join("info/a.txt.trashinfo"), "[Trash Info]\n").unwrap();
    fs::write(trash.path.join("info/dir.trashinfo"), "[Trash Info]\n").unwrap();
    fs::write(trash.path.join("directorysizes"), "200 0 dir\n").unwrap();
    (data_home, trash)
}

fn run(trash: &Trash, yes: bool, interactive: bool, answer: &str) -> (anyhow::Result<()>, String) {
    let mut out = Vec::new();
    let result = confirm_and_empty(
        std::slice::from_ref(trash),
        yes,
        interactive,
        &mut Cursor::new(answer),
        &mut out,
    );
    (result, String::from_utf8(out).unwrap())
}

#[test]
fn freedesktop_trash_counts_items_not_metadata() {
    let (root, trash) = freedesktop_fixture("count");
    assert_eq!(trash.item_count().unwrap(), 2);
    assert!(trash.size().bytes >= 300);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn confirmation_yes_empties_items_and_metadata_but_keeps_the_layout() {
    let (root, trash) = freedesktop_fixture("yes");
    let (result, out) = run(&trash, false, true, "y\n");
    result.unwrap();
    assert!(out.contains("Trash: 2 items"), "{out}");
    assert!(out.contains("Emptied the trash"), "{out}");
    assert_eq!(fs::read_dir(trash.path.join("files")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(trash.path.join("info")).unwrap().count(), 0);
    assert!(!trash.path.join("directorysizes").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn any_other_answer_deletes_nothing() {
    for answer in ["\n", "n\n", "nope\n", ""] {
        let (root, trash) = freedesktop_fixture("no");
        let (result, out) = run(&trash, false, true, answer);
        result.unwrap();
        assert!(out.contains("Aborted"), "{out}");
        assert_eq!(trash.item_count().unwrap(), 2);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn non_interactive_without_yes_refuses() {
    let (root, trash) = freedesktop_fixture("noninteractive");
    let (result, _) = run(&trash, false, false, "y\n");
    assert!(result.unwrap_err().to_string().contains("--yes"));
    assert_eq!(trash.item_count().unwrap(), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn yes_flag_skips_the_prompt() {
    let (root, trash) = freedesktop_fixture("assume-yes");
    let (result, out) = run(&trash, true, false, "");
    result.unwrap();
    assert!(!out.contains("[y/N]"), "{out}");
    assert_eq!(trash.item_count().unwrap(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn flat_trash_empties_every_entry() {
    let home = temp_dir("flat");
    let trash = Trash::macos(&home);
    fs::create_dir_all(trash.path.join("folder")).unwrap();
    fs::write(trash.path.join("folder/x"), b"x").unwrap();
    fs::write(trash.path.join("y"), b"y").unwrap();
    fs::write(trash.path.join(".DS_Store"), b"").unwrap();
    assert_eq!(trash.item_count().unwrap(), 2);
    let (result, _) = run(&trash, true, false, "");
    result.unwrap();
    assert!(trash.path.exists());
    assert_eq!(fs::read_dir(&trash.path).unwrap().count(), 0);
    fs::remove_dir_all(home).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn trash_permission_failure_requires_interactive_sudo_confirmation() {
    use std::os::unix::fs::PermissionsExt;

    let home = temp_dir("permission");
    let trash = Trash::macos(&home);
    let bundle = trash.path.join("Locked.app");
    fs::create_dir_all(&bundle).unwrap();
    let protected_file = bundle.join("file");
    fs::write(&protected_file, b"data").unwrap();
    let removable_file = trash.path.join("removable");
    fs::write(&removable_file, b"data").unwrap();
    fs::set_permissions(&bundle, fs::Permissions::from_mode(0o555)).unwrap();

    let (result, out) = run(&trash, true, false, "");
    let (declined, prompt) = run(&trash, true, true, "n\n");
    let protected_remains = protected_file.exists();
    let removable_remains = removable_file.exists();
    if bundle.exists() {
        fs::set_permissions(&bundle, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::remove_dir_all(home).unwrap();

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("1 failed to delete")
    );
    assert!(out.contains("in a terminal to retry with sudo"), "{out}");
    assert!(!out.contains("[y/N]"), "{out}");
    assert!(
        declined
            .unwrap_err()
            .to_string()
            .contains("1 failed to delete")
    );
    assert!(
        prompt.contains("Retry the permission-denied items with sudo?"),
        "{prompt}"
    );
    assert!(protected_remains);
    assert!(!removable_remains);
}

#[test]
fn missing_trash_is_reported_as_empty() {
    let root = temp_dir("missing");
    let trash = Trash::freedesktop(&root);
    let (result, out) = run(&trash, false, true, "");
    result.unwrap();
    assert!(out.contains("already empty"), "{out}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trashed_symlinks_are_removed_without_following_them() {
    let (root, trash) = freedesktop_fixture("symlink");
    let target = root.join("keep-me");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("important"), b"data").unwrap();
    std::os::unix::fs::symlink(&target, trash.path.join("files/link")).unwrap();
    let (result, _) = run(&trash, true, false, "");
    result.unwrap();
    assert!(target.join("important").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn linux_volume_trashes_follow_the_freedesktop_rules() {
    use std::os::unix::fs::PermissionsExt;
    let mount = temp_dir("volume");
    assert!(volume_trashes(&mount, 1000, false).is_empty());

    fs::create_dir_all(mount.join(".Trash-1000/files")).unwrap();
    fs::create_dir_all(mount.join(".Trash/1000/files")).unwrap();
    // Without the sticky bit, the shared .Trash must be ignored.
    let found: Vec<_> = volume_trashes(&mount, 1000, false)
        .into_iter()
        .map(|t| t.path)
        .collect();
    assert_eq!(found, vec![mount.join(".Trash-1000")]);

    fs::set_permissions(mount.join(".Trash"), fs::Permissions::from_mode(0o1777)).unwrap();
    assert_eq!(volume_trashes(&mount, 1000, false).len(), 2);
    assert!(volume_trashes(&mount, 1001, false).is_empty());
    fs::remove_dir_all(mount).unwrap();
}

#[test]
fn macos_volume_trash_lives_under_volumes() {
    let other = temp_dir("not-volumes");
    fs::create_dir_all(other.join(".Trashes/501")).unwrap();
    assert!(volume_trashes(&other, 501, true).is_empty());
    fs::remove_dir_all(other).unwrap();
}

#[test]
fn several_trashes_are_emptied_together() {
    let (root_a, a) = freedesktop_fixture("multi-a");
    let (root_b, b) = freedesktop_fixture("multi-b");
    let mut out = Vec::new();
    confirm_and_empty(
        &[a.clone(), b.clone()],
        true,
        false,
        &mut Cursor::new(""),
        &mut out,
    )
    .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert_eq!(out.matches("Trash: 2 items").count(), 2, "{out}");
    assert_eq!(a.item_count().unwrap(), 0);
    assert_eq!(b.item_count().unwrap(), 0);
    fs::remove_dir_all(root_a).unwrap();
    fs::remove_dir_all(root_b).unwrap();
}
