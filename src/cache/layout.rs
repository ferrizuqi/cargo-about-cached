use krates::{Utf8Path, Utf8PathBuf};

#[derive(Debug, Clone)]
pub struct CacheLayout {
    root: Utf8PathBuf,
}

impl CacheLayout {
    pub fn new(target_directory: &Utf8Path) -> Self {
        Self {
            root: target_directory.join(".cargo-about-cached"),
        }
    }

    pub fn metadata_store(&self) -> Utf8PathBuf {
        self.root.join("metadata")
    }
    pub fn license_store(&self) -> Utf8PathBuf {
        self.root.join("licenses.json")
    }
}
