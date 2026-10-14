mod config;

use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, anyhow};
use cargo_about::Krates;
use cargo_lock::Lockfile;
use krates::{
    Builder, DepKind, LockOptions, NoneFilter, Scope, Utf8Path, Utf8PathBuf,
    cm::{Metadata, MetadataCommand},
};

pub use crate::workspace::config::{ConfigFingerprint, WorkspaceConfig};

#[derive(Debug)]
pub struct Workspace {
    manifest_path: Utf8PathBuf,
    config: WorkspaceConfig,
}

#[derive(Debug)]
pub struct WorkspaceProbe {
    metadata: serde_json::Value,
    target_directory: Utf8PathBuf,
    lockfile: Lockfile,
    cargo_version: String,
}

impl WorkspaceProbe {
    pub fn metadata(&self) -> &serde_json::Value {
        &self.metadata
    }
    pub fn target_directory(&self) -> &Utf8Path {
        &self.target_directory
    }
    pub fn lockfile(&self) -> &Lockfile {
        &self.lockfile
    }
    pub fn cargo_version(&self) -> &str {
        &self.cargo_version
    }
}

impl Workspace {
    pub fn open(manifest_path: Option<PathBuf>) -> Result<Self> {
        let manifest_path = Self::resolve_manifest_path(manifest_path)?;

        anyhow::ensure!(
            manifest_path.exists(),
            "Cargo manifest does not exist: {manifest_path}"
        );
        anyhow::ensure!(
            manifest_path.is_file(),
            "Cargo manifest must be a file: {manifest_path}"
        );

        let config = WorkspaceConfig::discover(&manifest_path)?;

        Ok(Self {
            manifest_path,
            config,
        })
    }

    pub fn manifest_path(&self) -> &Utf8Path {
        &self.manifest_path
    }
    pub fn config_path(&self) -> Option<&Utf8Path> {
        self.config.path()
    }
    pub fn config(&self) -> &cargo_about::licenses::config::Config {
        self.config.value()
    }
    pub fn config_fingerprint(&self) -> &ConfigFingerprint {
        self.config.fingerprint()
    }

    pub fn into_config(self) -> cargo_about::licenses::config::Config {
        self.config.into_value()
    }

    pub fn probe(&self) -> Result<WorkspaceProbe> {
        let mut command = self.metadata_command();
        command.no_deps();

        let output = command
            .cargo_command()
            .output()
            .context("failed to execute cargo metadata --no-deps")?;

        anyhow::ensure!(
            output.status.success(),
            "cargo metadata --no-deps exited with {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );

        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
            .context("failed to parse cargo metadata --no-deps output")?;

        let paths: ProbePaths = serde_json::from_slice(&output.stdout)
            .context("failed to read paths from cargo metadata --no-deps output")?;

        let lockfile_path = paths.workspace_root.join("Cargo.lock");
        let lockfile = Lockfile::load(&lockfile_path)
            .with_context(|| format!("failed to load Cargo.lock: {lockfile_path}"))?;

        let cargo_version = self.load_cargo_version()?;

        Ok(WorkspaceProbe {
            metadata,
            target_directory: paths.target_directory,
            lockfile,
            cargo_version,
        })
    }

    pub fn load_metadata(&self) -> Result<Metadata> {
        self.metadata_command()
            .exec()
            .context("cargo metadata failed")
    }

    pub fn build_graph(&self, metadata: &Metadata) -> Result<Krates> {
        let config = self.config();

        let mut builder = Builder::new();

        if config.ignore_build_dependencies {
            builder.ignore_kind(DepKind::Build, Scope::All);
        }
        if config.ignore_dev_dependencies {
            builder.ignore_kind(DepKind::Dev, Scope::All);
        }
        if config.ignore_transitive_dependencies {
            builder.ignore_kind(DepKind::Normal, Scope::NonWorkspace);
            builder.ignore_kind(DepKind::Dev, Scope::NonWorkspace);
            builder.ignore_kind(DepKind::Build, Scope::NonWorkspace);
        }

        if !config.targets.is_empty() {
            builder.include_targets(
                config
                    .targets
                    .iter()
                    .map(|target| (target.as_str(), vec![])),
            );
        }

        builder
            .build_with_metadata(metadata.clone(), NoneFilter)
            .context("failed to build crate graph")
    }

    fn metadata_command(&self) -> MetadataCommand {
        let mut command = krates::Cmd::new();

        command.manifest_path(&self.manifest_path);

        command.lock_opts(LockOptions {
            frozen: false,
            locked: true,
            offline: false,
        });

        MetadataCommand::from(command)
    }

    fn load_cargo_version(&self) -> Result<String> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());

        let output = Command::new(cargo)
            .arg("--version")
            .output()
            .context("failed to execute cargo --version")?;

        anyhow::ensure!(
            output.status.success(),
            "cargo --version exited with {}",
            output.status
        );

        let version = String::from_utf8(output.stdout)
            .context("cargo --version returned non UTF-8 output")?;

        Ok(version.trim().to_owned())
    }

    fn resolve_manifest_path(manifest_path: Option<PathBuf>) -> Result<Utf8PathBuf> {
        let path = match manifest_path {
            Some(path) => path,

            None => std::env::current_dir()
                .context("failed to determine current directory")?
                .join("Cargo.toml"),
        };

        Utf8PathBuf::from_path_buf(path)
            .map_err(|path| anyhow!("manifest path is not valid UTF-8: {}", path.display()))
    }
}

#[derive(serde::Deserialize)]
struct ProbePaths {
    workspace_root: Utf8PathBuf,
    target_directory: Utf8PathBuf,
}
