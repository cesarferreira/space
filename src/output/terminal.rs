use crate::model::{Estimate, SemanticCategory, StorageReport, StorageVolume};
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

/// Show a lower bound for a partial size, or the reason no size could be measured.
pub fn format_estimate(estimate: &Estimate) -> String {
    match (estimate.incomplete, estimate.bytes) {
        (false, bytes) => format_bytes(bytes),
        (true, 0) if estimate.timed_out => "scan timed out".into(),
        (true, 0) if estimate.inaccessible_paths > 0 => "unreadable".into(),
        (true, 0) => "scan incomplete".into(),
        (true, bytes) => format!("≥ {}", format_bytes(bytes)),
    }
}

/// Replace characters a dumb terminal cannot show.
pub fn asciify(text: String) -> String {
    text.replace('·', "|")
        .replace('…', "~")
        .replace('⚠', "!")
        .replace('≥', ">=")
        .replace('→', "->")
}

pub fn section_lines(
    group: &str,
    total: Option<&Estimate>,
    children: &[(&str, &Estimate)],
    child_width: usize,
    options: RenderOptions,
) -> String {
    let width = options.width.clamp(1, 100);
    let group_width = child_width + 2;
    let line = match total {
        Some(size) => format!("  {group:<group_width$}  {}", format_estimate(size)),
        None => format!("  {group}"),
    };
    let line = truncate(&line, width);
    let mut out = if options.color {
        line.replacen(group, &format!("\u{1b}[36m{group}\u{1b}[0m"), 1)
    } else {
        line
    };
    // A group with one same-named child (e.g. Trash) needs no detail line.
    if !matches!(children, [(name, _)] if *name == group) {
        for (name, size) in children.iter().take(2) {
            let line = truncate(
                &format!("    {name:<child_width$}  {}", format_estimate(size)),
                width,
            );
            out.push('\n');
            if options.color {
                out.push_str(&format!("\u{1b}[2m{line}\u{1b}[0m"));
            } else {
                out.push_str(&line);
            }
        }
    }
    out
}

fn category_lines(categories: &[SemanticCategory], options: RenderOptions, width: usize) -> String {
    let mut groups: BTreeMap<&str, Vec<&SemanticCategory>> = BTreeMap::new();
    let mut other = None;
    for category in categories {
        if category.path.first().map(String::as_str) == Some("Other") {
            other = Some(category);
        } else if category.estimate.bytes > 0 || category.estimate.incomplete {
            // An existing but empty folder (exactly 0 bytes) is not worth a line.
            groups.entry(&category.path[0]).or_default().push(category);
        }
    }
    for children in groups.values_mut() {
        children.sort_by_key(|child| std::cmp::Reverse(child.estimate.bytes));
    }
    // Line up the size column, widening it for long names such as "node_modules".
    let child_width = groups
        .iter()
        .filter(|(group, children)| !matches!(children.as_slice(), [only] if only.name == **group))
        .flat_map(|(_, children)| children.iter().take(2))
        .map(|child| child.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(10, 16);
    let group_width = child_width + 2;
    let mut out = String::new();
    for (group, children) in groups {
        let total = children.iter().fold(Estimate::exact(0), |sum, child| {
            crate::scan::merge(&sum, &child.estimate)
        });
        let children: Vec<_> = children
            .iter()
            .take(2)
            .map(|child| (child.name.as_str(), &child.estimate))
            .collect();
        out.push_str(&section_lines(
            group,
            Some(&total),
            &children,
            child_width,
            options,
        ));
        out.push('\n');
    }
    if let Some(category) = other {
        let line = format!(
            "  {:<group_width$}  {}",
            category.name,
            format_bytes(category.estimate.bytes)
        );
        out.push_str(&truncate(&line, width));
        out.push('\n');
    }
    if options.ascii { asciify(out) } else { out }
}

pub fn volume_headers(report: &StorageReport, options: RenderOptions) -> Vec<String> {
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
    report
        .storage
        .iter()
        .map(|volume| volume_lines(volume, options, width, name_width, capacity_width))
        .collect()
}

pub fn render_with_options(report: &StorageReport, all: bool, options: RenderOptions) -> String {
    let width = options.width.clamp(1, 100);
    let mut out = String::new();
    for (volume, header) in report.storage.iter().zip(volume_headers(report, options)) {
        out.push_str(&header);
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
    if options.ascii { asciify(out) } else { out }
}

/// Options for the current terminal: `TERM=dumb` → ASCII, `NO_COLOR` or a pipe → no
/// color, `COLUMNS` → width (default 80).
pub fn detect_options() -> RenderOptions {
    let ascii = std::env::var("TERM").as_deref() == Ok("dumb");
    let color = !ascii && std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();
    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(80);
    RenderOptions {
        ascii,
        color,
        width,
    }
}

pub fn render(report: &StorageReport, all: bool) -> String {
    render_with_options(report, all, detect_options())
}
