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

/// Explain a known folder category, such as `gradle`, `trash`, or `developer`.
/// Returns `None` when no rule matches `query`.
pub fn category(
    rules: &[crate::classify::Rule],
    query: &str,
    top: usize,
    deadline: Option<std::time::Instant>,
) -> Option<String> {
    use crate::{
        classify::slug,
        output::terminal::format_estimate,
        scan::{largest_entries, measure_selected, merge},
    };
    let wanted = slug(query);
    let matching: Vec<usize> = (0..rules.len())
        .filter(|&i| rules[i].slug() == wanted || slug(rules[i].category) == wanted)
        .collect();
    if matching.is_empty() {
        return None;
    }
    let sizes: std::collections::BTreeMap<usize, crate::model::Estimate> =
        measure_selected(rules, &matching, deadline)
            .into_iter()
            .collect();

    let mut names: Vec<&str> = vec![];
    for &i in &matching {
        if !names.contains(&rules[i].name) {
            names.push(rules[i].name);
        }
    }
    let mut out = String::new();
    for name in names {
        let group: Vec<usize> = matching
            .iter()
            .copied()
            .filter(|&i| rules[i].name == name)
            .collect();
        let rule = &rules[group[0]];
        let found: Vec<usize> = group
            .iter()
            .copied()
            .filter(|i| sizes.contains_key(i))
            .collect();
        let total = found
            .iter()
            .fold(crate::model::Estimate::exact(0), |sum, i| {
                merge(&sum, &sizes[i])
            });
        let size = if found.is_empty() {
            "not found".to_string()
        } else {
            format_estimate(&total)
        };
        if !out.is_empty() {
            out.push_str("\n────\n\n");
        }
        out.push_str(&format!(
            "{name}  {size}\n{} · {}\n\nLocation\n",
            rule.category,
            rule.reclaimability.label()
        ));
        for &i in group.iter().take(10) {
            match sizes.get(&i) {
                Some(e) if group.len() > 1 => out.push_str(&format!(
                    "  {}  {}\n",
                    rules[i].path.display(),
                    format_estimate(e)
                )),
                Some(_) => out.push_str(&format!("  {}\n", rules[i].path.display())),
                None => out.push_str(&format!("  {} (not found)\n", rules[i].path.display())),
            }
        }
        if group.len() > 10 {
            out.push_str(&format!("  … and {} more\n", group.len() - 10));
        }
        out.push_str(&format!(
            "\nWhat it is\n  {}\n\nSafe to delete?\n  {}\n",
            rule.what, rule.safety
        ));
        if let [only] = found.as_slice() {
            let entries = largest_entries(&rules[*only].path, top, deadline);
            if !entries.is_empty() {
                out.push_str("\nLargest entries\n");
                let width = entries
                    .iter()
                    .map(|(p, _)| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .chars()
                            .count()
                    })
                    .max()
                    .unwrap_or(0)
                    .min(40);
                for (path, estimate) in entries {
                    let file = path.file_name().unwrap_or_default().to_string_lossy();
                    out.push_str(&format!(
                        "  {file:<width$}  {}\n",
                        format_estimate(&estimate)
                    ));
                }
            }
        }
        out.push_str(&format!(
            "\nClean up\n  {}\n",
            rule.clean_hint()
                .unwrap_or_else(|| "space does not clean this automatically.".into())
        ));
    }
    Some(out)
}
