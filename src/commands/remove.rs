use crate::Ui;
use crate::error::CustomError;
use crate::manifest::update_manifest;
use clap::Args;
use colored::*;
use std::path::Path;

const SYSTEMS_DIR: &str = "bonsai/systems";

#[derive(Args)]
pub struct RemoveArgs {
    pub name: String,
    #[arg(long, short)]
    pub yes: bool,
}

pub fn remove(args: &RemoveArgs, ui: Ui) -> Result<(), CustomError> {
    if args.name.contains('/') || args.name.contains('\\') {
        return Err(CustomError::ValidationError("Invalid system name.".into()));
    }

    let systems_path = Path::new(SYSTEMS_DIR);
    let target_path = systems_path.join(&args.name);

    if !target_path.exists() {
        return Err(CustomError::ValidationError(format!(
            "System '{}' not found.",
            args.name
        )));
    }

    if !args.yes {
        let question = format!(
            "{} Are you sure you want to delete '{}'?",
            "[WARNING]".yellow(),
            args.name.red().bold()
        );
        if !ui.confirm(&question) {
            ui.error("Operation cancelled.");
            return Ok(());
        }
    }

    ui.status(&format!("Removing system '{}'...", args.name));
    std::fs::remove_dir_all(&target_path)?;



    update_manifest(Path::new("."), &ui)?;

    ui.success(&format!("Removed system '{}'", args.name));
    Ok(())
}
