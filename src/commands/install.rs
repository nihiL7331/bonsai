use crate::error::CustomError;
use crate::git::clone_with_progress;
use crate::manifest::update_manifest;
use clap::Args;
use colored::Colorize;
use std::path::Path;
use url::Url;

const SYSTEMS_DIR: &str = "bonsai/systems";

#[derive(Args)]
pub struct InstallArgs {
    pub url: String,
    #[arg(long, short, default_value = "latest")]
    pub version: String,
    #[arg(long, short)]
    pub name: Option<String>,
    #[arg(long)]
    pub verbose: bool,
}

pub fn install(args: &InstallArgs) -> Result<(), CustomError> {
    let folder_name = match &args.name {
        Some(n) => n.clone(),
        None => extract_name_from_url(&args.url)?,
    };

    if folder_name.contains('/') || folder_name.contains('\\') {
        return Err(CustomError::ValidationError("Invalid system name.".into()));
    }

    let systems_dir = Path::new(SYSTEMS_DIR);
    if !systems_dir.exists() {
        return Err(CustomError::ValidationError(
            "source/systems directory not found. Are you in a bonsai project?".to_string(),
        ));
    }

    let target_path = systems_dir.join(&folder_name);

    if target_path.exists() {
        return Err(CustomError::ValidationError(format!(
            "System '{}' is already installed at {:?}",
            folder_name, target_path
        )));
    }

    if args.verbose {
        println!(
            "{} Installing system '{}'...",
            "[INFO]".green(),
            folder_name
        );
    }

    clone_with_progress(&args.url, &target_path, &args.version)?;

    let git_dir = target_path.join(".git");
    if git_dir.exists() {
        std::fs::remove_dir_all(git_dir)?;
    }

    if args.verbose {
        println!("{} Updating manifest...", "[INFO]".green());
    }
    update_manifest(Path::new("."))?;

    println!(
        "{} Installed {} successfully.",
        "[INFO]".green(),
        folder_name
    );
    Ok(())
}

fn extract_name_from_url(url_str: &str) -> Result<String, CustomError> {
    // TODO: ssh
    let url = Url::parse(url_str)
        .map_err(|e| CustomError::ValidationError(format!("Invalid URL: {}", e)))?;

    let segments = url
        .path_segments()
        .ok_or(CustomError::ValidationError("URL has no path".into()))?;
    let last = segments
        .filter(|s| !s.is_empty())
        .last()
        .ok_or(CustomError::ValidationError("URL has no segments".into()))?;

    let name = if last.ends_with(".git") {
        last.trim_end_matches(".git")
    } else {
        last
    };

    if name.is_empty() {
        return Err(CustomError::ValidationError(
            "Could not determine system name from URL".into(),
        ));
    }

    Ok(name.to_string())
}
