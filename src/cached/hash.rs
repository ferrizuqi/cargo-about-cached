use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CachedPackageHash(pub(super) String);

impl CachedPackageHash {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl From<String> for CachedPackageHash {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl From<CachedPackageHash> for String {
    fn from(value: CachedPackageHash) -> String {
        value.0
    }
}
impl std::fmt::Display for CachedPackageHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedConfigFingerprint(String);

impl CachedConfigFingerprint {
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

impl std::fmt::Display for CachedConfigFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
