use space::model::{Estimate, StorageReport, used_percent};

#[test]
fn percentage_uses_used_over_total() {
    assert_eq!(used_percent(650_000_000_000, 1_000_000_000_000), 65.0);
    assert_eq!(used_percent(1, 0), 0.0);
}

#[test]
fn report_json_has_a_versioned_stable_root() {
    let value = serde_json::to_value(StorageReport::empty()).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["storage"], serde_json::json!([]));
    assert_eq!(value["hidden_mounts"], serde_json::json!([]));
}

#[test]
fn lower_bound_estimate_records_inaccessible_paths() {
    let estimate = Estimate::lower_bound(86, 3);
    assert_eq!(estimate.bytes, 86);
    assert!(estimate.incomplete);
    assert_eq!(estimate.inaccessible_paths, 3);
}
