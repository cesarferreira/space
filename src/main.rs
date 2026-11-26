use anyhow::{Result, bail};
use clap::Parser;
use space::{cli::Cli, trash::Trash};
use std::io::IsTerminal;

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.empty_trash {
        let Some(trash) = Trash::detect() else {
            bail!("no trash location is known for this platform");
        };
        let stdin = std::io::stdin();
        let interactive = stdin.is_terminal();
        return space::trash::confirm_and_empty(
            &trash,
            cli.yes,
            interactive,
            &mut stdin.lock(),
            &mut std::io::stdout(),
        );
    }
    print!("{}", space::app::run(&cli)?);
    Ok(())
}
