use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use space::{
    app, cli::Cli, model::Estimate, output::terminal, platform, scan, storage::topology,
    trash::Trash,
};
use std::{
    io::{self, IsTerminal},
    sync::Mutex,
    time::{Duration, Instant},
};

mod dashboard;

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
        // Allow at least ten seconds to find and measure folders before cleaning.
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

    let outcome = run_report(&cli)?;
    print!("{}", outcome.text);
    if let Some(warning) = outcome.warning {
        eprintln!("space: {warning}");
    }
    if outcome.exit_code != 0 {
        std::process::exit(outcome.exit_code);
    }
    Ok(())
}

fn run_report(cli: &Cli) -> Result<app::Outcome> {
    let options = terminal::detect_options();
    if cli.no_scan || cli.json || options.ascii {
        return app::run(cli);
    }
    if cli.why.is_none() && !cli.reclaimable && !cli.diff {
        return if io::stdout().is_terminal() {
            dashboard::run(cli, options)
        } else {
            app::run(cli)
        };
    }
    if !io::stderr().is_terminal() {
        return app::run(cli);
    }

    let spinner = ProgressBar::new_spinner().with_style(ProgressStyle::with_template(
        "{spinner:.cyan} Scanning folders {elapsed_precise}\n{msg}",
    )?);
    spinner.enable_steady_tick(Duration::from_millis(80));
    let completed = Mutex::new(Vec::<(&str, Estimate)>::new());
    let result = app::run_with_progress(cli, |event| {
        let app::ScanProgress::FolderFinished {
            rule,
            estimate: Some(estimate),
        } = event
        else {
            return;
        };
        if estimate.bytes == 0 || (cli.reclaimable && !rule.reclaimability.is_reclaimable()) {
            return;
        }
        let mut completed = completed.lock().expect("progress state poisoned");
        let mut total = estimate.clone();
        if let Some(index) = completed.iter().position(|(name, _)| *name == rule.name) {
            total = scan::merge(&completed.remove(index).1, &total);
        }
        if matches!(rule.category, "Projects" | "Trash") {
            total.incomplete = true;
        }
        completed.push((rule.name, total));
        let message = completed
            .iter()
            .rev()
            .take(6)
            .map(|(name, size)| format!("  {name:<12}  {}", terminal::format_estimate(size)))
            .collect::<Vec<_>>()
            .join("\n");
        spinner.set_message(message);
    });
    spinner.finish_and_clear();
    result
}
