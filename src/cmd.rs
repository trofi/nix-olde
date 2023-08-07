use std::process::Command;

use crate::error::*;

/// Runs 'cmd' and returns stdout or failure.
pub(crate) fn run_cmd(args: &[&str]) -> Result<Vec<u8>, OldeError> {
    let output = Command::new(args[0]).args(&args[1..]).output()?;
    log::debug!("Running {:?}: {:?}", args, output.status);
    log::trace!("Result of {:?}: {:?}", args, output);

    if !output.status.success() {
        return Err(OldeError::CommandFailed {
            cmd: args.iter().map(|a| a.to_string()).collect(),
            output,
        });
    }

    Ok(output.stdout)
}
