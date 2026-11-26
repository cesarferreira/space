//! `space --clean`: delete reclaimable data in named categories, after confirmation.

use crate::{
    classify::{Clean, Rule, slug},
    model::Estimate,
    output::terminal::format_bytes,
    scan::{measure_selected, merge, nested_rule_paths},
};
use anyhow::{Result, bail};
use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

/// Ask a yes/no question. Only `y` or `yes` counts as yes.
///
/// With `assume_yes`, the answer is yes without asking. Without a terminal
/// (`interactive` is false) and without `assume_yes`, this returns an error.
pub fn confirm(
    question: &str,
    assume_yes: bool,
    interactive: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<bool> {
    if assume_yes {
        return Ok(true);
    }
    if !interactive {
        bail!("refusing to delete without confirmation; pass --yes to skip the prompt");
    }
    write!(out, "{question} [y/N] ")?;
    out.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Delete a file, a symlink (not its target), or a folder and everything in it.
pub fn remove_path(path: &Path) -> io::Result<()> {
    let m = fs::symlink_metadata(path)?;
    if m.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// One thing to clean: a rule name and all its folders (several for `node_modules`).
struct Item<'a> {
    name: &'static str,
    rules: Vec<(&'a Rule, &'a HashSet<PathBuf>)>,
    estimate: Estimate,
}

impl Item<'_> {
    fn action(&self) -> String {
        let first = self.rules[0].0;
        match &first.clean {
            Clean::Contents => format!("delete contents of {}", first.path.display()),
            Clean::Directory if self.rules.len() == 1 => format!("delete {}", first.path.display()),
            Clean::Directory => format!("delete {} folders", self.rules.len()),
            Clean::Command(argv) => format!("run `{}`", argv.join(" ")),
            Clean::Trash(_) => format!("empty {}", first.path.display()),
            Clean::Never => unreachable!("never-clean rules are not selected"),
        }
    }
}

/// Pick the rules a comma-separated selection names. A name matches a rule
/// (`gradle`, `node_modules`) or a whole group (`developer`, `projects`).
fn select(rules: &[Rule], selection: &str) -> Result<Vec<usize>> {
    let mut chosen = vec![];
    for wanted in selection.split(',').map(slug).filter(|s| !s.is_empty()) {
        let matches: Vec<usize> = rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.slug() == wanted || slug(r.category) == wanted)
            .map(|(i, _)| i)
            .collect();
        if matches.is_empty() {
            let mut known: Vec<String> = rules
                .iter()
                .filter(|r| r.cleanable())
                .flat_map(|r| [r.slug(), slug(r.category)])
                .collect();
            known.sort();
            known.dedup();
            bail!(
                "nothing named '{wanted}' can be cleaned here. Choose from: {}",
                known.join(", ")
            );
        }
        let cleanable: Vec<usize> = matches
            .iter()
            .copied()
            .filter(|&i| rules[i].cleanable())
            .collect();
        if cleanable.is_empty() {
            let rule = &rules[matches[0]];
            bail!("space does not clean {}: {}", rule.name, rule.safety);
        }
        chosen.extend(cleanable);
    }
    chosen.sort();
    chosen.dedup();
    Ok(chosen)
}

fn clean_rule(rule: &Rule, nested: &HashSet<PathBuf>) -> Vec<(PathBuf, io::Error)> {
    let mut failures = vec![];
    match &rule.clean {
        Clean::Never => {}
        Clean::Contents => match fs::read_dir(&rule.path) {
            Ok(rd) => {
                for entry in rd.flatten() {
                    let path = entry.path();
                    if nested.contains(&path) {
                        continue;
                    }
                    if let Err(e) = remove_path(&path)
                        && e.kind() != io::ErrorKind::NotFound
                    {
                        failures.push((path, e));
                    }
                }
            }
            Err(e) => failures.push((rule.path.clone(), e)),
        },
        Clean::Directory => {
            if let Err(e) = remove_path(&rule.path)
                && e.kind() != io::ErrorKind::NotFound
            {
                failures.push((rule.path.clone(), e));
            }
        }
        Clean::Command(argv) => {
            let status = Command::new(argv[0]).args(&argv[1..]).status();
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => failures.push((
                    rule.path.clone(),
                    io::Error::other(format!("`{}` exited with {s}", argv.join(" "))),
                )),
                Err(e) if e.kind() == io::ErrorKind::NotFound => failures.push((
                    rule.path.clone(),
                    io::Error::other(format!("`{}` is not installed", argv[0])),
                )),
                Err(e) => failures.push((rule.path.clone(), e)),
            }
        }
        Clean::Trash(trash) => match trash.empty() {
            Ok(outcome) => failures.extend(outcome.failures),
            Err(e) => failures.push((trash.path.clone(), e)),
        },
    }
    failures
}

