use serde::Deserialize;
#[cfg(feature = "bin")]
use url::Url;

#[derive(Debug, Deserialize)]
pub(crate) struct PrEventInfo {
    pub pull_request: PullRequestInfo,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PullRequestInfo {
    pub base: GitRefInfo,
    pub head: GitRefInfo,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitRefInfo {
    /// Commit SHA, e.g. `abcd1234`
    pub sha: String,
}

/// A Platform dependency fetched with the Board Manager.
#[derive(Debug, Clone, Default)]
pub struct ManagerEntry {
    /// The name of the dependency.
    ///
    /// For platforms, this is the form `vendor:board` (e.g. `arduino:avr`).
    ///
    /// For libraries, this is the library name used in the Arduino Library Manager (e.g. `Adafruit_BME280`).
    pub name: String,

    /// The version of the dependency. (e.g. `1.8.19`)
    pub version: Option<String>,

    /// The (optional) URL of the dependency source.
    pub source_url: Option<String>,
}

/// A Platform dependency specified as a local path.
#[derive(Debug, Clone, Default)]
pub struct PathEntry {
    /// The path to the dependency source.
    pub source_path: String,

    /// The name of the dependency.
    ///
    /// This is required when used to specify a platform dependency.
    /// For platforms, this is the form `vendor:board` (e.g. `arduino:avr`).
    ///
    /// For libraries, this is the destination path in which to install the library.
    pub name: Option<String>,
}

/// A Platform dependency specified as a git repository.
#[derive(Debug, Clone, Default)]
pub struct RepoEntry {
    /// The URL of the dependency source.
    pub source_url: String,
    /// The version of the dependency. (e.g. `1.8.19`)
    pub version: Option<String>,
    /// The (optional) local path to the dependency source.
    pub source_path: Option<String>,

    /// The (optional) name to use for the destination directory.
    ///
    /// This is required when used to specify a platform dependency.
    pub destination_name: Option<String>,
}

/// A Platform dependency specified as a direct download.
#[derive(Debug, Clone, Default)]
pub struct DownloadEntry {
    /// The URL of the dependency source.
    pub source_url: String,

    /// The (optional) local path to the dependency source.
    pub source_path: Option<String>,

    /// The (optional) name to use for the destination directory.
    ///
    /// This is required when used to specify a platform dependency.
    pub destination_name: Option<String>,
}

/// A Platform dependency specified in the input YAML
#[derive(Debug)]
#[cfg(feature = "bin")]
pub(crate) enum PlatformDependency {
    Manager(ManagerEntry),
    Path(PathEntry),
    Repo(RepoEntry),
    Download(DownloadEntry),
}

#[cfg(feature = "bin")]
use std::{collections::HashMap, ffi::OsStr, path::Path};
#[cfg(feature = "bin")]
impl TryFrom<HashMap<String, String>> for PlatformDependency {
    type Error = crate::error::CompileSketchesError;

    fn try_from(value: HashMap<String, String>) -> Result<Self, Self::Error> {
        if let Some(url) = value.get("source-url").or(value.get("url")) {
            if url.starts_with("git://") || url.ends_with(".git") {
                return Ok(PlatformDependency::Repo(RepoEntry {
                    source_url: url.clone(),
                    version: value.get("version").cloned(),
                    source_path: value.get("source-path").cloned(),
                    destination_name: value.get("destination-name").or(value.get("name")).cloned(),
                }));
            } else if let Ok(source_url) = Url::parse(url)
                && let Some(file_name) = Path::new(source_url.path())
                    .file_name()
                    .and_then(OsStr::to_str)
                && file_name.ends_with("index.json")
                && file_name.starts_with("package_")
                && let Some(name) = value.get("name")
                && let Some(pos) = name.find(':')
                && pos > 0
                && pos < name.len() - 1
            {
                // third-party platforms must be discoverable from a package_*index.json.
                // In this case, a name is also required, and it should have a ":" in
                // the middle of it (eg. `vendor:arch`).
                return Ok(PlatformDependency::Manager(ManagerEntry {
                    name: name.to_owned(),
                    version: value.get("version").cloned(),
                    source_url: Some(url.clone()),
                }));
            } else {
                return Ok(PlatformDependency::Download(DownloadEntry {
                    source_url: url.clone(),
                    source_path: value.get("source-path").cloned(),
                    destination_name: value.get("destination-name").cloned(),
                }));
            }
        } else if let Some(path) = value.get("source-path") {
            return Ok(PlatformDependency::Path(PathEntry {
                source_path: path.clone(),
                name: value.get("name").or(value.get("destination-name")).cloned(),
            }));
        } else if let Some(name) = value.get("name") {
            return Ok(PlatformDependency::Manager(ManagerEntry {
                name: name.clone(),
                version: value.get("version").cloned(),
                source_url: value.get("source-url").cloned(),
            }));
        }
        Err(crate::error::CompileSketchesError::ParseDependencyMapping)
    }
}

/// A struct containing lists of dependencies sorted by type.
///
/// This is used to represent either platform or library dependencies.
#[derive(Debug, Default)]
pub struct Dependencies {
    /// Dependencies managed by the Arduino CLI.
    ///
    /// Uses Arduino's platform or library manager.
    pub manager: Vec<ManagerEntry>,

    /// Dependencies specified as local paths.
    pub path: Vec<PathEntry>,

    /// Dependencies specified as git repositories.
    ///
    /// For private repositories, it is recommended to
    /// checkout the repository with appropriate credentials and
    /// specify the dependency as a [`Self::path`] instead.
    pub repository: Vec<RepoEntry>,

    /// Dependencies specified as direct download URLs.
    pub download: Vec<DownloadEntry>,
}

impl Dependencies {
    #[cfg(feature = "bin")]
    pub(crate) fn from_input(input: Vec<HashMap<String, String>>) -> crate::error::Result<Self> {
        let mut deps = Self::default();
        for map in input {
            match PlatformDependency::try_from(map)? {
                PlatformDependency::Manager(m) => deps.manager.push(m),
                PlatformDependency::Path(p) => deps.path.push(p),
                PlatformDependency::Repo(r) => deps.repository.push(r),
                PlatformDependency::Download(d) => deps.download.push(d),
            }
        }
        Ok(deps)
    }
}

/// A struct for deserializing json output from `arduino-cli core list` command.
#[derive(Debug, Deserialize)]
pub struct InstalledPlatforms {
    /// List of installed platforms
    pub platforms: Vec<InstalledPlatform>,
}

/// A struct representing an installed platform.
///
/// Note, only info relevant to this project (arduino-compile-sketches) is included here.
#[derive(Debug, Deserialize)]
pub struct InstalledPlatform {
    /// Platform ID in the format `vendor:arch`.
    ///
    /// Deserialization also supports older key name "ID" for this field, which was
    /// used by arduino-cli prior to v1.0.
    #[serde(alias = "ID")]
    pub id: String,

    /// Installed version of the platform, e.g. "1.2.3".
    ///
    /// Deserialization also supports older key name "Installed" or "installed" for
    /// this field, which was used by arduino-cli prior to v1.0.
    #[serde(alias = "Installed", alias = "installed")]
    pub installed_version: String,

    /// Latest available version of the platform, e.g. "1.2.4".
    ///
    /// This field is optional because older versions of arduino-cli may not have included it.
    ///
    /// Accuracy of this value may dependent on the last time `arduino-cli core update-index` was run.
    pub latest_version: Option<String>,
}

impl<'a> InstalledPlatforms {
    /// A convenience function to check if a platform is installed.
    ///
    /// Returns `Some(installed_version)` if a platform with the given `name`
    /// (and optional `version`) is installed. Otherwise, this returns [`None`].
    pub fn is_installed(&'a self, name: &str, version: Option<&str>) -> Option<&'a str> {
        for p in &self.platforms {
            if p.id == name {
                match version {
                    Some(v) => {
                        let result = (v == "latest"
                            && p.latest_version
                                .as_ref()
                                .is_some_and(|latest| *latest == p.installed_version))
                            || p.installed_version == *v;
                        if result {
                            log::info!(
                                "Platform already installed: {name}@{}",
                                p.installed_version
                            );
                            return Some(p.installed_version.as_str());
                        }
                    }
                    None => {
                        log::info!("Platform already installed: {name}@{}", p.installed_version);
                        return Some(p.installed_version.as_str());
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::path::PathBuf;

    use super::*;

    #[test]
    #[cfg(feature = "bin")]
    fn parse_platform_input() {
        use std::collections::HashMap;

        let yaml = r#"
- name: arduino:avr
- name: custom:platform
  source-url: https://example.com/package_arch_index.json
- source-path: ./local/platform
  name: Vendor:Arch:Board
- source-url: https://example.com/another-platform.git
  name: vendor:arch:board
- url: https://example.com/download-platform.zip
  destination-name: VENDOR:ARCH
"#;
        let map: Vec<HashMap<String, String>> = serde_saphyr::from_str(yaml).unwrap();
        let deps = Dependencies::from_input(map).unwrap();
        eprintln!("{:#?}", deps);
        assert!(!deps.manager.is_empty());
        assert!(!deps.path.is_empty());
        assert!(!deps.repository.is_empty());
        assert!(!deps.download.is_empty());
        for dep in &deps.download {
            assert_eq!(dep.source_url, "https://example.com/download-platform.zip");
            assert_eq!(dep.destination_name.as_deref(), Some("VENDOR:ARCH"));
        }
        for dep in &deps.path {
            assert_eq!(dep.source_path, "./local/platform");
            assert_eq!(dep.name.as_deref(), Some("Vendor:Arch:Board"));
        }
        for dep in &deps.repository {
            assert_eq!(dep.source_url, "https://example.com/another-platform.git");
            assert_eq!(dep.destination_name.as_deref(), Some("vendor:arch:board"));
        }
        for dep in deps.manager {
            assert!(["custom:platform", "arduino:avr"].contains(&dep.name.as_str()));
            assert!(dep.version.is_none());
            assert!(
                dep.source_url
                    .is_none_or(|url| url.ends_with("package_arch_index.json"))
            );
        }
    }

    #[test]
    fn deserialize_platform_list() {
        let json = PathBuf::from("tests/installed_platforms/core-list.json");
        let json_str = std::fs::read_to_string(json).unwrap();
        let platforms: InstalledPlatforms = serde_json::from_str(&json_str).unwrap();
        assert!(!platforms.platforms.is_empty());
        let is_installed = platforms.is_installed("arduino:avr", None);
        assert_eq!(is_installed, Some("1.8.6"));
        let is_installed_version = platforms.is_installed("esp32:esp32", Some("3.2.0"));
        assert_eq!(is_installed_version, Some("3.2.0"));
        let is_not_installed = platforms.is_installed("nonexistent:platform", None);
        assert_eq!(is_not_installed, None);
        let is_latest = platforms.is_installed("arduino:avr", Some("latest"));
        assert_eq!(is_latest, Some("1.8.6"));
        let is_not_latest = platforms.is_installed("arduino:avr", Some("1.8.5"));
        assert_eq!(is_not_latest, None);
    }

    #[test]
    #[cfg(feature = "bin")]
    fn fail_parse_dep_entry() {
        use crate::error::CompileSketchesError;
        use std::collections::HashMap;

        let map = HashMap::from_iter([("invalid-key".to_string(), "unused_value".to_string())]);
        let result = super::PlatformDependency::try_from(map);
        assert!(result.is_err_and(|e| matches!(e, CompileSketchesError::ParseDependencyMapping)))
    }
}
