use thiserror::Error;

#[derive(Error, Debug, Clone)]
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
}
