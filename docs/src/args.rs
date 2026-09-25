use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Clone, Subcommand)]
#[command()]
pub enum Command {
    Compile(CompileCommand),
    Watch(WatchCommand),
}

#[derive(Clone, Parser)]
pub struct CompileCommand {
    #[clap(flatten)]
    pub args: CompileArgs,
}

#[derive(Clone, Parser)]
pub struct WatchCommand {
    #[clap(flatten)]
    pub args: CompileArgs,
}

#[derive(Clone, Parser)]
pub struct CompileArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,

    #[arg(long = "pdf-output", value_name = "OUTPUT_PDF")]
    pub pdf_output: Option<PathBuf>,

    #[arg(long = "no-pdf")]
    pub no_pdf: bool,

    #[arg(long = "timings", value_name = "OUTPUT_JSON")]
    pub timings: Option<PathBuf>,

    #[arg(long)]
    pub open: bool,

    /// Examples to run at the same time (default: available CPU cores).
    #[arg(long, value_name = "N")]
    pub jobs: Option<usize>,

    /// Finish with success even when examples fail (they still show their
    /// errors on the page). Watch mode always allows them.
    #[arg(long = "allow-example-errors")]
    pub allow_example_errors: bool,
}
