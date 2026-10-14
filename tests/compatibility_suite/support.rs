use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context, Result};
use tempfile::TempDir;

use crate::cargo_about_ref::CargoAboutRef;

pub(crate) struct CompatibilityContext {
    project_root: PathBuf,
    cargo_about: CargoAboutRef,
    cached_binary: PathBuf,
}

impl CompatibilityContext {
    pub(crate) fn new() -> Result<Self> {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cargo_about = CargoAboutRef::install(&project_root)?;
        let cached_binary = PathBuf::from(env!("CARGO_BIN_EXE_cargo-about-cached"));

        Ok(Self {
            project_root,
            cargo_about,
            cached_binary,
        })
    }

    pub(crate) fn project_root(&self) -> &Path {
        &self.project_root
    }
    pub(crate) fn cargo_about(&self) -> &CargoAboutRef {
        &self.cargo_about
    }
    pub(crate) fn cached_binary(&self) -> &Path {
        &self.cached_binary
    }
}

pub(crate) struct FixtureWorkspace {
    _temp: TempDir,
    root: PathBuf,
    target: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CacheState {
    pub(crate) metadata: bool,
    pub(crate) licenses: bool,
}

impl CacheState {
    pub(crate) fn is_empty(self) -> bool {
        !self.metadata && !self.licenses
    }
    pub(crate) fn is_populated(self) -> bool {
        self.metadata && self.licenses
    }

    pub(crate) fn assert_empty(self, phase: &str) {
        assert!(
            self.is_empty(),
            "{phase} must start without cache: {self:?}",
        );
    }
    pub(crate) fn assert_populated(self, phase: &str) {
        assert!(
            self.is_populated(),
            "{phase} must start with populated cache: {self:?}",
        );
    }
}

impl FixtureWorkspace {
    pub(crate) fn create(context: &CompatibilityContext, fixture: &str) -> Result<Self> {
        let temp = tempfile::tempdir().context("failed to create compatibility test directory")?;
        let root = temp.path().join("workspace");
        let source = context
            .project_root()
            .join("tests")
            .join("fixtures")
            .join(fixture);

        DirectoryCopy::new(&source, &root).copy()?;

        let target = temp.path().join("target");
        Ok(Self {
            _temp: temp,
            root,
            target,
        })
    }

    pub(crate) fn generate_lockfile(&self) -> Result<()> {
        let mut command = self.command(CargoCommand::executable());
        command.arg("generate-lockfile");
        CommandResult::execute(&mut command, "cargo generate-lockfile")?;

        Ok(())
    }

    pub(crate) fn update_package(&self, package: &str, version: &str) -> Result<()> {
        let mut command = self.command(CargoCommand::executable());

        command
            .arg("update")
            .arg("-p")
            .arg(package)
            .arg("--precise")
            .arg(version);

        CommandResult::execute(
            &mut command,
            &format!("cargo update -p {package} --precise {version}"),
        )?;

        Ok(())
    }

    pub(crate) fn replace_file_text(
        &self,
        relative_path: impl AsRef<Path>,
        from: &str,
        to: &str,
    ) -> Result<()> {
        let path = self.root.join(relative_path);
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read fixture file: {}", path.display(),))?;
        let occurrences = contents.matches(from).count();

        anyhow::ensure!(
            occurrences == 1,
            "expected exactly one occurrence of {:?} in {}, found {}",
            from,
            path.display(),
            occurrences,
        );

        let contents = contents.replacen(from, to, 1);
        std::fs::write(&path, contents)
            .with_context(|| format!("failed to update fixture file: {}", path.display(),))?;

        Ok(())
    }

    pub(crate) fn generate_baseline(
        &self,
        context: &CompatibilityContext,
        output_name: &str,
    ) -> Result<ArtifactHash> {
        let output = self.root.join(output_name);
        let mut command = self.command(context.cargo_about().executable());
        command
            .arg("generate")
            .arg("-o")
            .arg(&output)
            .arg(self.template_path());

        CommandResult::execute(&mut command, "cargo-about generate")?;
        ArtifactHash::from_file(&output)
    }

    pub(crate) fn run_cached(
        &self,
        context: &CompatibilityContext,
        output_name: &str,
    ) -> Result<CachedExecution> {
        let output = self.root.join(output_name);
        let mut command = self.command(context.cached_binary());
        command
            .arg(self.template_path())
            .arg("--output")
            .arg(&output);

        let result = CommandResult::execute(&mut command, "cargo-about-cached")?;
        Ok(CachedExecution {
            hash: ArtifactHash::from_file(&output)?,
            stdout: result.stdout,
        })
    }

    pub(crate) fn cache_state(&self) -> CacheState {
        let root = self.target.join(".cargo-about-cached");

        CacheState {
            metadata: root.join("metadata").is_dir(),
            licenses: root.join("licenses.json").is_file(),
        }
    }

    fn template_path(&self) -> PathBuf {
        self.root.join("about.hbs")
    }

    fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(&self.root)
            .env("CARGO_TARGET_DIR", &self.target);

        command
    }
}

pub(crate) struct CachedExecution {
    hash: ArtifactHash,
    stdout: String,
}

