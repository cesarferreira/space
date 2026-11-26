use anyhow::Result;
use clap::Parser;
use space::{cli::Cli, platform, storage::topology, trash::Trash};
use std::{
    io::IsTerminal,
    time::{Duration, Instant},
};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let stdin = std::io::stdin();
    let interactive = stdin.is_terminal();

    if cli.empty_trash || cli.clean.is_some() {
        let report = topology::normalize(platform::discover()?);
        let mounts = space::app::mount_paths(&report);
        if cli.empty_trash {
            let trashes = Trash::detect_all(&mounts);
            if trashes.is_empty() {
                anyhow::bail!("no trash location is known for this platform");
            }
            return space::trash::confirm_and_empty(
                &trashes,
                cli.yes,
                interactive,
                &mut stdin.lock(),
                &mut std::io::stdout(),
            );
        }
        // Cleaning deletes data, so search and measure longer than the report does.
        let budget = cli.scan_budget().max(Duration::from_secs(10));
        let start = Instant::now();
        let (rules, incomplete) = space::app::rules_for(&report, start, budget);
        return space::clean::run(
            &rules,
            cli.clean.as_deref().unwrap_or_default(),
            incomplete,
            Some(start + budget),
            cli.yes,
            interactive,
            &mut stdin.lock(),
            &mut std::io::stdout(),
        );
    }

    let outcome = space::app::run(&cli)?;
    print!("{}", outcome.text);
    if let Some(warning) = outcome.warning {
        eprintln!("space: {warning}");
    }
    if outcome.exit_code != 0 {
        std::process::exit(outcome.exit_code);
    }
    Ok(())
}
