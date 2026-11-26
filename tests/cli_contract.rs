use std::process::Command;

fn space() -> Command {
    Command::new(env!("CARGO_BIN_EXE_space"))
}

#[test]
fn accepts_the_mvp_flags() {
    for args in [
        vec!["--json", "--no-scan"],
        vec!["--all", "--no-scan"],
        vec!["--why", "tmpfs", "--no-scan"],
        vec!["--reclaimable", "--no-scan"],
    ] {
        let output = space().args(args).output().expect("run space");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn rejects_conflicting_focus_modes() {
    let output = space()
        .args(["--why", "tmpfs", "--reclaimable"])
        .output()
        .expect("run space");
    assert!(!output.status.success());
}

#[test]
fn json_mode_emits_a_versioned_report() {
    let output = space().args(["--json", "--no-scan"]).output().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert!(value["storage"].is_array());
}

#[test]
fn dumb_terminal_uses_ascii() {
    let output = space()
        .env("TERM", "dumb")
        .arg("--no-scan")
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains('#'), "{text}");
    for unicode in ['█', '░', '·', '…', '⚠', '├'] {
        assert!(!text.contains(unicode), "found {unicode:?} in {text}");
    }
}

#[test]
fn empty_trash_conflicts_with_report_modes() {
    for args in [
        vec!["--empty-trash", "--json"],
        vec!["--empty-trash", "--why", "tmpfs"],
        vec!["--empty-trash", "--all"],
    ] {
        let output = space().args(&args).output().unwrap();
        assert!(!output.status.success(), "{args:?} should be rejected");
    }
}

#[test]
fn yes_requires_empty_trash() {
    let output = space().arg("--yes").output().unwrap();
    assert!(!output.status.success());
    let output = space()
        .args(["--clean", "x", "--empty-trash"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[cfg(unix)]
#[test]
fn empty_trash_without_a_terminal_deletes_nothing() {
    let home = std::env::temp_dir().join(format!("space-cli-trash-{}", std::process::id()));
    let files = home.join(".local/share/Trash/files");
    let mac = home.join(".Trash");
    std::fs::create_dir_all(&files).unwrap();
    std::fs::create_dir_all(&mac).unwrap();
    std::fs::write(files.join("keep"), b"x").unwrap();
    std::fs::write(mac.join("keep"), b"x").unwrap();
    let output = space()
        .arg("--empty-trash")
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--yes"));
    assert!(files.join("keep").exists());
    assert!(mac.join("keep").exists());
    std::fs::remove_dir_all(home).unwrap();
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("space-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn fail_above_exits_with_status_three() {
    let output = space()
        .args(["--no-scan", "--fail-above", "0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(!output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("full"));

    let output = space()
        .args(["--no-scan", "--fail-above", "100"])
        .output()
        .unwrap();
    assert!(output.status.success() || output.status.code() == Some(3));
    assert!(
        space()
            .args(["--fail-above", "101"])
            .output()
            .unwrap()
            .status
            .code()
            == Some(2)
    );
}

#[test]
fn diff_saves_a_snapshot_then_compares() {
    let state = scratch("diff");
    let run = || {
        space()
            .args(["--diff", "--no-scan"])
            .env("SPACE_STATE_DIR", &state)
            .output()
            .unwrap()
    };
    let first = String::from_utf8(run().stdout).unwrap();
    assert!(first.contains("Saved a first snapshot"), "{first}");
    assert!(state.join("snapshot.json").exists());
    let second = String::from_utf8(run().stdout).unwrap();
    assert!(second.starts_with("Changes since just now"), "{second}");
    std::fs::remove_dir_all(state).unwrap();
}

#[test]
fn reclaimable_json_has_totals_and_items() {
    let home = scratch("reclaimable");
    std::fs::create_dir_all(home.join(".gradle/caches/x")).unwrap();
    std::fs::write(home.join(".gradle/caches/x/blob"), vec![1; 100_000]).unwrap();
    let output = space()
        .args(["--reclaimable", "--json"])
        .env("HOME", &home)
        .current_dir(&home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["total_bytes"].as_u64().unwrap() >= 100_000);
    let gradle = value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "Gradle")
        .expect("Gradle item");
    assert_eq!(gradle["clean_command"], "space --clean gradle");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn clean_needs_confirmation_and_rejects_unknown_names() {
    let home = scratch("clean");
    std::fs::create_dir_all(home.join(".gradle/caches")).unwrap();
    std::fs::write(home.join(".gradle/caches/blob"), b"x").unwrap();
    let run = |args: &[&str]| {
        space()
            .args(args)
            .env("HOME", &home)
            .current_dir(&home)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let output = run(&["--clean", "gradle"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--yes"));
    assert!(home.join(".gradle/caches/blob").exists());

    let output = run(&["--clean", "nope"]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Choose from"));

    let output = run(&["--clean", "gradle", "--yes"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!home.join(".gradle/caches/blob").exists());
    assert!(home.join(".gradle/caches").is_dir());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn why_explains_categories_and_still_explains_mounts() {
    let home = scratch("why");
    std::fs::create_dir_all(home.join(".gradle/caches")).unwrap();
    let output = space()
        .args(["--why", "gradle", "--top", "3"])
        .env("HOME", &home)
        .current_dir(&home)
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Safe to delete?"), "{text}");
    let output = space().args(["--why", "tmpfs"]).output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("Temporary memory filesystem"));
    assert!(
        !space()
            .args(["--top", "3"])
            .output()
            .unwrap()
            .status
            .success()
    );
    std::fs::remove_dir_all(home).unwrap();
}
