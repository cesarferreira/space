use crate::{
    cli::{Cli, OutputMode},
    model::StorageReport,
    platform,
    storage::topology,
};
use anyhow::Result;
use std::{path::PathBuf, time::Instant};

/// Exit status when a volume is at least as full as `--fail-above`.
pub const FAIL_ABOVE_STATUS: i32 = 3;

pub struct Outcome {
    pub text: String,
    /// A message for stderr, such as the `--fail-above` breach.
    pub warning: Option<String>,
    pub exit_code: i32,
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
    let mut report = topology::normalize(platform::discover()?);
    let start = Instant::now();
    let budget = cli.scan_budget();

    if let Some(q) = &cli.why {
        let (rules, _) = rules_for(&report, start, budget);
        let text = match crate::explain::category(&rules, q, cli.top, Some(start + budget)) {
            Some(text) => text,
            None => crate::explain::render(&report, q)?,
        };
        return Ok(Outcome {
            text,
            warning: None,
            exit_code: 0,
        });
    }

    if !cli.no_scan {
        let (rules, projects_incomplete) = rules_for(&report, start, budget);
        crate::scan::scan_report(
            &mut report,
            &rules,
            Some(start + budget),
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
