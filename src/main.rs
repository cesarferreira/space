use anyhow::Result;
use clap::Parser;
use space::cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    print!("{}", space::app::run(&cli)?);
    Ok(())
}
