use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use serde::Deserialize;

use super::command::{CargoCommand, CommandOutput};

#[derive(Debug, Deserialize)]
pub(crate) struct MetadataDocument {
    pub(crate) packages: Vec<MetadataPackage>,
    pub(crate) resolve: Option<MetadataResolve>,
}

impl MetadataDocument {
    pub(crate) fn load(manifest_path: &Path, locked: bool) -> Result<Self> {
        let mut command = Command::new(CargoCommand::executable());

        command
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .arg("--manifest-path")
            .arg(manifest_path);

        if locked {
            command.arg("--locked");
        }

        let output = CommandOutput::execute(&mut command, "cargo metadata")?;

        serde_json::from_str(&output.stdout).context("failed to deserialize cargo metadata")
    }

    pub(crate) fn find_single_package(&self, name: &str) -> Result<&MetadataPackage> {
        let packages = self
            .packages
            .iter()
            .filter(|package| package.name == name)
            .collect::<Vec<_>>();

        anyhow::ensure!(
            packages.len() == 1,
            "expected exactly one {name} package, found {}",
            packages.len(),
        );

        Ok(packages[0])
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct MetadataPackage {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) source: Option<String>,
    pub(crate) manifest_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MetadataResolve {
    pub(crate) nodes: Vec<MetadataNode>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MetadataNode {
    pub(crate) id: String,

    #[serde(default)]
    pub(crate) deps: Vec<MetadataDependency>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MetadataDependency {
    pub(crate) pkg: String,
}
