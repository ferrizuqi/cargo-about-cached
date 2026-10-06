use std::collections::HashMap;

use anyhow::{Context, Result};
use cargo_about::Krate;
use cargo_lock::Lockfile;

use super::*;

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

    pub fn package(&self, krate: &Krate) -> Result<Option<CachedPackage>> {
        let Some(source) = &krate.source else {
            // workspace/path dependency
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
                "package was found in cargo metadata but not in Cargo.lock: {} {} ({})",
                krate.name, krate.version, source
            )
        })?;

        Ok(Some(CachedPackage {
            name: krate.name.to_string(),
            version: krate.version.clone(),
            source,
            checksum: checksum.clone(),
        }))
    }
}
