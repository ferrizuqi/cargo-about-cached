use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::{Context, Result};
use cargo_about::{
    Krate, Krates,
    licenses::{Gatherer, KrateLicense, config::Config, store_from_cache},
};
use krates::{
    Builder, NoneFilter,
    cm::{Metadata, PackageId, WorkspaceDefaultMembers},
};

use crate::{
    cache::{CachedLicense, LicenseCache, LockPackageIndex, PackageIdentity},
    workspace::ConfigFingerprint,
};

pub struct LicenseResolver<'k, 'ctx> {
    krates: &'k Krates,
    metadata: &'k Metadata,

    package_index: &'ctx LockPackageIndex,
    cache: &'ctx mut LicenseCache,
    config: &'ctx Config,
    config_fingerprint: &'ctx ConfigFingerprint,
}

impl<'k, 'ctx> LicenseResolver<'k, 'ctx> {
    pub fn new(
        krates: &'k Krates,
        metadata: &'k Metadata,
        package_index: &'ctx LockPackageIndex,
        cache: &'ctx mut LicenseCache,
        config: &'ctx Config,
        config_fingerprint: &'ctx ConfigFingerprint,
    ) -> Self {
        Self {
            krates,
            metadata,
            package_index,
            cache,
            config,
            config_fingerprint,
        }
    }

    pub fn resolve(mut self) -> Result<LicenseResolution<'k>> {
        let mut licenses = Vec::with_capacity(self.krates.len());
        let mut missing = Vec::new();

        let mut hits = 0;
        let mut misses = 0;

        for krate in self.krates.krates() {
            match self.package_index.identify(krate)? {
                Some(package) => {
                    if let Some(cached) = self.cache.get(&package, self.config_fingerprint)? {
                        licenses.push(cached.restore(krate).with_context(|| {
                            format!(
                                "failed to restore cached license for {} {}",
                                krate.name, krate.version,
                            )
                        })?);

                        hits += 1;
                    } else {
                        missing.push(MissingPackage {
                            krate,
                            package: Some(package),
                        });

                        misses += 1;
                    }
                }

                None => {
                    missing.push(MissingPackage {
                        krate,
                        package: None,
                    });
                }
            }
        }

        if !missing.is_empty() {
            self.gather_missing(&missing, &mut licenses)?;
        }

        licenses.sort();

        Ok(LicenseResolution {
            licenses,
            hits,
            misses,
        })
    }

    fn gather_missing(
        &mut self,
        missing: &[MissingPackage<'k>],
        licenses: &mut Vec<KrateLicense<'k>>,
    ) -> Result<()> {
        let ids = missing
            .iter()
            .map(|missing| missing.krate.id.clone())
            .collect::<HashSet<_>>();

        let missing_krates = Self::build_subset(self.metadata, &ids)?;

        let store =
            Arc::new(store_from_cache().context("failed to load cargo-about license store")?);

        let gatherer = Gatherer::with_store(store)
            .with_max_depth(self.config.max_depth.map(|depth| depth as usize));

        let gathered = gatherer.gather(&missing_krates, self.config, Some(Self::http_client()));
        let mut gathered = gathered
            .into_iter()
            .map(|license| (license.krate.id.clone(), CachedLicense::capture(&license)))
            .collect::<HashMap<_, _>>();

        for missing in missing {
            let cached_license = gathered.remove(&missing.krate.id).with_context(|| {
                format!(
                    "cargo-about did not produce a license for {} {}",
                    missing.krate.name, missing.krate.version,
                )
            })?;

            if let Some(package) = &missing.package {
                self.cache.insert(
                    package.clone(),
                    self.config_fingerprint.clone(),
                    cached_license.clone(),
                )?;
            }

            licenses.push(cached_license.restore(missing.krate).with_context(|| {
                format!(
                    "failed to restore gathered license for {} {}",
                    missing.krate.name, missing.krate.version,
                )
            })?);
        }

        Ok(())
    }

    fn build_subset(metadata: &Metadata, package_ids: &HashSet<PackageId>) -> Result<Krates> {
        let mut metadata = metadata.clone();

        metadata
            .packages
            .retain(|package| package_ids.contains(&package.id));

        metadata.workspace_members = metadata
            .packages
            .iter()
            .map(|package| package.id.clone())
            .collect();

        metadata.workspace_default_members =
            WorkspaceDefaultMembers(Some(metadata.workspace_members.clone()));

        let resolve = metadata
            .resolve
            .as_mut()
            .context("cargo metadata did not contain a resolve graph")?;
        resolve.root = None;
        resolve.nodes.retain(|node| package_ids.contains(&node.id));

        for node in &mut resolve.nodes {
            node.deps
                .retain(|dependency| package_ids.contains(&dependency.pkg));
            node.dependencies
                .retain(|dependency| package_ids.contains(dependency));
        }

        Builder::new()
            .build_with_metadata(metadata, NoneFilter)
            .context("failed to build cache-miss crate graph")
    }

    fn http_client() -> ureq::Agent {
        use ureq::tls::{RootCerts, TlsConfig};
        let provider = rustls::crypto::ring::default_provider();

        ureq::Agent::config_builder()
            .tls_config(
                TlsConfig::builder()
                    .unversioned_rustls_crypto_provider(std::sync::Arc::new(provider))
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .new_agent()
    }
}

pub struct LicenseResolution<'k> {
    pub licenses: Vec<KrateLicense<'k>>,
    pub hits: usize,
    pub misses: usize,
}

struct MissingPackage<'k> {
    krate: &'k Krate,
    package: Option<PackageIdentity>,
}
