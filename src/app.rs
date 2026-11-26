use crate::{
    classify::{self, Rule},
    cli::{Cli, OutputMode},
    model::{Estimate, StorageReport},
    platform, scan,
    storage::topology,
};
use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    thread,
    time::Instant,
};

/// Exit status when a volume is at least as full as `--fail-above`.
pub const FAIL_ABOVE_STATUS: i32 = 3;

pub struct Outcome {
    pub text: String,
    /// A message for stderr, such as the `--fail-above` breach.
    pub warning: Option<String>,
    pub exit_code: i32,
}

pub enum ScanProgress<'a> {
    Capacity(&'a StorageReport),
    Started {
        known: &'a [Rule],
        home: &'a Path,
    },
    ProjectsFound {
        rules: &'a [Rule],
        timed_out: bool,
    },
    FolderFinished {
        rule: &'a Rule,
        estimate: Option<&'a Estimate>,
    },
}

pub fn mount_paths(report: &StorageReport) -> Vec<PathBuf> {
    report
        .storage
        .iter()
        .flat_map(|v| v.mounts.iter().cloned())
        .collect()
}

/// The rules for this machine. The project search gets half of the scan budget.
pub fn rules_for(
    report: &StorageReport,
    start: Instant,
    budget: std::time::Duration,
) -> (Vec<crate::classify::Rule>, bool) {
    crate::classify::known_rules(&mount_paths(report), Some(start + budget / 2))
}

/// Volumes at least `limit` percent full, as a message.
pub fn fail_above(report: &StorageReport, limit: f64) -> Option<String> {
    let full: Vec<String> = report
        .storage
        .iter()
        .filter(|v| v.used_percent >= limit)
        .map(|v| format!("{} is {:.1}% full", v.name, v.used_percent))
        .collect();
    (!full.is_empty()).then(|| format!("{} (limit {limit}%)", full.join(", ")))
}

pub fn run(cli: &Cli) -> Result<Outcome> {
    run_with_progress(cli, |_| {})
}

/// Report capacity and folder scan progress as they become available.
pub fn run_with_progress(cli: &Cli, notify: impl Fn(ScanProgress<'_>) + Sync) -> Result<Outcome> {
    let mut report = topology::normalize(platform::discover()?);
    notify(ScanProgress::Capacity(&report));
    let start = Instant::now();
    let budget = cli.scan_budget();
    let deadline = Some(start + budget);
    let on_finished = |rule: &Rule, estimate: Option<&Estimate>| {
        notify(ScanProgress::FolderFinished { rule, estimate });
    };

    if let Some(q) = &cli.why {
        let rules = classify::rules_for_query(&mount_paths(&report), q, Some(start + budget / 2));
        let text = match crate::explain::category_with_progress(
            &rules,
            q,
            cli.top,
            deadline,
            &on_finished,
        ) {
            Some(text) => text,
            None => crate::explain::render(&report, q)?,
        };
        return Ok(Outcome {
            text,
            warning: None,
            exit_code: 0,
        });
    }

    if !cli.no_scan
        && let Some(home) = classify::home_dir()
    {
        let known = classify::known_folders(&home, &mount_paths(&report));
        notify(ScanProgress::Started {
            known: &known,
            home: &home,
        });
        let (known_sizes, (rules, project_sizes, projects_incomplete)) = thread::scope(|scope| {
            let projects = scope.spawn(|| {
                let (projects, incomplete) =
                    classify::project_rules(&home, &known, Some(start + budget / 2));
                notify(ScanProgress::ProjectsFound {
                    rules: &projects,
                    timed_out: incomplete,
                });
                // Project folders can contain a separately measured folder, such as a custom trash location.
                let mut rules = known.clone();
                rules.extend(projects);
                let indices: Vec<_> = (known.len()..rules.len()).collect();
                let sizes = scan::measure_with_progress(&rules, &indices, deadline, &on_finished);
                (rules, sizes, incomplete)
            });
            let indices: Vec<_> = (0..known.len()).collect();
            let sizes = scan::measure_with_progress(&known, &indices, deadline, &on_finished);
            (sizes, projects.join().expect("project scan panicked"))
        });
        scan::attach_estimates(
            &mut report,
            &rules,
            known_sizes.into_iter().chain(project_sizes),
            projects_incomplete,
        );
    }

    let text = if cli.reclaimable {
        match cli.output_mode() {
            OutputMode::Json => crate::output::reclaimable::render_json(&report)?,
            OutputMode::Terminal => crate::output::reclaimable::render_terminal(
                &report,
                crate::output::terminal::detect_options(),
            ),
        }
    } else if cli.diff {
        crate::history::diff_and_save(&report)?
    } else {
        match cli.output_mode() {
            OutputMode::Json => crate::output::json::render(&report)?,
            OutputMode::Terminal => crate::output::terminal::render(&report, cli.all),
        }
    };
    let warning = cli.fail_above.and_then(|limit| fail_above(&report, limit));
    Ok(Outcome {
        exit_code: if warning.is_some() {
            FAIL_ABOVE_STATUS
        } else {
            0
        },
        text,
        warning,
    })
}
