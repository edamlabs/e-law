use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use law_config::Source;
use law_model::Selector;
use law_report::{Format, Output, Table};

use crate::asked::Asked;

/// The command line.
#[derive(Debug, Parser)]
#[command(
    name = "law",
    version,
    about = "Checks a project against its law",
    arg_required_else_help = true
)]
pub struct Cli {
    /// The command.
    #[command(subcommand)]
    pub command: Command,
    /// Say on stderr what the run counted: the classes the `members` engine judged and skipped.
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

/// What `law` is asked to do.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check the project and report what breaks its law.
    Check(CheckArgs),
    /// Prove every selected rule against its own cases.
    Test(TestArgs),
    /// Show a rule, or list the selected rules.
    Rule(RuleArgs),
    /// Print the JSON Schema of a file the tool reads or writes.
    Schema(SchemaArgs),
    /// Write a saved JSON report in another format.
    Report(ReportArgs),
    /// Count the rules by the values of a tag, and list what stays prose.
    Coverage(CoverageArgs),
}

/// The project a command works on, and the rules it selects.
#[derive(Debug, Args)]
pub struct ProjectArgs {
    /// The rules selected, replacing the configuration's.
    #[arg(long)]
    pub select: Vec<Selector>,
    /// The rules ignored, replacing the configuration's.
    #[arg(long)]
    pub ignore: Vec<Selector>,
    /// The configuration file, instead of the one discovered from the working directory.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// The arguments of `law check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// The paths checked; none, the configured roots.
    pub paths: Vec<PathBuf>,
    /// The project and the selection.
    #[command(flatten)]
    pub project: ProjectArgs,
    /// How the report is written: text, json, sarif or md.
    #[arg(long, default_value_t = Output::Text)]
    pub output_format: Output,
    /// Apply the safe fixes, then check again, until none applies.
    #[arg(long)]
    pub fix: bool,
    /// Apply the unsafe fixes too.
    #[arg(long, requires = "fix")]
    pub unsafe_fixes: bool,
}

/// The arguments of `law test`.
#[derive(Debug, Args)]
pub struct TestArgs {
    /// The project and the selection.
    #[command(flatten)]
    pub project: ProjectArgs,
    /// The catalogues tested, built-in names or directories, replacing the configuration's; no configuration needed.
    #[arg(long = "catalogue")]
    pub catalogues: Vec<Source>,
    /// How the trials are written: text or json.
    #[arg(long, default_value_t = Format::Text)]
    pub output_format: Format,
}

/// The arguments of `law rule`.
#[derive(Debug, Args)]
pub struct RuleArgs {
    /// The rule shown; none, every selected rule listed.
    pub id: Option<Asked>,
    /// The project and the selection.
    #[command(flatten)]
    pub project: ProjectArgs,
    /// The catalogues read, built-in names or directories, replacing the configuration's; no configuration needed.
    #[arg(long = "catalogue")]
    pub catalogues: Vec<Source>,
    /// How the rule is written: text or json.
    #[arg(long, default_value_t = Format::Text)]
    pub output_format: Format,
}

/// The arguments of `law report`.
#[derive(Debug, Args)]
pub struct ReportArgs {
    /// The report, as `law check --output-format json` wrote it.
    pub path: PathBuf,
    /// How it is written: text, json, sarif or md.
    #[arg(long, default_value_t = Output::Text)]
    pub format: Output,
}

/// The arguments of `law coverage`.
#[derive(Debug, Args)]
pub struct CoverageArgs {
    /// The tag key the rules are counted by.
    #[arg(long, default_value = "law")]
    pub by: String,
    /// The project and the selection.
    #[command(flatten)]
    pub project: ProjectArgs,
    /// The catalogues counted, built-in names or directories, replacing the configuration's; no configuration needed.
    #[arg(long = "catalogue")]
    pub catalogues: Vec<Source>,
    /// How the table is written: text, md or json.
    #[arg(long, default_value_t = Table::Text)]
    pub output_format: Table,
}

/// The arguments of `law schema`.
#[derive(Debug, Args)]
pub struct SchemaArgs {
    /// The schema printed; none, all five keyed by name.
    pub name: Option<SchemaName>,
}

/// The files `law schema` describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SchemaName {
    /// `law.toml`, or `[tool.law]`.
    Config,
    /// A catalogue's rule file.
    Rules,
    /// A catalogue's profile file.
    Profiles,
    /// An exceptions file.
    Exceptions,
    /// A run's report.
    Report,
}
