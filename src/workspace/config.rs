use anyhow::{Context, Result, anyhow};
use cargo_about::licenses::config::Config;
use krates::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use toml_span::Deserialize as _;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ConfigFingerprint(String);

impl ConfigFingerprint {
    pub fn from_contents(contents: Option<&str>) -> Self {
        const FINGERPRINT_VERSION: &str = "cargo-about-cached-config-v1";

        let input = match contents {
            Some(contents) => {
                format!("{FINGERPRINT_VERSION}\0file\0{contents}")
            }
            None => {
                format!("{FINGERPRINT_VERSION}\0default")
            }
        };

        Self(sha256::digest(input))
    }
}

impl std::fmt::Display for ConfigFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub struct WorkspaceConfig {
    path: Option<Utf8PathBuf>,
    value: Config,
    fingerprint: ConfigFingerprint,
}

impl std::fmt::Debug for WorkspaceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceConfig")
            .field("path", &self.path)
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}

impl WorkspaceConfig {
    pub fn discover(manifest_path: &Utf8Path) -> Result<Self> {
        let mut parent = manifest_path.parent();

        while let Some(directory) = parent {
            let path = directory.join("about.toml");

            if path.exists() {
                return Self::load(path);
            }

            parent = directory.parent();
        }

        Ok(Self {
            path: None,
            value: Config::default(),
            fingerprint: ConfigFingerprint::from_contents(None),
        })
    }

    pub fn path(&self) -> Option<&Utf8Path> {
        self.path.as_deref()
    }
    pub fn value(&self) -> &Config {
        &self.value
    }
    pub fn fingerprint(&self) -> &ConfigFingerprint {
        &self.fingerprint
    }
    pub fn into_value(self) -> Config {
        self.value
    }

    fn load(path: Utf8PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read cargo-about config: {path}"))?;

        let fingerprint = ConfigFingerprint::from_contents(Some(&contents));
        let value = Self::deserialize(&path, &contents)?;

        Ok(Self {
            path: Some(path),
            value,
            fingerprint,
        })
    }

    fn deserialize(path: &Utf8Path, contents: &str) -> Result<Config> {
        let mut value = toml_span::parse(contents)
            .map_err(|error| anyhow!("failed to parse cargo-about config '{path}': {error}"))?;

        Config::deserialize(&mut value).map_err(|error| {
            let errors = error
                .errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n");

            anyhow!(
                "failed to deserialize cargo-about config \
                 '{path}':\n{errors}"
            )
        })
    }
}
