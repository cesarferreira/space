use clap::{ArgGroup, Parser};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputMode {
    Terminal,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanMode {
    KnownPaths,
    None,
}

fn seconds(value: &str) -> Result<f64, String> {
    let n: f64 = value
        .parse()
        .map_err(|_| format!("'{value}' is not a number"))?;
    if n.is_finite() && n > 0.0 && n <= 3_600.0 {
        Ok(n)
    } else {
        Err("must be more than 0 and at most 3600 seconds".into())
    }
}

fn percent(value: &str) -> Result<f64, String> {
    let n: f64 = value
        .trim_end_matches('%')
        .parse()
        .map_err(|_| format!("'{value}' is not a number"))?;
    if (0.0..=100.0).contains(&n) {
        Ok(n)
    } else {
        Err("must be between 0 and 100".into())
    }
}

#[derive(Debug, Parser)]
#[command(
    version,
    about,
    group(ArgGroup::new("focus").args(["why", "reclaimable", "diff", "clean", "empty_trash"]).multiple(false)),
    group(ArgGroup::new("deletes").args(["clean", "empty_trash"]).multiple(false))
)]
pub struct Cli {
    /// Show every hidden or derived mount
    #[arg(long)]
    pub all: bool,
    /// Explain a mount, filesystem type, or folder category (such as gradle or trash)
    #[arg(long, value_name = "MOUNT_OR_CATEGORY")]
    pub why: Option<String>,
    /// With --why: how many of the largest entries inside the category to list
    #[arg(long, value_name = "N", requires = "why", default_value_t = 5)]
    pub top: usize,
    /// List the space you could free, largest first
    #[arg(long)]
    pub reclaimable: bool,
    /// Emit machine-readable JSON
    #[arg(long)]
    pub json: bool,
    /// Skip measuring folders; show capacity only
    #[arg(long)]
    pub no_scan: bool,
    /// Seconds to spend measuring folders
    #[arg(long, value_name = "SECONDS", default_value = "2", value_parser = seconds)]
    pub scan_timeout: f64,
    /// Exit with status 3 when any volume is at least PERCENT full
    #[arg(long, value_name = "PERCENT", value_parser = percent)]
    pub fail_above: Option<f64>,
    /// Show what changed since the previous --diff run, then save a new snapshot
    #[arg(long, conflicts_with_all = ["json", "all"])]
    pub diff: bool,
    /// Delete reclaimable data in these comma-separated categories, after confirmation
    #[arg(long, value_name = "CATEGORIES", conflicts_with_all = ["all", "json", "no_scan", "fail_above"])]
    pub clean: Option<String>,
    /// Show the trash size and permanently delete its contents after confirmation
    #[arg(long, conflicts_with_all = ["all", "json", "no_scan", "fail_above"])]
    pub empty_trash: bool,
    /// Skip the confirmation prompt of --clean and --empty-trash
    #[arg(long, short = 'y', requires = "deletes")]
    pub yes: bool,
}

impl Cli {
    pub fn output_mode(&self) -> OutputMode {
        if self.json {
            OutputMode::Json
        } else {
            OutputMode::Terminal
        }
    }

    pub fn scan_mode(&self) -> ScanMode {
        if self.no_scan || self.why.is_some() {
            ScanMode::None
        } else {
            ScanMode::KnownPaths
        }
    }

    pub fn scan_budget(&self) -> Duration {
        Duration::from_secs_f64(self.scan_timeout)
    }
}
