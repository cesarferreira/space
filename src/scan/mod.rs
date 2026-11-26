use crate::{
    classify::Rule,
    model::{Confidence, Estimate, Reclaimability, SemanticCategory, StorageReport},
};
use rayon::prelude::*;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub fn owning_volume_index(report: &StorageReport, path: &Path) -> Option<usize> {
    report
        .storage
        .iter()
        .enumerate()
        .filter(|(_, volume)| path.starts_with(&volume.representative_mount))
        .max_by_key(|(_, volume)| volume.representative_mount.components().count())
        .map(|(index, _)| index)
}

/// The space a file takes on disk. Sparse files (such as Docker's virtual disk) count
/// only their allocated blocks, like `du`.
fn disk_usage(m: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        m.blocks().saturating_mul(512)
    }
    #[cfg(not(unix))]
    {
        m.len()
    }
}

pub fn directory_size(path: &Path) -> Estimate {
    directory_size_before(path, None, &HashSet::new())
}

pub fn directory_size_with_budget(path: &Path, budget: Duration) -> Estimate {
    directory_size_before(path, Some(Instant::now() + budget), &HashSet::new())
}

/// The size of `path`, skipping any folder in `excluded`, stopping at `deadline`.
pub fn directory_size_before(
    path: &Path,
    deadline: Option<Instant>,
    excluded: &HashSet<PathBuf>,
) -> Estimate {
    fn walk(
        p: &Path,
        estimate: &mut Estimate,
        deadline: Option<Instant>,
        excluded: &HashSet<PathBuf>,
    ) {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            estimate.timed_out = true;
            return;
        }
        let Ok(rd) = fs::read_dir(p) else {
            estimate.inaccessible_paths += 1;
            return;
        };
        for entry in rd {
            if deadline.is_some_and(|limit| Instant::now() >= limit) {
                estimate.timed_out = true;
                return;
            }
            let Ok(entry) = entry else {
                estimate.inaccessible_paths += 1;
                continue;
            };
            let path = entry.path();
            if !excluded.is_empty() && excluded.contains(&path) {
                continue;
            }
            let Ok(m) = fs::symlink_metadata(&path) else {
                estimate.inaccessible_paths += 1;
                continue;
            };
            if !m.file_type().is_symlink() {
                if m.is_dir() {
                    walk(&path, estimate, deadline, excluded);
                } else {
                    estimate.bytes += disk_usage(&m);
                }
            }
        }
    }
    let mut estimate = Estimate::exact(0);
    walk(path, &mut estimate, deadline, excluded);
    estimate.incomplete = estimate.timed_out || estimate.inaccessible_paths > 0;
    estimate
}

/// For each rule, the paths of the other rules nested inside it, so nothing is counted twice.
pub fn nested_rule_paths(rules: &[Rule]) -> Vec<HashSet<PathBuf>> {
    rules
        .iter()
        .map(|rule| {
            rules
                .iter()
                .filter(|other| other.path != rule.path && other.path.starts_with(&rule.path))
                .map(|other| other.path.clone())
                .collect()
        })
        .collect()
}

/// Measure every rule that exists, in parallel, until `deadline`.
pub fn measure(rules: &[Rule], deadline: Option<Instant>) -> Vec<(usize, Estimate)> {
    let all: Vec<usize> = (0..rules.len()).collect();
    measure_selected(rules, &all, deadline)
}

/// Measure the rules at `indices` that exist. Folders of any other rule nested inside
/// a measured folder are skipped.
pub fn measure_selected(
    rules: &[Rule],
    indices: &[usize],
    deadline: Option<Instant>,
) -> Vec<(usize, Estimate)> {
    measure_with_progress(rules, indices, deadline, &|_, _| {})
}

pub(crate) fn measure_with_progress(
    rules: &[Rule],
    indices: &[usize],
    deadline: Option<Instant>,
    on_finished: &(impl Fn(&Rule, Option<&Estimate>) + Sync),
) -> Vec<(usize, Estimate)> {
    let nested = nested_rule_paths(rules);
    indices
        .par_iter()
        .filter_map(|&i| {
            let rule = &rules[i];
            let estimate = rule
                .path
                .exists()
                .then(|| directory_size_before(&rule.path, deadline, &nested[i]));
            on_finished(rule, estimate.as_ref());
            estimate.map(|estimate| (i, estimate))
        })
        .collect()
}

pub fn merge(a: &Estimate, b: &Estimate) -> Estimate {
    Estimate {
        bytes: a.bytes + b.bytes,
        incomplete: a.incomplete || b.incomplete,
        inaccessible_paths: a.inaccessible_paths + b.inaccessible_paths,
        timed_out: a.timed_out || b.timed_out,
    }
}

