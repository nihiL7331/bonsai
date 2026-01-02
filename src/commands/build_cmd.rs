use crate::Ui;
use crate::build::{build_desktop, build_web, clean_build};
use crate::error::CustomError;
use clap::Args;
use std::path::Path;

#[derive(Args)]
pub struct BuildArgs {
    #[arg(default_value = ".")]
    pub dir: String,
    #[arg(long, conflicts_with = "web")]
    pub desktop: bool,
    #[arg(long, conflicts_with = "desktop")]
    pub web: bool,
    #[arg(long, short = 'c', default_value = "debug")]
    pub config: String,
    #[arg(long)]
    pub clean: bool,
}

pub fn build(args: &BuildArgs, ui: Ui) -> Result<(), CustomError> {
    let project_dir = Path::new(&args.dir);
    if !project_dir.exists() {
        return Err(CustomError::ValidationError(format!(
            "Directory '{}' does not exist",
            args.dir
        )));
    }

    let manifest_path = project_dir.join("bonsai.toml");
    if !manifest_path.exists() {
        return Err(CustomError::ValidationError(format!(
            "Not a bonsai project: '{}'. (Missing bonsai.toml)",
            args.dir
        )));
    }

    let current_dir = std::env::current_dir()?;
    std::env::set_current_dir(project_dir)?;

    let _cleanup_on_fail = scopeguard::guard((), |_| {
        let _ = std::env::set_current_dir(&current_dir);
    });

    ui.log(&format!("Building project in: '{}'", project_dir.display()));

    if args.clean {
        clean_build(&ui)?;
    }

    if args.web {
        ui.log(&format!("Building for web ({})", args.config));
        build_web(&args.config, args.clean, &ui)?;
    } else {
        ui.log(&format!("Building for desktop ({})", args.config));
        build_desktop(&args.config, args.clean, &ui)?;
    }

    ui.success("Build completed successfully.");

    Ok(())
}
