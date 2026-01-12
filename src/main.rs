use crate::ui::Ui;
use clap::{Parser, Subcommand};
use colored::*;

mod assets;
mod build;
mod commands;
mod error;
mod git;
mod manifest;
mod packer;
mod shdc;
mod ui;

use commands::build_cmd::{self, BuildArgs};
use commands::docs::{self, DocsArgs};
use commands::init::{self, InitArgs};
use commands::install::{self, InstallArgs};
use commands::remove::{self, RemoveArgs};
use commands::run::{self, RunArgs};

#[derive(Parser)]
#[command(
    name = "bonsai",
    version = env!("CARGO_PKG_VERSION"),
    about = "Bonsai is a tool for managing games made in the Bonsai framework.",
    author,
)]
struct Cli {
    #[arg(short, long)]
    verbose: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init(InitArgs),
    Run(RunArgs),
    Build(BuildArgs),
    Install(InstallArgs),
    Remove(RemoveArgs),
    Docs(DocsArgs),
}

fn handle_result(res: Result<(), crate::error::CustomError>, context: &str) {
    if let Err(e) = res {
        eprintln!("{}: {}.", format!("[ERROR] ({})", context).red().bold(), e);
        std::process::exit(1);
    }
}

fn main() {
    let _ = enable_ansi_support::enable_ansi_support();

    let cli = Cli::parse();

    let ui = Ui::new(cli.verbose);

    match &cli.command {
        Commands::Init(args) => handle_result(init::init(args, ui.clone()), "init"),
        Commands::Run(args) => handle_result(run::run(args, ui.clone()), "run"),
        Commands::Build(args) => handle_result(build_cmd::build(args, ui.clone()), "build"),
        Commands::Install(args) => handle_result(install::install(args, ui.clone()), "install"),
        Commands::Remove(args) => handle_result(remove::remove(args, ui.clone()), "remove"),
        Commands::Docs(args) => handle_result(docs::docs(args, ui.clone()), "docs"),
    }
}
