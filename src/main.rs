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

use commands::build_cmd::{self, BuildArgs};
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
}

fn handle_result(res: Result<(), crate::error::CustomError>, context: &str) {
    if let Err(e) = res {
        eprintln!("{}: {}.", format!("[ERROR] ({})", context).red().bold(), e);
        std::process::exit(1);
    }
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init(args) => handle_result(init::init(args), "init"),
        Commands::Run(args) => handle_result(run::run(args), "run"),
        Commands::Build(args) => handle_result(build_cmd::build(args), "build"),
        Commands::Install(args) => handle_result(install::install(args), "install"),
        Commands::Remove(args) => handle_result(remove::remove(args), "remove"),
    }
}
