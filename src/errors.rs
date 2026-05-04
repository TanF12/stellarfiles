use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum AppError {
    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Security violation: {0}")]
    Security(String),
    #[error("Operation cancelled.")]
    Cancelled,
    #[error("Target already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("Archive error: {0}")]
    Archive(String),
    #[error("Task failed: {0}")]
    Task(String),
}

impl AppError {
    pub fn security(msg: &str) -> Self {
        AppError::Security(msg.to_string())
    }
}
