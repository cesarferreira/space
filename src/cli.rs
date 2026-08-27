use clap::{ArgGroup, Parser};

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

#[derive(Debug, Parser)]
#[command(
    version,
    about,
    group(ArgGroup::new("focus").args(["why", "reclaimable"]).multiple(false))
)]
pub struct Cli {
    #[arg(long)]
    pub all: bool,
    #[arg(long, value_name = "MOUNT_OR_CATEGORY")]
    pub why: Option<String>,
    #[arg(long)]
    pub reclaimable: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub no_scan: bool,
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
}
