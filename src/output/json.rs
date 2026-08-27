use crate::model::StorageReport;
use anyhow::Result;
pub fn render(r: &StorageReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(r)? + "\n")
}
