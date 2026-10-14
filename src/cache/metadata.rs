use anyhow::{Context, Result};
use cargo_lock::Lockfile;
use krates::{Utf8Path, Utf8PathBuf, cm::Metadata};
use serde::{Deserialize, Serialize};

use crate::workspace::{Workspace, WorkspaceProbe};

const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub struct MetadataCache {
    directory: Utf8PathBuf,
}

impl MetadataCache {
    pub fn new(directory: Utf8PathBuf) -> Self {
        Self { directory }
    }

    pub fn directory(&self) -> &Utf8Path {
        &self.directory
    }

    pub fn resolve(
        &self,
        workspace: &Workspace,
        probe: &WorkspaceProbe,
    ) -> Result<MetadataResolution> {
        let fingerprint = MetadataFingerprint::new(probe)?;
        let path = self.entry_path(&fingerprint);

        if let Some(metadata) = self.load(&path, &fingerprint)? {
            return Ok(MetadataResolution {
                metadata,
                source: MetadataSource::Cache,
            });
        }

        let metadata = workspace.load_metadata()?;

        self.save(&path, &fingerprint, &metadata)?;

        Ok(MetadataResolution {
            metadata,
            source: MetadataSource::Cargo,
        })
    }

    fn entry_path(&self, fingerprint: &MetadataFingerprint) -> Utf8PathBuf {
        self.directory
            .join(format!("{}.json", fingerprint.as_str()))
    }

    fn load(
        &self,
        path: &Utf8Path,
        expected_fingerprint: &MetadataFingerprint,
    ) -> Result<Option<Metadata>> {
        if !path.exists() {
            return Ok(None);
        }

        let bytes = std::fs::read(path.as_std_path())
            .with_context(|| format!("failed to read metadata cache: {path}"))?;

        let entry = match serde_json::from_slice::<MetadataCacheEntry>(&bytes) {
            Ok(entry) => entry,

            Err(_) => {
                self.remove(path)?;
                return Ok(None);
            }
        };

        if entry.schema_version != CACHE_SCHEMA_VERSION {
            self.remove(path)?;
            return Ok(None);
        }

        if entry.fingerprint != expected_fingerprint.as_str() {
            self.remove(path)?;
            return Ok(None);
        }

        Ok(Some(entry.metadata))
    }

    fn save(
        &self,
        path: &Utf8Path,
        fingerprint: &MetadataFingerprint,
        metadata: &Metadata,
    ) -> Result<()> {
        std::fs::create_dir_all(self.directory.as_std_path()).with_context(|| {
            format!(
                "failed to create metadata cache directory: {}",
                self.directory
            )
        })?;

        let entry = MetadataCacheEntry {
            schema_version: CACHE_SCHEMA_VERSION,
            fingerprint: fingerprint.as_str().to_owned(),
            metadata: metadata.clone(),
        };

        let bytes = serde_json::to_vec(&entry).context("failed to serialize metadata cache")?;

        std::fs::write(path.as_std_path(), bytes)
            .with_context(|| format!("failed to write metadata cache: {path}"))?;

        Ok(())
    }

    fn remove(&self, path: &Utf8Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        std::fs::remove_file(path.as_std_path())
            .with_context(|| format!("failed to remove invalid metadata cache: {path}"))?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct MetadataResolution {
    metadata: Metadata,
    source: MetadataSource,
}

impl MetadataResolution {
    pub fn source(&self) -> MetadataSource {
        self.source
    }

    pub fn into_metadata(self) -> Metadata {
        self.metadata
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataSource {
    Cache,
    Cargo,
}

impl std::fmt::Display for MetadataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cache => f.write_str("cache"),
            Self::Cargo => f.write_str("cargo"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MetadataFingerprint(String);

impl MetadataFingerprint {
    fn new(probe: &WorkspaceProbe) -> Result<Self> {
        let input = MetadataFingerprintInput {
            schema_version: CACHE_SCHEMA_VERSION,
            cargo_version: probe.cargo_version(),
            workspace: probe.metadata(),
            lockfile: probe.lockfile(),
        };

        let bytes =
            serde_json::to_vec(&input).context("failed to serialize metadata fingerprint")?;
        Ok(Self(sha256::digest(bytes)))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Serialize)]
struct MetadataFingerprintInput<'a> {
    schema_version: u32,
    cargo_version: &'a str,
    workspace: &'a serde_json::Value,
    lockfile: &'a Lockfile,
}

#[derive(Debug, Serialize, Deserialize)]
struct MetadataCacheEntry {
    schema_version: u32,
    fingerprint: String,
    metadata: Metadata,
}
