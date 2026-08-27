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
