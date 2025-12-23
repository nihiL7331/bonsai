use crate::error::CustomError;
use crate::git::clone_with_progress;
use crate::manifest::create_manifest;
use clap::Args;
use colored::Colorize;
use std::fs;
use std::path::Path;

const REPO_URL: &str = "https://github.com/nihiL7331/bonsai-2d.git";

#[derive(Args)]
pub struct InitArgs {
    pub name: String,
    #[arg(long, short)]
    pub dir: Option<String>,
    #[arg(long, short, default_value = "latest")]
    pub version: String,
    #[arg(long)]
    pub verbose: bool,
}

pub fn init(args: &InitArgs) -> Result<(), CustomError> {
    if args.name.is_empty() {
        return Err(CustomError::ValidationError(
            "Project name cannot be empty".to_string(),
        ));
    }

    let invalid_chars: &[char] = if cfg!(windows) {
        &['<', '>', ':', '"', '|', '?', '*']
    } else {
        &['/', '\0']
    };

    if args.name.chars().any(|c| invalid_chars.contains(&c)) {
        return Err(CustomError::ValidationError(format!(
            "Project name contains invalid characters: {:?}",
            invalid_chars
        )));
    }

    let destination_dir = args.dir.as_deref().unwrap_or(&args.name);
    let destination = Path::new(destination_dir);

    let should_cleanup = if destination.exists() {
        if !destination.is_dir() {
            return Err(CustomError::ValidationError(format!(
                "'{}' exists but is not a directory",
                destination.display()
            )));
        }

        let is_empty = fs::read_dir(destination)
            .map_err(CustomError::IoError)?
            .next()
            .is_none();

        if !is_empty {
            return Err(CustomError::ValidationError(format!(
                "Destination '{}' already exists and is not empty",
                destination.display()
            )));
        }

        false
    } else {
        true
    };
    let cleanup_on_fail = scopeguard::guard(should_cleanup, |should| {
        if should && destination.exists() {
            println!("{} Cleaning up partial installation...", "[INFO]".green());
            let _ = fs::remove_dir_all(destination);
        }
    });

    println!("{} Initializing project '{}'", "[INFO]".green(), args.name);

    clone_with_progress(REPO_URL, destination, &args.version)?;

    let git_dir = destination.join(".git");
    if git_dir.exists() {
        fs::remove_dir_all(&git_dir)?;
    }

    create_manifest(destination, &args.name)?;

    println!(
        "{} Project {} initialized successfully.",
        "[INFO]".green(),
        args.name
    );

    scopeguard::ScopeGuard::into_inner(cleanup_on_fail);

    Ok(())
}
