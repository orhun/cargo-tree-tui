use clap::Parser as _;

mod cli;
mod commands;

fn main() -> anyhow::Result<()> {
    let command = cli::Command::parse();
    command.exec()
}
