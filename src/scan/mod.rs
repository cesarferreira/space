use crate::{
    classify::Rule,
    model::{Confidence, Estimate, Reclaimability, SemanticCategory, StorageReport},
};
use rayon::prelude::*;
use std::{
    fs,
    path::Path,
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
pub fn directory_size(path: &Path) -> Estimate {
    directory_size_before(path, None)
}

pub fn directory_size_with_budget(path: &Path, budget: Duration) -> Estimate {
    directory_size_before(path, Some(Instant::now() + budget))
}

fn directory_size_before(path: &Path, deadline: Option<Instant>) -> Estimate {
    fn walk(p: &Path, n: &mut usize, deadline: Option<Instant>) -> u64 {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            *n += 1;
            return 0;
        }
        let Ok(rd) = fs::read_dir(p) else {
            *n += 1;
            return 0;
        };
        let mut bytes = 0;
        for entry in rd {
            if deadline.is_some_and(|limit| Instant::now() >= limit) {
                *n += 1;
                break;
            }
            if let Ok(e) = entry {
                let Ok(m) = fs::symlink_metadata(e.path()) else {
                    *n += 1;
                    continue;
                };
                if !m.file_type().is_symlink() {
                    bytes += if m.is_dir() {
                        walk(&e.path(), n, deadline)
                    } else {
                        m.len()
                    };
                }
            }
        }
        bytes
    }
    let mut n = 0;
    let b = walk(path, &mut n, deadline);
    if n == 0 {
        Estimate::exact(b)
    } else {
        Estimate::lower_bound(b, n)
    }
}
pub fn scan_report(report: &mut StorageReport, rules: &[Rule]) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let found: Vec<_> = rules
        .par_iter()
        .filter(|r| r.path.exists())
        .map(|r| (r, directory_size_before(&r.path, Some(deadline))))
        .collect();
    for (rule, estimate) in found {
        if let Some(index) = owning_volume_index(report, &rule.path) {
            report.storage[index].categories.push(SemanticCategory {
                name: rule.name.into(),
                path: vec![rule.category.into(), rule.name.into()],
                estimate,
                confidence: rule.confidence,
                reclaimability: rule.reclaimability,
                children: vec![],
            });
        }
    }
    for v in report
        .storage
        .iter_mut()
        .filter(|v| !v.categories.is_empty())
    {
        let used = v.categories.iter().map(|c| c.estimate.bytes).sum();
        v.categories.push(SemanticCategory {
            name: "Other".into(),
            path: vec!["Other".into()],
            estimate: Estimate::exact(v.used_bytes.saturating_sub(used)),
            confidence: Confidence::Unknown,
            reclaimability: Reclaimability::Unknown,
            children: vec![],
        });
    }
}
