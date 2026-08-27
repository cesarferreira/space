use crate::model::{SemanticCategory, StorageReport, StorageVolume};
use std::{collections::BTreeMap, io::IsTerminal};

#[derive(Clone, Copy)]
pub struct RenderOptions {
    pub ascii: bool,
    pub color: bool,
    pub width: usize,
}

pub fn format_bytes(n: u64) -> String {
    let (v, u) = if n >= 1_000_000_000_000 {
        (n as f64 / 1e12, "TB")
    } else if n >= 1_000_000_000 {
        (n as f64 / 1e9, "GB")
    } else if n >= 1_000_000 {
        (n as f64 / 1e6, "MB")
    } else if n >= 1_000 {
        (n as f64 / 1e3, "KB")
    } else {
        return format!("{n} B");
    };
    format!("{v:.1} {u}")
}

fn usage_bar(percent: f64, width: usize, options: RenderOptions) -> String {
    let filled = (percent.clamp(0.0, 100.0) / 100.0 * width as f64).round() as usize;
    let (full, empty) = if options.ascii {
        ('#', '-')
    } else {
        ('█', '░')
    };
    let full: String = std::iter::repeat_n(full, filled).collect();
    let empty: String = std::iter::repeat_n(empty, width.saturating_sub(filled)).collect();
    if !options.color {
        return format!("{full}{empty}");
    }
    let mut out = String::new();
    if !full.is_empty() {
        out.push_str(&colorize(&full, percent));
    }
    if !empty.is_empty() {
        out.push_str(&format!("\u{1b}[2m{empty}\u{1b}[0m"));
    }
    out
}

