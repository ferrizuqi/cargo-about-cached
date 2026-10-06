use super::*;

use std::{fs, path::Path};

use anyhow::{Context, Result};

pub const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStore {
    pub schema_version: u32,

    #[serde(default)]
    pub config_fingerprint: CachedConfigFingerprint,

    pub entries: HashMap<CachedPackageHash, CacheEntry>,
}

impl CacheStore {
    pub fn new(config_fingerprint: CachedConfigFingerprint) -> Self {
        Self {
            schema_version: CACHE_SCHEMA_VERSION,
            config_fingerprint,
            entries: HashMap::new(),
        }
    }

    pub fn load(
        path: impl AsRef<Path>,
        expected_fingerprint: &CachedConfigFingerprint,
    ) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(Self::new(expected_fingerprint.clone()));
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read cache: {}", path.display()))?;

        let store: Self = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse cache: {}", path.display()))?;

        if store.schema_version != CACHE_SCHEMA_VERSION {
            return Ok(Self::new(expected_fingerprint.clone()));
        }

        if &store.config_fingerprint != expected_fingerprint {
            return Ok(Self::new(expected_fingerprint.clone()));
        }

        Ok(store)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create cache directory: {}", parent.display())
            })?;
        }

        let content = serde_json::to_vec_pretty(self).context("failed to serialize cache")?;

        fs::write(path, content)
            .with_context(|| format!("failed to write cache: {}", path.display()))?;

        Ok(())
    }

    pub fn get(&self, package: &CachedPackage) -> Option<&CacheEntry> {
        let hash = package.hash();
        let entry = self.entries.get(&hash)?;

        if &entry.package == package {
            Some(entry)
        } else {
            None
        }
    }

    pub fn insert(&mut self, package: CachedPackage, license: CachedLicense) -> CachedPackageHash {
        let hash = package.hash();

        self.entries
            .insert(hash.clone(), CacheEntry { package, license });

        hash
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub package: CachedPackage,
    pub license: CachedLicense,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedPackage {
    pub name: String,
    pub version: Version,
    pub source: String,
    pub checksum: Option<String>,
}

impl CachedPackage {
    pub fn hash(&self) -> CachedPackageHash {
        let input = format!(
            "{}\0{}\0{}\0{}",
            self.name,
            self.version,
            self.source,
            self.checksum.as_deref().unwrap_or("<none>"),
        );

        CachedPackageHash(sha256::digest(input))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedLicense {
    pub info: CachedLicenseInfo,
    pub files: Vec<CachedLicenseFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CachedLicenseInfo {
    Expr(String),
    Unknown,
    Ignore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedLicenseFile {
    pub license: CachedLicenseSource,
    pub path: String,
    pub confidence: f32,
    pub kind: CachedLicenseFileKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CachedLicenseSource {
    Detected(String),
    Clarified(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CachedLicenseFileKind {
    Text(String),
    AddendumText { text: String, path: String },
    Header,
}
