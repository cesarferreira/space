use crate::model::StorageReport;
use anyhow::{Result, bail};
pub fn render(r: &StorageReport, q: &str) -> Result<String> {
    let ql = q.to_lowercase();
    let m = r.hidden_mounts.iter().find(|m| {
        m.target.to_string_lossy().to_lowercase() == ql
            || m.filesystem.to_lowercase() == ql
            || m.target.to_string_lossy().to_lowercase().contains(&ql)
    });
    if let Some(m) = m {
        let name = match m.filesystem.as_str() {
            "tmpfs" => "Temporary memory filesystem",
            "overlay" => "Docker overlay",
            "devfs" | "devtmpfs" => "Device filesystem",
            _ => m.hidden_reason.as_deref().unwrap_or("Derived mount"),
        };
        return Ok(format!(
            "{name}\n\nType\n  {}\n\nBacked by\n  {}\n\nIndependent disk\n  No\n\nWhy it exists\n  This mount is provided by the operating system.\n\nDisk usage\n  Its capacity is not counted as an additional disk.\n",
            m.hidden_reason.as_deref().unwrap_or("derived mount"),
            m.backing_storage_id
                .as_deref()
                .unwrap_or("memory or system storage")
        ));
    }
    if ql == "tmpfs" {
        return Ok("Temporary memory filesystem\n\nIndependent disk\n  No\n\nWhy it exists\n  tmpfs stores temporary data in memory and is commonly mounted by Linux.\n\nDisk usage\n  Its capacity is not counted as an additional disk.\n".into());
    }
    bail!("No mount or category matches '{q}'")
}
