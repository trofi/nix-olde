use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum OldeError {
    /// Running external command failed for some reason.
    #[error("command {cmd:?} failed: {output:?}")]
    CommandFailed {
        cmd: Vec<String>,
        output: std::process::Output,
    },

    // Multiple errors happened. See individual entries for an
    // explanation.
    #[error("multiple errors: {0:?}")]
    MultipleErrors(Vec<OldeError>),

    // Cancelled externally.
    #[error("canceled {0}")]
    Canceled(String),

    // Unexpected empty output.
    #[error("unexpected empty output from {0}")]
    EmptyOutput(String),

    // IO error: symlink resolution, file read failures.
    #[error("IO error: {0}")]
    IOError(std::io::Error),

    #[error("JSON parse error: {0}")]
    JSONError(serde_json::Error),

    #[error("UTF8 decoding error: {0}")]
    UTF8Error(std::string::FromUtf8Error),
}

impl From<std::io::Error> for OldeError {
    fn from(error: std::io::Error) -> Self {
        OldeError::IOError(error)
    }
}

impl From<serde_json::Error> for OldeError {
    fn from(error: serde_json::Error) -> Self {
        OldeError::JSONError(error)
    }
}

impl From<std::string::FromUtf8Error> for OldeError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        OldeError::UTF8Error(error)
    }
}
