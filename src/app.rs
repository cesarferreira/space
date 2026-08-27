use crate::{
    cli::{Cli, OutputMode},
    platform,
    storage::topology,
};
use anyhow::Result;
pub fn run(cli: &Cli) -> Result<String> {
    let mut report = topology::normalize(platform::discover()?);
    if !cli.no_scan && cli.why.is_none() {
        crate::scan::scan_report(&mut report, &crate::classify::known_rules());
    }
    if let Some(q) = &cli.why {
        return crate::explain::render(&report, q);
    }
    match cli.output_mode() {
        OutputMode::Json => crate::output::json::render(&report),
        OutputMode::Terminal => Ok(crate::output::terminal::render(&report, cli.all)),
    }
}
