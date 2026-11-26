//! `space --diff`: compare usage with the previous `--diff` run, then save a new snapshot.

use crate::{model::StorageReport, output::terminal::format_bytes};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const MIN_CHANGE: u64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeSnapshot {
    pub id: String,
    pub name: String,
    pub used_bytes: u64,
    /// Category path (such as `Developer/Gradle`) → bytes.
    pub categories: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    /// Seconds since the Unix epoch.
    pub taken_at: u64,
    pub volumes: Vec<VolumeSnapshot>,
}

impl Snapshot {
    pub fn of(report: &StorageReport, taken_at: u64) -> Self {
        Self {
            schema_version: 1,
            taken_at,
            volumes: report
                .storage
                .iter()
                .map(|v| VolumeSnapshot {
                    id: v.id.clone(),
                    name: v.name.clone(),
                    used_bytes: v.used_bytes,
                    categories: v
                        .categories
                        .iter()
                        .map(|c| (c.path.join("/"), c.estimate.bytes))
                        .collect(),
                })
                .collect(),
        }
    }
}

/// Where the snapshot is kept. `SPACE_STATE_DIR` overrides the platform default.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SPACE_STATE_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = crate::classify::home_dir();
    let base = if cfg!(target_os = "macos") {
        home?.join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local/state")))?
    };
    Some(base.join("space"))
}

fn signed_bytes(delta: i128) -> String {
    let sign = if delta < 0 { '-' } else { '+' };
    format!("{sign}{}", format_bytes(delta.unsigned_abs() as u64))
}

pub fn relative_time(seconds: u64) -> String {
    let (n, unit) = match seconds {
        0..60 => return "just now".into(),
        60..3_600 => (seconds / 60, "minute"),
        3_600..86_400 => (seconds / 3_600, "hour"),
        _ => (seconds / 86_400, "day"),
    };
    let plural = if n == 1 { "" } else { "s" };
    format!("{n} {unit}{plural} ago")
}

/// Describe what changed from `before` to `after`.
pub fn render_diff(before: &Snapshot, after: &Snapshot) -> String {
    let mut out = format!(
        "Changes since {}\n\n",
        relative_time(after.taken_at.saturating_sub(before.taken_at))
    );
    let name_width = after
        .volumes
        .iter()
        .map(|v| v.name.chars().count())
        .max()
        .unwrap_or(0);
    for volume in &after.volumes {
        let Some(old) = before.volumes.iter().find(|v| v.id == volume.id) else {
            out.push_str(&format!(
                "{:<name_width$}  new since the last snapshot\n",
                volume.name
            ));
            continue;
        };
        let delta = volume.used_bytes as i128 - old.used_bytes as i128;
        if delta.unsigned_abs() < u128::from(MIN_CHANGE) {
            out.push_str(&format!("{:<name_width$}  no change\n", volume.name));
        } else {
            out.push_str(&format!(
                "{:<name_width$}  {} used  ({} → {})\n",
                volume.name,
                signed_bytes(delta),
                format_bytes(old.used_bytes),
                format_bytes(volume.used_bytes)
            ));
        }
        if volume.categories.is_empty() || old.categories.is_empty() {
            continue;
        }
        let mut changes: Vec<(&str, i128)> = volume
            .categories
            .keys()
            .chain(old.categories.keys())
            .map(|key| {
                let now = volume.categories.get(key).copied().unwrap_or(0) as i128;
                let then = old.categories.get(key).copied().unwrap_or(0) as i128;
                (key.as_str(), now - then)
            })
            .filter(|(_, d)| d.unsigned_abs() >= u128::from(MIN_CHANGE))
            .collect();
        changes.sort_by_key(|(key, d)| (std::cmp::Reverse(d.unsigned_abs()), *key));
        changes.dedup_by_key(|(key, _)| *key);
        for (key, delta) in changes.into_iter().take(5) {
            let name = key.rsplit('/').next().unwrap_or(key);
            out.push_str(&format!("  {name:<12}  {:>10}\n", signed_bytes(delta)));
        }
    }
    for old in &before.volumes {
        if !after.volumes.iter().any(|v| v.id == old.id) {
            out.push_str(&format!("{}  no longer present\n", old.name));
        }
    }
    out
}

/// Compare `report` with the saved snapshot, save `report` as the new one, and
/// return the description.
pub fn diff_and_save(report: &StorageReport) -> Result<String> {
    let dir = state_dir().context("cannot find a folder to keep snapshots in")?;
    let file = dir.join("snapshot.json");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let current = Snapshot::of(report, now);
    let previous: Option<Snapshot> = fs::read(&file)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    fs::write(&file, serde_json::to_vec_pretty(&current)?)
        .with_context(|| format!("write {}", file.display()))?;
    Ok(match previous {
        Some(previous) => render_diff(&previous, &current),
        None => format!(
            "Saved a first snapshot to {}.\nRun `space --diff` again later to see what changed.\n",
            file.display()
        ),
    })
}
