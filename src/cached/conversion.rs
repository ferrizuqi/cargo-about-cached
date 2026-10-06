use anyhow::{Context, anyhow};
use cargo_about::{
    Krate,
    licenses::{KrateLicense, LicenseFile, LicenseFileKind, LicenseInfo, LicenseSource},
};

use super::*;

impl From<&KrateLicense<'_>> for CachedLicense {
    fn from(value: &KrateLicense<'_>) -> Self {
        Self {
            info: match &value.lic_info {
                LicenseInfo::Expr(expr) => CachedLicenseInfo::Expr(expr.to_string()),
                LicenseInfo::Unknown => CachedLicenseInfo::Unknown,
                LicenseInfo::Ignore => CachedLicenseInfo::Ignore,
            },

            files: value
                .license_files
                .iter()
                .map(|file| CachedLicenseFile {
                    license: match &file.license {
                        LicenseSource::Detected(id) => {
                            CachedLicenseSource::Detected(id.name.to_owned())
                        }
                        LicenseSource::Clarified(expr) => {
                            CachedLicenseSource::Clarified(expr.to_string())
                        }
                    },

                    // そのまま保存する
                    path: file.path.as_str().to_owned(),

                    confidence: file.confidence,

                    kind: match &file.kind {
                        LicenseFileKind::Text(text) => CachedLicenseFileKind::Text(text.clone()),

                        LicenseFileKind::AddendumText(text, path) => {
                            CachedLicenseFileKind::AddendumText {
                                text: text.clone(),
                                path: path.as_str().to_owned(),
                            }
                        }

                        LicenseFileKind::Header => CachedLicenseFileKind::Header,
                    },
                })
                .collect(),
        }
    }
}

impl CachedLicense {
    pub fn to_krate_license<'a>(&self, krate: &'a Krate) -> anyhow::Result<KrateLicense<'a>> {
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
            .map(|file| {
                let license = match &file.license {
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

                let kind = match &file.kind {
                    CachedLicenseFileKind::Text(text) => LicenseFileKind::Text(text.clone()),
                    CachedLicenseFileKind::AddendumText { text, path } => {
                        LicenseFileKind::AddendumText(
                            text.clone(),
                            krates::Utf8PathBuf::from(path.as_str()),
                        )
                    }
                    CachedLicenseFileKind::Header => LicenseFileKind::Header,
                };

                Ok(LicenseFile {
                    license,
                    path: krates::Utf8PathBuf::from(file.path.as_str()),
                    confidence: file.confidence,
                    kind,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(KrateLicense {
            krate,
            lic_info,
            license_files,
        })
    }
}