/// Attach the measured rules to their volumes as categories, then add "Other".
///
/// Rules that share a name on the same volume (such as many `node_modules` folders)
/// become one category. When `projects_incomplete` is true, project categories are
/// marked as lower bounds because the search for them stopped early.
pub fn scan_report(
    report: &mut StorageReport,
    rules: &[Rule],
    deadline: Option<Instant>,
    projects_incomplete: bool,
) {
    attach_estimates(report, rules, measure(rules, deadline), projects_incomplete);
}

pub(crate) fn attach_estimates(
    report: &mut StorageReport,
    rules: &[Rule],
    sizes: impl IntoIterator<Item = (usize, Estimate)>,
    projects_incomplete: bool,
) {
    for (index, mut estimate) in sizes {
        let rule = &rules[index];
        let Some(volume) = owning_volume_index(report, &rule.path) else {
            continue;
        };
        if projects_incomplete && rule.category == "Projects" {
            estimate.incomplete = true;
            estimate.timed_out = true;
        }
        let path = vec![rule.category.to_string(), rule.name.to_string()];
        let categories = &mut report.storage[volume].categories;
        if let Some(existing) = categories.iter_mut().find(|c| c.path == path) {
            existing.estimate = merge(&existing.estimate, &estimate);
        } else {
            categories.push(SemanticCategory {
                name: rule.name.into(),
                path,
                estimate,
                confidence: rule.confidence,
                reclaimability: rule.reclaimability,
                cleanable: rule.cleanable(),
                children: vec![],
            });
        }
    }
    for v in report
        .storage
        .iter_mut()
        .filter(|v| !v.categories.is_empty())
    {
        v.categories.sort_by(|a, b| a.path.cmp(&b.path));
        let used = v.categories.iter().map(|c| c.estimate.bytes).sum();
        v.categories.push(SemanticCategory {
            name: "Other".into(),
            path: vec!["Other".into()],
            estimate: Estimate::exact(v.used_bytes.saturating_sub(used)),
            confidence: Confidence::Unknown,
            reclaimability: Reclaimability::Unknown,
            cleanable: false,
            children: vec![],
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectKind {
    NodeModules,
    RustTarget,
}

/// Is `dir` a folder of project build output?
pub fn project_kind(dir: &Path) -> Option<ProjectKind> {
    let name = dir.file_name()?.to_str()?;
    let parent = dir.parent()?;
    match name {
        "node_modules" if parent.join("package.json").is_file() => Some(ProjectKind::NodeModules),
        // Cargo writes CACHEDIR.TAG into every target folder it creates.
        "target" if dir.join("CACHEDIR.TAG").is_file() && parent.join("Cargo.toml").is_file() => {
            Some(ProjectKind::RustTarget)
        }
        _ => None,
    }
}

const PROJECT_SEARCH_DEPTH: usize = 6;

/// Find `node_modules` and Cargo `target` folders under `roots`.
///
/// The search skips hidden folders, folders in `skip`, and anything inside a found folder.
/// It stops at `deadline`; the second value is true when it stopped early.
pub fn find_projects(
    roots: &[PathBuf],
    skip: &[PathBuf],
    deadline: Option<Instant>,
) -> (Vec<(PathBuf, ProjectKind)>, bool) {
    let mut found = vec![];
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> =
        roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = queue.pop_front() {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return (found, true);
        }
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') || skip.contains(&path) {
                continue;
            }
            if let Some(kind) = project_kind(&path) {
                found.push((path, kind));
            } else if depth + 1 < PROJECT_SEARCH_DEPTH {
                queue.push_back((path, depth + 1));
            }
        }
    }
    (found, false)
}

/// The `n` largest entries directly inside `path`, largest first.
pub fn largest_entries(
    path: &Path,
    n: usize,
    deadline: Option<Instant>,
) -> Vec<(PathBuf, Estimate)> {
    let Ok(rd) = fs::read_dir(path) else {
        return vec![];
    };
    let entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    let mut sized: Vec<(PathBuf, Estimate)> = entries
        .into_par_iter()
        .filter_map(|p| {
            let m = fs::symlink_metadata(&p).ok()?;
            let estimate = if m.file_type().is_symlink() {
                Estimate::exact(0)
            } else if m.is_dir() {
                directory_size_before(&p, deadline, &HashSet::new())
            } else {
                Estimate::exact(disk_usage(&m))
            };
            Some((p, estimate))
        })
        .collect();
    sized.sort_by_key(|(_, e)| std::cmp::Reverse(e.bytes));
    sized.truncate(n);
    sized
}
