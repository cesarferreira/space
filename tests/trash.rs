#![cfg(unix)]

use space::trash::{Trash, confirm_and_empty};
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
    let result = confirm_and_empty(trash, yes, interactive, &mut Cursor::new(answer), &mut out);
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
