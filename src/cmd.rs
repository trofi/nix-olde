use std::process::Command;

use crate::error::*;

/// Runs 'cmd' and returns stdout or failure.
pub(crate) fn run_cmd(args: &[&str]) -> Result<Vec<u8>, OldeError> {
    let output = Command::new(args[0]).args(&args[1..]).output()?;

    if !output.status.success() {
        // Be verbose about all command run failures.
        log::info!("Failed running {:?}: {:?}", args, output.status);
        log::info!("Result of {:?}: {:?}", args, output);
        return Err(OldeError::CommandFailed {
            cmd: args.iter().map(|a| a.to_string()).collect(),
            output,
        });
    } else {
        log::debug!("Running {:?}: {:?}", args, output.status);
        log::trace!("Result of {:?}: {:?}", args, output);
    }

    Ok(output.stdout)
}
