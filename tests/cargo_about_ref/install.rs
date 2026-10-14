use std::{
    collections::BTreeSet,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};

use super::{
    command::CargoCommand,
    resolver::{ProjectCargoAbout, ReferenceResolver},
};

pub struct CargoAboutRef {
    executable: PathBuf,
    version: String,
}

impl CargoAboutRef {
    pub fn install(project_root: &Path) -> Result<Self> {
        println!("cargo-about-ref: resolving project cargo-about");
        let project = ProjectCargoAbout::load(project_root)?;

        println!("cargo-about-ref: creating reference resolver");
        let resolver = ReferenceResolver::create(project_root, &project)?;

        println!("cargo-about-ref: resolving reference dependency graph");
        let resolution = resolver.resolve(&project)?;

        let lockfile = resolution.create_lockfile()?;
        let lock_contents = lockfile.to_string();
        let lock_hash = sha256::digest(lock_contents.as_bytes());

        println!("cargo-about-ref: preparing locked reference build");
        let root = project_root
            .join("target")
            .join("test-tools")
            .join("cargo-about-ref")
            .join(&project.version)
            .join(lock_hash);

        let source_directory = root.join("source");
        let install_directory = root.join("install");
        let build_directory = root.join("build");

        let executable = install_directory
            .join("bin")
            .join(format!("cargo-about{}", std::env::consts::EXE_SUFFIX,));

        let reference = Self {
            executable,
            version: project.version.clone(),
        };
        if reference.matches_installed_version()? {
            println!(
                "cargo-about-ref: using cached cargo-about {}",
                reference.version,
            );

            return Ok(reference);
        }

        if root.exists() {
            std::fs::remove_dir_all(&root).with_context(|| {
                format!(
                    "failed to remove previous cargo-about reference: {}",
                    root.display(),
                )
            })?;
        }

        std::fs::create_dir_all(&source_directory).with_context(|| {
            format!(
                "failed to create cargo-about reference source directory: {}",
                source_directory.display(),
            )
        })?;

        DirectoryCopy::new(&project.source_directory, &source_directory)
            .exclude("Cargo.lock")
            .copy()?;

        std::fs::write(source_directory.join("Cargo.lock"), lock_contents)
            .context("failed to write cargo-about reference Cargo.lock")?;

        std::fs::create_dir_all(&install_directory).with_context(|| {
            format!(
                "failed to create cargo-about reference install directory: {}",
                install_directory.display(),
            )
        })?;

        let mut command = Command::new(CargoCommand::executable());

        command
            .arg("install")
            .arg("--path")
            .arg(&source_directory)
            .arg("--root")
            .arg(&install_directory)
            .arg("--features")
            .arg("cli")
            .arg("--locked")
            .arg("--force")
            .env("CARGO_TARGET_DIR", &build_directory);

        println!(
            "cargo-about-ref: building cargo-about {}...",
            reference.version,
        );

        let status = command
            .status()
            .context("failed to execute cargo install reference cargo-about")?;

        anyhow::ensure!(
            status.success(),
            "cargo install reference cargo-about failed with {status}",
        );

        anyhow::ensure!(
            reference.matches_installed_version()?,
            "installed cargo-about version does not match {}",
            reference.version,
        );

        Ok(reference)
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    fn matches_installed_version(&self) -> Result<bool> {
        if !self.executable.is_file() {
            return Ok(false);
        }

        let output = Command::new(&self.executable)
            .arg("--version")
            .output()
            .with_context(|| {
                format!("failed to execute {} --version", self.executable.display(),)
            })?;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8(output.stdout)
            .context("cargo-about --version returned non UTF-8 output")?;

        Ok(stdout.split_whitespace().last() == Some(self.version.as_str()))
    }
}

struct DirectoryCopy<'a> {
    source: &'a Path,
    destination: &'a Path,
    exclusions: BTreeSet<OsString>,
}

impl<'a> DirectoryCopy<'a> {
    fn new(source: &'a Path, destination: &'a Path) -> Self {
        Self {
            source,
            destination,
            exclusions: BTreeSet::new(),
        }
    }

    fn exclude(mut self, name: impl Into<OsString>) -> Self {
        self.exclusions.insert(name.into());

        self
    }

    fn copy(self) -> Result<()> {
        self.copy_directory(self.source, self.destination)
    }

    fn copy_directory(&self, source: &Path, destination: &Path) -> Result<()> {
        std::fs::create_dir_all(destination)
            .with_context(|| format!("failed to create directory: {}", destination.display(),))?;

        for entry in std::fs::read_dir(source)
            .with_context(|| format!("failed to read directory: {}", source.display(),))?
        {
            let entry = entry?;
            let file_name = entry.file_name();
            if self.exclusions.contains(&file_name) {
                continue;
            }

            let source_path = entry.path();
            let destination_path = destination.join(&file_name);
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                self.copy_directory(&source_path, &destination_path)?;
            } else if file_type.is_file() {
                std::fs::copy(&source_path, &destination_path).with_context(|| {
                    format!(
                        "failed to copy {} to {}",
                        source_path.display(),
                        destination_path.display(),
                    )
                })?;
            }
        }

        Ok(())
    }
}
