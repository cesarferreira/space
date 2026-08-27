use std::fs;
use std::time::Duration;
#[test]
fn scans_files_without_following_symlinks() {
    let root = std::env::temp_dir().join(format!("space-test-{}", std::process::id()));
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/file"), vec![0; 123]).unwrap();
    let e = space::scan::directory_size(&root);
    assert_eq!(e.bytes, 123);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn exhausted_budget_returns_an_incomplete_lower_bound() {
    let root = std::env::temp_dir().join(format!("space-budget-test-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("file"), vec![0; 123]).unwrap();
    let estimate = space::scan::directory_size_with_budget(&root, Duration::ZERO);
    assert!(estimate.incomplete);
    fs::remove_dir_all(root).unwrap();
}
