use crate::error::CustomError;
use git2::build::RepoBuilder;
use git2::{FetchOptions, Progress, RemoteCallbacks};
use indicatif::{ProgressBar, ProgressStyle};
use std::cell::RefCell;
use std::path::Path;

pub fn clone_with_progress(
    url: &str,
    destination: &Path,
    version: &str,
) -> Result<(), CustomError> {
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let pb_ref = RefCell::new(pb);
    let mut callbacks = RemoteCallbacks::new();

    callbacks.transfer_progress(|stats: Progress| {
        let pb = pb_ref.borrow();

        if stats.total_objects() > 0 {
            pb.set_length(stats.total_objects() as u64);
        } else {
            pb.set_style(ProgressStyle::default_spinner());
        }

        pb.set_position(stats.received_objects() as u64);

        if stats.received_bytes() > 0 {
            pb.set_message(format!("{} KB", stats.received_bytes() / 1024));
        }

        true
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    fetch_options.depth(1);

    let mut builder = RepoBuilder::new();
    if version != "latest" {
        builder.branch(version);
    }
    builder.fetch_options(fetch_options);

    builder.clone(url, destination).map_err(|e| {
        let error_msg = e.message().to_lowercase();
        if error_msg.contains("branch") && error_msg.contains("not found") {
            CustomError::ValidationError(format!("Version/branch '{}' not found", version))
        } else {
            CustomError::GitError(e)
        }
    })?;

    pb_ref.borrow().finish_with_message("Cloning complete.");

    Ok(())
}
