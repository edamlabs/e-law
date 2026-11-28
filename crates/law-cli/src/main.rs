//! The `law` binary: the composition root.

mod asked;
mod builtin;
mod check;
mod cli;
mod commit;
mod coverage;
mod exit;
mod project;
mod report;
mod rule;
mod schema;
mod test;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check(args) => args.run(cli.verbose).into(),
        Command::Test(args) => args.run().into(),
        Command::Rule(args) => args.run().into(),
        Command::Schema(args) => args.run().into(),
        Command::Report(args) => args.run().into(),
        Command::Coverage(args) => args.run().into(),
    }
}
