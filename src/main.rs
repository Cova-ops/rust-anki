use clap::Parser;
use rust_anki::{cli, storage};

fn main() -> eyre::Result<()> {
    let cli = cli::Cli::parse();
    let mut connection = storage::connect_default()?;

    cli::run(cli, &mut connection)
}
