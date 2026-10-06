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

use crate::cached::{CacheStore, CachedLicense, CachedPackage, LockPackageIndex};

pub struct GatherResult<'a> {
    pub licenses: Vec<KrateLicense<'a>>,
    pub hits: usize,
    pub misses: usize,
}

struct MissingPackage<'a> {
    krate: &'a Krate,
    package: Option<CachedPackage>,
}

pub fn gather_cached<'a>(
    krates: &'a Krates,
    metadata: &Metadata,
    package_index: &LockPackageIndex,
    cache: &mut CacheStore,
    config: &Config,
) -> Result<GatherResult<'a>> {
    let mut licenses = Vec::with_capacity(krates.len());
    let mut missing = Vec::new();

    let mut hits = 0;
    let mut misses = 0;

    for krate in krates.krates() {
        match package_index.package(krate)? {
            Some(package) => {
                if let Some(entry) = cache.get(&package) {
                    licenses.push(entry.license.to_krate_license(krate).with_context(|| {
                        format!(
                            "failed to restore cached license for {} {}",
                            krate.name, krate.version,
                        )
                    })?);

                    hits += 1;
                } else {
                    // registry/git dependency:
                    // キャッシュ可能だが、まだキャッシュにない
                    missing.push(MissingPackage {
                        krate,
                        package: Some(package),
                    });

                    misses += 1;
                }
            }

            None => {
                // workspace/path dependency:
                // キャッシュはしないが cargo-about の gather 対象には含める
                missing.push(MissingPackage {
                    krate,
                    package: None,
                });
            }
        }
    }

    if !missing.is_empty() {
        gather_missing(metadata, &missing, cache, config, &mut licenses)?;
    }

    licenses.sort();

    Ok(GatherResult {
        licenses,
        hits,
        misses,
    })
}

fn gather_missing<'a>(
    metadata: &Metadata,
    missing: &[MissingPackage<'a>],
    cache: &mut CacheStore,
    config: &Config,
    licenses: &mut Vec<KrateLicense<'a>>,
) -> Result<()> {
    let ids = missing
        .iter()
        .map(|missing| missing.krate.id.clone())
        .collect::<HashSet<_>>();

    let missing_krates = build_missing_krates(metadata, &ids)?;

    let store = Arc::new(store_from_cache().context("failed to load cargo-about license store")?);

    let gatherer =
        Gatherer::with_store(store).with_max_depth(config.max_depth.map(|depth| depth as usize));

    let gathered = gatherer.gather(&missing_krates, config, Some(http_client()));

    let mut gathered = gathered
        .into_iter()
        .map(|license| (license.krate.id.clone(), CachedLicense::from(&license)))
        .collect::<HashMap<_, _>>();

    for missing in missing {
        let cached_license = gathered.remove(&missing.krate.id).with_context(|| {
            format!(
                "cargo-about did not produce a license for {} {}",
                missing.krate.name, missing.krate.version,
            )
        })?;

        if let Some(package) = &missing.package {
            cache.insert(package.clone(), cached_license.clone());
        }

        licenses.push(
            cached_license
                .to_krate_license(missing.krate)
                .with_context(|| {
                    format!(
                        "failed to restore gathered license for {} {}",
                        missing.krate.name, missing.krate.version,
                    )
                })?,
        );
    }

    Ok(())
}

fn build_missing_krates(metadata: &Metadata, missing_ids: &HashSet<PackageId>) -> Result<Krates> {
    let mut metadata = metadata.clone();

    metadata
        .packages
        .retain(|package| missing_ids.contains(&package.id));

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
    resolve.nodes.retain(|node| missing_ids.contains(&node.id));

    for node in &mut resolve.nodes {
        node.deps
            .retain(|dependency| missing_ids.contains(&dependency.pkg));

        node.dependencies
            .retain(|dependency| missing_ids.contains(dependency));
    }

    let krates: Krates = Builder::new()
        .build_with_metadata(metadata, NoneFilter)
        .context("failed to build cache-miss crate graph")?;

    Ok(krates)
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
