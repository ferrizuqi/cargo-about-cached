use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use cargo_lock::Lockfile;
use tempfile::TempDir;

use super::{
    graph::{DependencyGraph, PackageKey},
    metadata::MetadataDocument,
};

pub(crate) struct ProjectCargoAbout {
    pub(crate) version: String,
    pub(crate) source_directory: PathBuf,
    pub(crate) graph: DependencyGraph,
}

impl ProjectCargoAbout {
    pub(crate) fn load(project_root: &Path) -> Result<Self> {
        let metadata = MetadataDocument::load(&project_root.join("Cargo.toml"), true)?;

        let cargo_about = metadata.find_single_package("cargo-about")?;

        let source_directory = cargo_about
            .manifest_path
            .parent()
            .context("cargo-about manifest path has no parent")?
            .to_owned();

        let graph = DependencyGraph::from_metadata(&metadata, &cargo_about.id)?;

        Ok(Self {
            version: cargo_about.version.clone(),
            source_directory,
            graph,
        })
    }
}

pub(crate) struct ReferenceResolver {
    _temp: TempDir,
    root: PathBuf,
}

impl ReferenceResolver {
    pub(crate) fn create(project_root: &Path, project: &ProjectCargoAbout) -> Result<Self> {
        let temp_parent = project_root
            .join("target")
            .join("test-tools")
            .join("resolver");

        std::fs::create_dir_all(&temp_parent).with_context(|| {
            format!(
                "failed to create resolver temp directory: {}",
                temp_parent.display(),
            )
        })?;

        let temp = tempfile::tempdir_in(&temp_parent)
            .context("failed to create cargo-about reference resolver directory")?;

        let root = temp.path().join("workspace");

        std::fs::create_dir_all(root.join("src"))
            .context("failed to create reference resolver source directory")?;

        std::fs::write(
            root.join("src").join("lib.rs"),
            "// cargo-about reference resolver\n",
        )
        .context("failed to create reference resolver lib.rs")?;

        let manifest = ResolverManifest::new(project);

        std::fs::write(root.join("Cargo.toml"), manifest.render()?)
            .context("failed to write cargo-about reference resolver Cargo.toml")?;

        Ok(Self { _temp: temp, root })
    }

    pub(crate) fn resolve(&self, project: &ProjectCargoAbout) -> Result<ReferenceResolution> {
        let metadata = MetadataDocument::load(&self.root.join("Cargo.toml"), false)?;

        let cargo_about = metadata
            .packages
            .iter()
            .filter(|package| package.name == "cargo-about" && package.version == project.version)
            .collect::<Vec<_>>();

        anyhow::ensure!(
            cargo_about.len() == 1,
            "expected exactly one cargo-about {} package \
             in reference resolver, found {}",
            project.version,
            cargo_about.len(),
        );

        let cargo_about = cargo_about[0];
        let reference_graph = DependencyGraph::from_metadata(&metadata, &cargo_about.id)?;
        project.graph.ensure_contained_in(&reference_graph)?;

        Ok(ReferenceResolution {
            resolver_root: self.root.clone(),
            graph: reference_graph,
            root: PackageKey::from_metadata(cargo_about),
        })
    }
}

struct ResolverManifest<'a> {
    project: &'a ProjectCargoAbout,
}

impl<'a> ResolverManifest<'a> {
    fn new(project: &'a ProjectCargoAbout) -> Self {
        Self { project }
    }

    fn render(&self) -> Result<String> {
        let mut output = String::new();

        output.push_str(
            "[package]\n\
             name = \"cargo-about-ref-tool\"\n\
             version = \"0.0.0\"\n\
             edition = \"2024\"\n\
             publish = false\n\
             \n\
             [dependencies]\n",
        );

        output.push_str(&format!(
            "cargo-about-ref = {{ package = \"cargo-about\", \
             version = \"={}\", features = [\"cli\"] }}\n",
            self.project.version,
        ));

        let mut index = 0usize;

        for package in self.project.graph.packages() {
            if package.name == "cargo-about" && package.version == self.project.version {
                continue;
            }

            let Some(source) = package.source.as_deref() else {
                return Err(anyhow!(
                    "cargo-about dependency graph contains \
                     local/path dependency: {} {}",
                    package.name,
                    package.version,
                ));
            };

            anyhow::ensure!(
                source.starts_with("registry+"),
                "unsupported cargo-about dependency source \
                 for {} {}: {}",
                package.name,
                package.version,
                source,
            );

            output.push_str(&format!(
                "pin-{index:04} = {{ package = {:?}, \
                     version = {:?}, \
                     default-features = false }}\n",
                package.name,
                format!("={}", package.version,),
            ));

            index += 1;
        }

        output.push_str("\n[workspace]\n");

        Ok(output)
    }
}

pub(crate) struct ReferenceResolution {
    resolver_root: PathBuf,
    graph: DependencyGraph,
    root: PackageKey,
}

impl ReferenceResolution {
    pub(crate) fn create_lockfile(&self) -> Result<Lockfile> {
        let path = self.resolver_root.join("Cargo.lock");

        let mut lockfile = Lockfile::load(&path).with_context(|| {
            format!(
                "failed to load reference resolver Cargo.lock: {}",
                path.display(),
            )
        })?;

        lockfile
            .packages
            .retain(|package| self.graph.contains(&PackageKey::from_lock(package)));

        let root = lockfile
            .packages
            .iter_mut()
            .find(|package| PackageKey::from_lock(package) == self.root)
            .context("cargo-about package was not found in reference lockfile")?;

        root.source = None;
        root.checksum = None;

        lockfile.root = None;
        lockfile.metadata.clear();
        lockfile.patch = Default::default();

        Ok(lockfile)
    }
}
