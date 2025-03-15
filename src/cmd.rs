use std::process::Command;

use crate::error::*;

/// Runs 'cmd' and returns stdout or failure.
pub(crate) fn run_cmd(args: &[&str]) -> Result<Vec<u8>, OldeError> {
    let output = Command::new(args[0]).args(&args[1..]).output()?;

    if !output.status.success() {
        // Be verbose about all command run failures.
        log::info!("Failed running {:?}: {:?}", args, output.status);
        for l in output
            .stdout
            .split(|c| *c == b'\n')
            .filter(|e| !e.is_empty())
        {
            log::info!("out> {}", String::from_utf8_lossy(l));
        }
        for l in output
            .stderr
            .split(|c| *c == b'\n')
            .filter(|e| !e.is_empty())
        {
            log::info!("err> {}", String::from_utf8_lossy(l));
        }
        return Err(OldeError::CommandFailed {
            cmd: args.iter().map(|a| a.to_string()).collect(),
            output,
        });
    } else {
        log::debug!("Running {:?}: {:?}", args, output.status);
        for l in output
            .stdout
            .split(|c| *c == b'\n')
            .filter(|e| !e.is_empty())
        {
            log::trace!("out> {}", String::from_utf8_lossy(l));
        }
        for l in output
            .stderr
            .split(|c| *c == b'\n')
            .filter(|e| !e.is_empty())
        {
            log::trace!("err> {}", String::from_utf8_lossy(l));
        }
    }

    Ok(output.stdout)
}
