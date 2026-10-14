use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result, anyhow};
use cargo_about::{
    Krate,
    licenses::{KrateLicense, LicenseFile, LicenseFileKind, LicenseInfo, LicenseSource},
};
use cargo_lock::Lockfile;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::workspace::ConfigFingerprint;

const CACHE_SCHEMA_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub struct LicenseCache {
    entries: HashMap<String, LicenseCacheEntry>,
}

impl LicenseCache {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::new());
        }

        let contents = std::fs::read(path)
            .with_context(|| format!("failed to read license cache: {}", path.display()))?;

        let store = match serde_json::from_slice::<LicenseCacheStore>(&contents) {
            Ok(store) => store,

            Err(_) => {
                Self::remove_invalid(path)?;
                return Ok(Self::new());
            }
        };
        if store.schema_version != CACHE_SCHEMA_VERSION {
            Self::remove_invalid(path)?;
            return Ok(Self::new());
        }

        Ok(Self {
            entries: store.entries,
        })
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create license cache directory: {}",
                    parent.display()
                )
            })?;
        }

        let store = LicenseCacheStore {
            schema_version: CACHE_SCHEMA_VERSION.to_string(),
            entries: self.entries.clone(),
        };

        let contents = serde_json::to_vec(&store).context("failed to serialize license cache")?;

        std::fs::write(path, contents)
            .with_context(|| format!("failed to write license cache: {}", path.display()))?;

        Ok(())
    }

    pub fn get(
        &self,
        package: &PackageIdentity,
        config: &ConfigFingerprint,
    ) -> Result<Option<&CachedLicense>> {
        let key = LicenseCacheKey::new(package.clone(), config.clone());
        let id = key.hash()?;

        let Some(entry) = self.entries.get(&id) else {
            return Ok(None);
        };

        if entry.key != key {
            return Ok(None);
        }

        Ok(Some(&entry.license))
    }

    pub fn insert(
        &mut self,
        package: PackageIdentity,
        config: ConfigFingerprint,
        license: CachedLicense,
    ) -> Result<()> {
        let key = LicenseCacheKey::new(package, config);
        let id = key.hash()?;

        self.entries.insert(id, LicenseCacheEntry { key, license });

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn remove_invalid(path: &Path) -> Result<()> {
        if path.exists() {
            std::fs::remove_file(path).with_context(|| {
                format!("failed to remove invalid license cache: {}", path.display())
            })?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LicenseCacheStore {
    schema_version: String,
    entries: HashMap<String, LicenseCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LicenseCacheEntry {
    key: LicenseCacheKey,
    license: CachedLicense,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LicenseCacheKey {
    package: PackageIdentity,
    config: ConfigFingerprint,
}

impl LicenseCacheKey {
    fn new(package: PackageIdentity, config: ConfigFingerprint) -> Self {
        Self { package, config }
    }

    fn hash(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self).context("failed to serialize license cache key")?;

        Ok(sha256::digest(bytes))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageIdentity {
    pub name: String,
    pub version: Version,
    pub source: String,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LockPackageKey {
    name: String,
    version: Version,
    source: String,
}

#[derive(Debug)]
pub struct LockPackageIndex {
    entries: HashMap<LockPackageKey, Option<String>>,
}

impl LockPackageIndex {
    pub fn new(lockfile: &Lockfile) -> Result<Self> {
        let mut entries = HashMap::with_capacity(lockfile.packages.len());

        for package in &lockfile.packages {
            let Some(source) = &package.source else {
                continue;
            };

            let version = Version::parse(&package.version.to_string()).with_context(|| {
                format!(
                    "failed to parse package version: {} {}",
                    package.name, package.version
                )
            })?;

            let key = LockPackageKey {
                name: package.name.to_string(),
                version,
                source: source.to_string(),
            };

            let checksum = package.checksum.as_ref().map(ToString::to_string);
            entries.insert(key, checksum);
        }

        Ok(Self { entries })
    }

    pub fn identify(&self, krate: &Krate) -> Result<Option<PackageIdentity>> {
        let Some(source) = &krate.source else {
            return Ok(None);
        };

        let source = source.to_string();
        let key = LockPackageKey {
            name: krate.name.to_string(),
            version: krate.version.clone(),
            source: source.clone(),
        };

        let checksum = self.entries.get(&key).with_context(|| {
            format!(
                "package was found in cargo metadata but not in Cargo.lock: \
                 {} {} ({source})",
                krate.name, krate.version
            )
        })?;

        Ok(Some(PackageIdentity {
            name: krate.name.to_string(),
            version: krate.version.clone(),
            source,
            checksum: checksum.clone(),
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedLicense {
    info: CachedLicenseInfo,
    files: Vec<CachedLicenseFile>,
}

impl CachedLicense {
    pub fn capture(value: &KrateLicense<'_>) -> Self {
        Self {
            info: match &value.lic_info {
                LicenseInfo::Expr(expr) => CachedLicenseInfo::Expr(expr.to_string()),
                LicenseInfo::Unknown => CachedLicenseInfo::Unknown,
                LicenseInfo::Ignore => CachedLicenseInfo::Ignore,
            },

            files: value
                .license_files
                .iter()
                .map(CachedLicenseFile::capture)
                .collect(),
        }
    }

    pub fn restore<'a>(&self, krate: &'a Krate) -> Result<KrateLicense<'a>> {
        let lic_info = match &self.info {
            CachedLicenseInfo::Expr(expr) => {
                LicenseInfo::Expr(cargo_about::parse_license_expression(expr).with_context(
                    || format!("failed to parse cached license expression '{expr}'"),
                )?)
            }

            CachedLicenseInfo::Unknown => LicenseInfo::Unknown,
            CachedLicenseInfo::Ignore => LicenseInfo::Ignore,
        };

        let license_files = self
            .files
            .iter()
            .map(CachedLicenseFile::restore)
            .collect::<Result<Vec<_>>>()?;

        Ok(KrateLicense {
            krate,
            lic_info,
            license_files,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CachedLicenseInfo {
    Expr(String),
    Unknown,
    Ignore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedLicenseFile {
    license: CachedLicenseSource,
    path: String,
    confidence: f32,
    kind: CachedLicenseFileKind,
}

impl CachedLicenseFile {
    fn capture(file: &LicenseFile) -> Self {
        Self {
            license: match &file.license {
                LicenseSource::Detected(id) => CachedLicenseSource::Detected(id.name.to_owned()),

                LicenseSource::Clarified(expr) => CachedLicenseSource::Clarified(expr.to_string()),
            },

            path: file.path.as_str().to_owned(),
            confidence: file.confidence,

            kind: match &file.kind {
                LicenseFileKind::Text(text) => CachedLicenseFileKind::Text(text.clone()),

                LicenseFileKind::AddendumText(text, path) => CachedLicenseFileKind::AddendumText {
                    text: text.clone(),
                    path: path.as_str().to_owned(),
                },

                LicenseFileKind::Header => CachedLicenseFileKind::Header,
            },
        }
    }

    fn restore(&self) -> Result<LicenseFile> {
        let license = match &self.license {
            CachedLicenseSource::Detected(id) => {
                let id = spdx::license_id(id)
                    .ok_or_else(|| anyhow!("unknown SPDX license id in cache: '{id}'"))?;

                LicenseSource::Detected(id)
            }

            CachedLicenseSource::Clarified(expr) => LicenseSource::Clarified(
                cargo_about::parse_license_expression(expr).with_context(|| {
                    format!(
                        "failed to parse cached clarified \
                                 license expression '{expr}'"
                    )
                })?,
            ),
        };

        let kind = match &self.kind {
            CachedLicenseFileKind::Text(text) => LicenseFileKind::Text(text.clone()),

            CachedLicenseFileKind::AddendumText { text, path } => LicenseFileKind::AddendumText(
                text.clone(),
                krates::Utf8PathBuf::from(path.as_str()),
            ),

            CachedLicenseFileKind::Header => LicenseFileKind::Header,
        };

        Ok(LicenseFile {
            license,
            path: krates::Utf8PathBuf::from(self.path.as_str()),
            confidence: self.confidence,
            kind,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CachedLicenseSource {
    Detected(String),
    Clarified(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CachedLicenseFileKind {
    Text(String),
    AddendumText { text: String, path: String },
    Header,
}