fn colorize(text: &str, percent: f64) -> String {
    let code = if percent < 70.0 {
        32
    } else if percent < 85.0 {
        33
    } else {
        31
    };
    format!("\u{1b}[{code}m{text}\u{1b}[0m")
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

fn compact_capacity(used: u64, total: u64) -> String {
    let used = format_bytes(used);
    let total = format_bytes(total);
    match (used.rsplit_once(' '), total.rsplit_once(' ')) {
        (Some((used_value, used_unit)), Some((_, total_unit))) if used_unit == total_unit => {
            format!("{used_value} / {total}")
        }
        _ => format!("{used} / {total}"),
    }
}

fn styled_name(name: &str, cell_width: usize, color: bool) -> String {
    let padding = cell_width.saturating_sub(name.chars().count());
    if color {
        format!("\u{1b}[1m{name}\u{1b}[0m{}", " ".repeat(padding))
    } else {
        format!("{name}{}", " ".repeat(padding))
    }
}

fn styled_percent(percent: f64, color: bool) -> String {
    let text = format!("{percent:>4.1}%");
    if color {
        colorize(&text, percent)
    } else {
        text
    }
}

fn volume_lines(
    volume: &StorageVolume,
    options: RenderOptions,
    width: usize,
    name_width: usize,
    capacity_width: usize,
) -> String {
    let name = truncate(&volume.name, name_width);
    let capacity = compact_capacity(volume.used_bytes, volume.total_bytes);
    let free = format!("{} free", format_bytes(volume.available_bytes));
    let bar = usage_bar(volume.used_percent, 12, options);
    let plain = format!(
        "{name:<name_width$}  {capacity:>capacity_width$}  {:>5}  {}  {free}",
        format!("{:.1}%", volume.used_percent),
        usage_bar(
            volume.used_percent,
            12,
            RenderOptions {
                color: false,
                ..options
            }
        )
    );

    if plain.chars().count() <= width {
        return format!(
            "{}  {capacity:>capacity_width$}  {}  {bar}  {free}\n",
            styled_name(&name, name_width, options.color),
            styled_percent(volume.used_percent, options.color),
        );
    }

    let heading = truncate(
        &format!("{name}  {capacity}  {:.1}%", volume.used_percent),
        width,
    );
    let heading = if options.color {
        heading.replacen(&name, &format!("\u{1b}[1m{name}\u{1b}[0m"), 1)
    } else {
        heading
    };
    let detail = truncate(
        &format!(
            "  {}  {free}",
            usage_bar(
                volume.used_percent,
                12,
                RenderOptions {
                    color: false,
                    ..options
                }
            )
        ),
        width,
    );
    let detail = if options.color {
        detail.replacen(
            &usage_bar(
                volume.used_percent,
                12,
                RenderOptions {
                    color: false,
                    ..options
                },
            ),
            &bar,
            1,
        )
    } else {
        detail
    };
    format!("{heading}\n{detail}\n")
}

fn category_lines(categories: &[SemanticCategory], options: RenderOptions, width: usize) -> String {
    let mut groups: BTreeMap<&str, Vec<&SemanticCategory>> = BTreeMap::new();
    let mut other = None;
    for category in categories {
        if category.path.first().map(String::as_str) == Some("Other") {
            other = Some(category);
        } else {
            groups.entry(&category.path[0]).or_default().push(category);
        }
    }
    let mut out = String::new();
    for (group, mut children) in groups {
        children.sort_by_key(|child| std::cmp::Reverse(child.estimate.bytes));
        let total: u64 = children.iter().map(|child| child.estimate.bytes).sum();
        let line = format!("  {group:<12}  {}", format_bytes(total));
        let line = truncate(&line, width);
        let line = if options.color {
            line.replacen(group, &format!("\u{1b}[36m{group}\u{1b}[0m"), 1)
        } else {
            line
        };
        out.push_str(&line);
        out.push('\n');
        for child in children.into_iter().take(2) {
            let line = truncate(
                &format!(
                    "    {:<10}  {}",
                    child.name,
                    format_bytes(child.estimate.bytes)
                ),
                width,
            );
            if options.color {
                out.push_str(&format!("\u{1b}[2m{line}\u{1b}[0m\n"));
            } else {
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    if let Some(category) = other {
        let line = format!(
            "  {:<12}  {}",
            category.name,
            format_bytes(category.estimate.bytes)
        );
        out.push_str(&truncate(&line, width));
        out.push('\n');
    }
    if options.ascii {
        out.replace('·', "|").replace('…', "~").replace('⚠', "!")
    } else {
        out
    }
}

pub fn render_with_options(report: &StorageReport, all: bool, options: RenderOptions) -> String {
    let width = options.width.clamp(1, 100);
    let name_width = report
        .storage
        .iter()
        .map(|volume| volume.name.chars().count())
        .max()
        .unwrap_or(0)
        .min(18);
    let capacity_width = report
        .storage
        .iter()
        .map(|volume| {
            compact_capacity(volume.used_bytes, volume.total_bytes)
                .chars()
                .count()
        })
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for volume in &report.storage {
        out.push_str(&volume_lines(
            volume,
            options,
            width,
            name_width,
            capacity_width,
        ));
        if !volume.categories.is_empty() {
            out.push('\n');
            out.push_str(&category_lines(&volume.categories, options, width));
            out.push('\n');
        }
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    let footer = truncate(
        &format!(
            "{} system mounts hidden · --all details · --why explain",
            report.hidden_mounts.len()
        ),
        width,
    );
    if options.color {
        out.push_str(&format!("\u{1b}[2m{footer}\u{1b}[0m\n"));
    } else {
        out.push_str(&footer);
        out.push('\n');
    }
    if all {
        for mount in &report.hidden_mounts {
            let branch = if options.ascii { "|-" } else { "├─" };
            let line = format!(
                "{branch} {}  {}\n",
                mount.target.display(),
                mount.hidden_reason.as_deref().unwrap_or("derived mount")
            );
            out.push_str(&truncate(line.trim_end(), width));
            out.push('\n');
        }
    }
    if report
        .storage
        .iter()
        .any(|volume| volume.name == "Container root")
    {
        out.push_str(&truncate(
            "⚠ Container environment: visible storage may not be physical disks.",
            width,
        ));
        out.push('\n');
    }
    if options.ascii {
        out.replace('·', "|").replace('…', "~").replace('⚠', "!")
    } else {
        out
    }
}

pub fn render(report: &StorageReport, all: bool) -> String {
    let ascii = std::env::var("TERM").as_deref() == Ok("dumb");
    let color = !ascii && std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();
    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(80);
    render_with_options(
        report,
        all,
        RenderOptions {
            ascii,
            color,
            width,
        },
    )
}
