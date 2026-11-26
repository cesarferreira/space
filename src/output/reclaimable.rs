//! `space --reclaimable`: the space you could free, largest first.

use crate::{
    classify::slug,
    model::{Estimate, Reclaimability, StorageReport},
    output::terminal::{RenderOptions, asciify, format_bytes, format_estimate},
    scan::merge,
};
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub name: String,
    pub group: String,
    pub volumes: Vec<String>,
    pub estimate: Estimate,
    pub reclaimability: Reclaimability,
    /// The command that frees this space, if `space` can do it.
    pub clean_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub schema_version: u32,
    pub total_bytes: u64,
    /// The part of `total_bytes` that `space --clean` or `--empty-trash` can free.
    pub cleanable_bytes: u64,
    pub items: Vec<Item>,
}

/// Reclaimable categories across all volumes, merged by name, largest first.
pub fn summarize(report: &StorageReport) -> Summary {
    let mut items: Vec<Item> = vec![];
    for volume in &report.storage {
        for category in &volume.categories {
            if !category.reclaimability.is_reclaimable() || category.estimate.bytes == 0 {
                continue;
            }
            if let Some(item) = items.iter_mut().find(|i| i.name == category.name) {
                item.estimate = merge(&item.estimate, &category.estimate);
                item.volumes.push(volume.name.clone());
                continue;
            }
            let clean_command = category.cleanable.then(|| {
                if category.path.first().map(String::as_str) == Some("Trash") {
                    "space --empty-trash".to_string()
                } else {
                    format!("space --clean {}", slug(&category.name))
                }
            });
            items.push(Item {
                name: category.name.clone(),
                group: category.path.first().cloned().unwrap_or_default(),
                volumes: vec![volume.name.clone()],
                estimate: category.estimate.clone(),
                reclaimability: category.reclaimability,
                clean_command,
            });
        }
    }
    items.sort_by_key(|i| std::cmp::Reverse(i.estimate.bytes));
    Summary {
        schema_version: 1,
        total_bytes: items.iter().map(|i| i.estimate.bytes).sum(),
        cleanable_bytes: items
            .iter()
            .filter(|i| i.clean_command.is_some())
            .map(|i| i.estimate.bytes)
            .sum(),
        items,
    }
}

pub fn render_json(report: &StorageReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(&summarize(report))? + "\n")
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

pub fn render_terminal(report: &StorageReport, options: RenderOptions) -> String {
    let width = options.width.clamp(1, 100);
    let scanned = report.storage.iter().any(|v| !v.categories.is_empty());
    if !scanned {
        return "Nothing was scanned. Run `space --reclaimable` without --no-scan.\n".into();
    }
    let summary = summarize(report);
    if summary.items.is_empty() {
        return "No reclaimable space found in the known folders.\n".into();
    }
    let bold = |text: &str| {
        if options.color {
            format!("\u{1b}[1m{text}\u{1b}[0m")
        } else {
            text.to_owned()
        }
    };
    let name_width = summary
        .items
        .iter()
        .map(|i| i.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(5, 14);
    let mut out = format!("{}\n\n", bold("Reclaimable space"));
    for item in &summary.items {
        let how = item
            .clean_command
            .clone()
            .unwrap_or_else(|| format!("space --why {}", slug(&item.name)));
        let line = format!(
            "  {:<name_width$}  {:>10}  {:<12}  {how}",
            truncate(&item.name, name_width),
            format_estimate(&item.estimate),
            item.reclaimability.label(),
        );
        out.push_str(&truncate(&line, width));
        out.push('\n');
    }
    let incomplete = summary.items.iter().any(|i| i.estimate.incomplete);
    let at_least = if incomplete { "≥ " } else { "" };
    out.push('\n');
    out.push_str(&truncate(
        &format!(
            "  {:<name_width$}  {:>10}  {} can be cleaned by space",
            "Total",
            format!("{at_least}{}", format_bytes(summary.total_bytes)),
            format_bytes(summary.cleanable_bytes),
        ),
        width,
    ));
    out.push('\n');
    out.push_str(&truncate(
        "Review first: check what the folder holds before deleting it.",
        width,
    ));
    out.push('\n');
    if options.ascii { asciify(out) } else { out }
}
