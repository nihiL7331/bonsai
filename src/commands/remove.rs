use crate::error::CustomError;
use crate::manifest::update_manifest;
use clap::Args;
use colored::*;
use std::io;
use std::io::Write;
use std::path::Path;

const SYSTEMS_DIR: &str = "source/systems";

#[derive(Args)]
pub struct RemoveArgs {
    pub name: String,
    #[arg(long, short)]
    pub yes: bool,
    #[arg(long)]
    pub verbose: bool,
}

pub fn remove(args: &RemoveArgs) -> Result<(), CustomError> {
    if args.name.contains('/') || args.name.contains('\\') {
        return Err(CustomError::ValidationError("Invalid system name.".into()));
    }

    let systems_dir = Path::new(SYSTEMS_DIR);
    let target_path = systems_dir.join(&args.name);

    if !target_path.exists() {
        return Err(CustomError::ValidationError(format!(
            "System '{}' not found.",
            args.name
        )));
    }

    if !args.yes {
        print!(
            "{} Are you sure you want to delete '{}'? [Y/N] ",
            "[WARNING]".yellow(),
            args.name.red().bold()
        );

        io::stdout().flush().map_err(CustomError::IoError)?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(CustomError::IoError)?;

        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            println!("{} Operation cancelled.", "[INFO]".green());
            return Ok(());
        }
    }

    if args.verbose {
        println!("{} Removing system '{}'...", "[INFO]".green(), args.name);
    }
    std::fs::remove_dir_all(&target_path)?;

    update_manifest(Path::new("."))?;

    println!("{} Removed.", "[INFO]".green());
    Ok(())
}
