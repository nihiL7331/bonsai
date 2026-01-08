use crate::Ui;
use crate::error::CustomError;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

pub fn clone_repo(
    full_url: &str,
    destination: &Path,
    version: &str,
    ui: &Ui,
) -> Result<(), CustomError> {
    let mut args = vec!["clone", "--depth", "1"];

    ui.status("Initializing template repository...");

    if version != "latest" {
        args.push("--branch");
        args.push(version);
    }

    args.push(full_url);
    args.push(
        destination
            .to_str()
            .ok_or_else(|| CustomError::ValidationError("Invalid destination path".to_string()))?,
    );

    let output = Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| CustomError::IoError(e))?;

    if output.status.success() {
        ui.log("Download complete.");
        Ok(())
    } else {
        let error_msg = String::from_utf8_lossy(&output.stderr);

        if error_msg.to_lowercase().contains("remote branch") || error_msg.contains("not found") {
            return Err(CustomError::GitError(format!(
                "Version/branch '{}' not found",
                version
            )));
        }

        Err(CustomError::GitError(format!(
            "Git clone failed: {}",
            error_msg
        )))
    }
}

pub fn clone_repo_to_temp(full_url: &str, version: &str, ui: &Ui) -> Result<TempDir, CustomError> {
    let temp_dir = TempDir::new().map_err(|e| CustomError::IoError(e))?;

    clone_repo(full_url, temp_dir.path(), version, ui)?;

    Ok(temp_dir)
}