/// Measure the selected categories, show the plan, confirm, and clean.
#[allow(clippy::too_many_arguments)]
pub fn run(
    rules: &[Rule],
    selection: &str,
    search_incomplete: bool,
    deadline: Option<Instant>,
    assume_yes: bool,
    interactive: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<()> {
    let chosen = select(rules, selection)?;
    let nested = nested_rule_paths(rules);
    let sizes: BTreeMap<usize, Estimate> = measure_selected(rules, &chosen, deadline)
        .into_iter()
        .collect();

    let mut items: Vec<Item> = vec![];
    for &i in &chosen {
        let Some(estimate) = sizes.get(&i) else {
            continue; // The folder does not exist.
        };
        let rule = &rules[i];
        match items
            .iter_mut()
            .find(|item| item.name == rule.name && matches!(rule.clean, Clean::Directory))
        {
            Some(item) => {
                item.rules.push((rule, &nested[i]));
                item.estimate = merge(&item.estimate, estimate);
            }
            None => items.push(Item {
                name: rule.name,
                rules: vec![(rule, &nested[i])],
                estimate: estimate.clone(),
            }),
        }
    }
    if items.is_empty() {
        writeln!(out, "Nothing to clean: none of the selected folders exist.")?;
        return Ok(());
    }

    let name_width = items.iter().map(|i| i.name.len()).max().unwrap_or(0);
    let mut total = Estimate::exact(0);
    writeln!(out, "Will clean:")?;
    for item in &items {
        writeln!(
            out,
            "  {:<name_width$}  {:>10}  {}",
            item.name,
            size_text(&item.estimate),
            item.action()
        )?;
        if item.rules.len() > 1 {
            for (rule, _) in item.rules.iter().take(10) {
                writeln!(
                    out,
                    "  {:<name_width$}              {}",
                    "",
                    rule.path.display()
                )?;
            }
            if item.rules.len() > 10 {
                writeln!(
                    out,
                    "  {:<name_width$}              … and {} more",
                    "",
                    item.rules.len() - 10
                )?;
            }
        }
        total = merge(&total, &item.estimate);
    }
    writeln!(out, "Total: {}", size_text(&total))?;
    if search_incomplete && items.iter().any(|i| i.rules[0].0.category == "Projects") {
        writeln!(
            out,
            "The project search stopped early; some project folders may be missing. Use --scan-timeout to search longer."
        )?;
    }

    if !confirm(
        "Permanently delete these files?",
        assume_yes,
        interactive,
        input,
        out,
    )? {
        writeln!(out, "Aborted. Nothing was deleted.")?;
        return Ok(());
    }

    let mut failures = 0;
    let mut freed = Estimate::exact(0);
    for item in &items {
        let mut item_failures = vec![];
        for (rule, nested) in &item.rules {
            item_failures.extend(clean_rule(rule, nested));
        }
        if item_failures.is_empty() {
            writeln!(out, "  cleaned {}", item.name)?;
            freed = merge(&freed, &item.estimate);
        } else {
            for (path, e) in &item_failures {
                writeln!(out, "  could not clean {}: {e}", path.display())?;
            }
            failures += item_failures.len();
        }
    }
    writeln!(out, "Freed about {}.", size_text(&freed))?;
    if failures > 0 {
        bail!("cleaned partially: {failures} failed");
    }
    Ok(())
}

fn size_text(size: &Estimate) -> String {
    let bytes = format_bytes(size.bytes);
    if size.incomplete {
        format!("≥ {bytes}")
    } else {
        bytes
    }
}