impl CachedExecution {
    pub(crate) fn hash(&self) -> &ArtifactHash {
        &self.hash
    }

    pub(crate) fn metadata_source(&self) -> Result<MetadataSource> {
        for line in self.stdout.lines() {
            if !line.starts_with("metadata: ") {
                continue;
            }
            if line.contains(", cargo (") {
                return Ok(MetadataSource::Cargo);
            }
            if line.contains(", cache (") {
                return Ok(MetadataSource::Cache);
            }
        }

        anyhow::bail!("metadata source was not found in output:\n{}", self.stdout,)
    }

    pub(crate) fn license_cache_stats(&self) -> Result<LicenseCacheStats> {
        for line in self.stdout.lines() {
            let Some(value) = line.strip_prefix("license cache: ") else {
                continue;
            };
            let Some((hits, misses)) = value.split_once(" hit, ") else {
                continue;
            };
            let Some(misses) = misses.strip_suffix(" miss") else {
                continue;
            };

            return Ok(LicenseCacheStats {
                hits: hits.parse().context("failed to parse license cache hits")?,
                misses: misses
                    .parse()
                    .context("failed to parse license cache misses")?,
            });
        }

        anyhow::bail!(
            "license cache statistics were not found in output:\n{}",
            self.stdout,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataSource {
    Cargo,
    Cache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LicenseCacheStats {
    pub(crate) hits: usize,
    pub(crate) misses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactHash(String);

impl ArtifactHash {
    fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read(path)
            .with_context(|| format!("failed to read generated output: {}", path.display(),))?;

        Ok(Self(sha256::digest(contents)))
    }
}

impl std::fmt::Display for ArtifactHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

struct DirectoryCopy<'a> {
    source: &'a Path,
    destination: &'a Path,
}

impl<'a> DirectoryCopy<'a> {
    fn new(source: &'a Path, destination: &'a Path) -> Self {
        Self {
            source,
            destination,
        }
    }

    fn copy(self) -> Result<()> {
        self.copy_directory(self.source, self.destination)
    }

    fn copy_directory(&self, source: &Path, destination: &Path) -> Result<()> {
        std::fs::create_dir_all(destination).with_context(|| {
            format!(
                "failed to create fixture directory: {}",
                destination.display(),
            )
        })?;

        for entry in std::fs::read_dir(source)
            .with_context(|| format!("failed to read fixture directory: {}", source.display(),))?
        {
            let entry = entry?;
            let source_path = entry.path();
            let destination_path = destination.join(entry.file_name());

            if entry.file_type()?.is_dir() {
                self.copy_directory(&source_path, &destination_path)?;
            } else {
                std::fs::copy(&source_path, &destination_path).with_context(|| {
                    format!("failed to copy fixture file: {}", source_path.display(),)
                })?;
            }
        }

        Ok(())
    }
}

struct CargoCommand;

impl CargoCommand {
    fn executable() -> OsString {
        std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
    }
}

struct CommandResult {
    stdout: String,
}

impl CommandResult {
    fn execute(command: &mut Command, description: &str) -> Result<Self> {
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

pub(crate) struct CacheAssertions;

impl CacheAssertions {
    pub(crate) fn cold(fixture: &FixtureWorkspace, execution: &CachedExecution) -> Result<()> {
        assert_eq!(
            execution.metadata_source()?,
            MetadataSource::Cargo,
            "cold run should use cargo metadata",
        );

        let stats = execution.license_cache_stats()?;
        assert_eq!(stats.hits, 0, "cold run should have no license cache hits",);
        assert!(
            stats.misses > 0,
            "cold run should have license cache misses",
        );

        let cache = fixture.cache_state();
        assert!(
            cache.is_populated(),
            "cold run should create cache: {cache:?}",
        );

        Ok(())
    }

    pub(crate) fn warm(fixture: &FixtureWorkspace, execution: &CachedExecution) -> Result<()> {
        let cache = fixture.cache_state();
        assert!(
            cache.is_populated(),
            "warm run should have cache available: {cache:?}",
        );
        assert_eq!(
            execution.metadata_source()?,
            MetadataSource::Cache,
            "warm run should use metadata cache",
        );

        let stats = execution.license_cache_stats()?;
        assert!(stats.hits > 0, "warm run should have license cache hits",);
        assert_eq!(
            stats.misses, 0,
            "warm run should have no license cache misses",
        );

        Ok(())
    }

    pub(crate) fn version_change(
        fixture: &FixtureWorkspace,
        execution: &CachedExecution,
    ) -> Result<()> {
        let cache = fixture.cache_state();
        assert!(
            cache.is_populated(),
            "version-change run should retain existing cache: {cache:?}",
        );
        assert_eq!(
            execution.metadata_source()?,
            MetadataSource::Cargo,
            "dependency change should invalidate metadata cache",
        );

        let stats = execution.license_cache_stats()?;
        assert!(
            stats.hits > 0,
            "version-change run should reuse unchanged license cache entries",
        );
        assert!(
            stats.misses > 0,
            "version-change run should miss changed/new package versions",
        );

        Ok(())
    }
}
