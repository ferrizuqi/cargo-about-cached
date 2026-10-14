use std::{
    ffi::OsString,
    process::{Command, Output},
};

use anyhow::{Context, Result};

pub(crate) struct CargoCommand;

impl CargoCommand {
    pub(crate) fn executable() -> OsString {
        std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
    }
}

pub(crate) struct CommandOutput {
    pub(crate) stdout: String,
}

impl CommandOutput {
    pub(crate) fn execute(command: &mut Command, description: &str) -> Result<Self> {
        let output = command
            .output()
            .with_context(|| format!("failed to execute {description}"))?;

        Self::check(output, description)
    }

    fn check(output: Output, description: &str) -> Result<Self> {
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        anyhow::ensure!(
            output.status.success(),
            "{description} failed with {}\n\
             \nstdout:\n{}\n\
             \nstderr:\n{}",
            output.status,
            stdout,
            stderr,
        );

        Ok(Self { stdout })
    }
}
