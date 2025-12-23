use thiserror::Error;

#[derive(Error, Debug)]
pub enum CustomError {
    #[error("Git operation failed: {0}")]
    GitError(#[from] git2::Error),
    #[error("I/O operation failed: {0}")]
    IoError(#[from] std::io::Error),
    #[error("TOML parsing failed: {0}")]
    TomlError(#[from] toml_edit::TomlError),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Build failed: {0}")]
    BuildError(String),
    #[error("Process execution failed: {0}")]
    ProcessError(String),
}
