use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result};

use super::metadata::{MetadataDocument, MetadataPackage};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PackageKey {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) source: Option<String>,
}

impl PackageKey {
    pub(crate) fn from_metadata(package: &MetadataPackage) -> Self {
        Self {
            name: package.name.clone(),
            version: package.version.clone(),
            source: package.source.clone(),
        }
    }

    pub(crate) fn from_lock(package: &cargo_lock::Package) -> Self {
        Self {
            name: package.name.to_string(),
            version: package.version.to_string(),
            source: package.source.as_ref().map(ToString::to_string),
        }
    }
}

#[derive(Debug)]
pub(crate) struct DependencyGraph {
    nodes: BTreeMap<PackageKey, BTreeSet<PackageKey>>,
}

impl DependencyGraph {
    pub(crate) fn from_metadata(metadata: &MetadataDocument, root_id: &str) -> Result<Self> {
        let packages = metadata
            .packages
            .iter()
            .map(|package| (package.id.as_str(), package))
            .collect::<HashMap<_, _>>();

        let resolve = metadata
            .resolve
            .as_ref()
            .context("cargo metadata did not contain a resolve graph")?;

        let resolve_nodes = resolve
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect::<HashMap<_, _>>();

        let mut pending = vec![root_id.to_owned()];
        let mut visited = BTreeSet::<String>::new();
        let mut nodes = BTreeMap::<PackageKey, BTreeSet<PackageKey>>::new();

        while let Some(id) = pending.pop() {
            if !visited.insert(id.clone()) {
                continue;
            }

            let package = packages
                .get(id.as_str())
                .with_context(|| format!("metadata package was not found for {id}"))?;

            let node = resolve_nodes
                .get(id.as_str())
                .with_context(|| format!("resolve node was not found for {id}"))?;

            let key = PackageKey::from_metadata(package);

            let mut dependencies = BTreeSet::new();

            for dependency in &node.deps {
                let dependency_package =
                    packages.get(dependency.pkg.as_str()).with_context(|| {
                        format!(
                            "metadata dependency package was not found for {}",
                            dependency.pkg,
                        )
                    })?;

                let dependency_key = PackageKey::from_metadata(dependency_package);
                dependencies.insert(dependency_key);
                pending.push(dependency.pkg.clone());
            }

            nodes.insert(key, dependencies);
        }

        Ok(Self { nodes })
    }

    pub(crate) fn packages(&self) -> impl Iterator<Item = &PackageKey> {
        self.nodes.keys()
    }

    pub(crate) fn contains(&self, package: &PackageKey) -> bool {
        self.nodes.contains_key(package)
    }

    pub(crate) fn ensure_contained_in(&self, reference: &Self) -> Result<()> {
        for (package, dependencies) in &self.nodes {
            let reference_dependencies = reference.nodes.get(package).with_context(|| {
                format!(
                    "reference cargo-about does not contain \
                             the same package: {} {}",
                    package.name, package.version,
                )
            })?;

            for dependency in dependencies {
                anyhow::ensure!(
                    reference_dependencies.contains(dependency),
                    "reference dependency graph differs:\n\
                     {} {} should depend on {} {}, \
                     but reference graph does not",
                    package.name,
                    package.version,
                    dependency.name,
                    dependency.version,
                );
            }
        }

        Ok(())
    }
}
