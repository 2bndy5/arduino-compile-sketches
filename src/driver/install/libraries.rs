use std::{env, ffi::OsStr, fs};

use crate::error::{CompileSketchesError, Result};

use crate::driver::CompileSketches;
use crate::serde_types::{DownloadEntry, PathEntry, RepoEntry};

impl CompileSketches {
    /// Installs library dependencies passed to [`Self::libraries].
    ///
    /// Note, this will mutate and drain dependencies from [`Self::libraries`] as it installs them,
    /// with the exception of [`Dependencies::manager`](crate::serde_types::Dependencies::manager).
    pub async fn install_libraries(&mut self) -> Result<()> {
        self.install_libraries_from_manager()?;
        self.install_libraries_from_path()?;
        self.install_libraries_from_repo()?;
        self.install_libraries_from_download().await?;
        Ok(())
    }

    fn install_libraries_from_manager(&self) -> Result<()> {
        // Note from og action:
        // // `arduino-cli lib install` fails if one of the libraries in the list has a dependency on another, but an
        // // earlier version of the dependency is specified in the list.
        // // The solution is to install one library at a time
        // // (even though `arduino-cli lib install` supports installing multiple libraries at once).
        // // This also allows the user to control which version is installed via the order of the `libraries` input list items.
        for dep in &self.libraries.manager {
            let mut lib_install_cmd = vec!["lib", "install"];
            let lib_dep_name = if let Some(version) = &dep.version
                && version != "latest"
            {
                format!("{}@{version}", dep.name)
            } else {
                dep.name.clone()
            };
            lib_install_cmd.push(lib_dep_name.as_str());
            self.run_arduino_cli_command(&lib_install_cmd)?;
            log::info!("Installed library {}", dep.name);
        }
        Ok(())
    }

    fn install_libraries_from_path(&mut self) -> Result<()> {
        let deps = std::mem::take::<Vec<PathEntry>>(&mut self.libraries.path);
        for dep in &deps {
            log::info!("Installing library from path: {}", dep.source_path);

            let source_path = fs::canonicalize(&dep.source_path).map_err(|source| {
                CompileSketchesError::ResolvePath {
                    path_for: "library",
                    path: dep.source_path.clone(),
                    source,
                }
            })?;

            let dest_name = match &dep.name {
                Some(p) => Some(p.as_str()),
                None => {
                    let cwd = env::current_dir().map_err(|source| {
                        CompileSketchesError::DetectWorkingDirectory { source }
                    })?;
                    if source_path == cwd {
                        source_path.file_name().and_then(|s: &OsStr| s.to_str())
                    } else {
                        None
                    }
                }
            };
            self.install_from_path(
                &source_path,
                self.libraries_path.clone().as_path(),
                dest_name,
                true,
            )?;
        }
        Ok(())
    }

    fn install_libraries_from_repo(&mut self) -> Result<()> {
        let deps = std::mem::take::<Vec<RepoEntry>>(&mut self.libraries.repository);
        for dep in &deps {
            log::info!("Installing library from repository: {}", dep.source_url);

            let git_ref = dep.version.as_deref();
            let source_path = match &dep.source_path {
                Some(p) => p.as_str(),
                None => "./",
            };
            let dest_name = dep.destination_name.as_deref();

            self.install_from_repository(
                &dep.source_url,
                git_ref,
                source_path,
                self.libraries_path.clone().as_path(),
                dest_name,
                true,
            )?;
        }
        Ok(())
    }

    async fn install_libraries_from_download(&mut self) -> Result<()> {
        let deps = std::mem::take::<Vec<DownloadEntry>>(&mut self.libraries.download);
        for dep in &deps {
            log::info!("Installing library from download URL: {}", dep.source_url);

            let source_path = match &dep.source_path {
                Some(p) => p.as_str(),
                None => "./",
            };
            let dest_name = dep.destination_name.as_deref();

            self.install_from_download(
                &dep.source_url,
                source_path,
                self.libraries_path.clone().as_path(),
                dest_name,
                true,
            )
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use std::path::PathBuf;

    use crate::{
        driver::DefaultPaths,
        serde_types::{Dependencies, ManagerEntry},
    };

    use super::*;

    #[test]
    fn unresolved_path_lib_source_dir() {
        let deps = Dependencies {
            path: vec![PathEntry {
                name: None,
                source_path: "nonexistent/path/to/library".to_string(),
            }],
            ..Default::default()
        };
        let mut driver = CompileSketches {
            libraries: deps,
            ..Default::default()
        };
        let Err(result) = driver.install_libraries_from_path() else {
            panic!("Expected error when library source path cannot be resolved");
        };
        assert!(matches!(result, CompileSketchesError::ResolvePath { .. }));
    }

    #[test]
    fn install_path_lib_from_sub_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let lib_dir = PathBuf::from("tests/dep_fixtures/path-lib");

        let deps = Dependencies {
            path: vec![PathEntry {
                name: None,
                source_path: lib_dir.to_str().unwrap().to_string(),
            }],
            ..Default::default()
        };
        let mut driver = CompileSketches {
            libraries: deps,
            libraries_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let new_default_paths = DefaultPaths::new_in(&temp_dir.path().join("test-ws"));
        driver.relocate_paths(new_default_paths);

        driver.install_libraries_from_path().unwrap();
        driver.clean_up_tmp_assets().unwrap();
    }

    /// installs the ArduinoJson lib multiple times with different requested versions.
    #[tokio::test]
    async fn install_from_mgr() {
        #[cfg(feature = "bin")]
        {
            crate::logger::init();
            log::set_max_level(log::LevelFilter::Debug);
        }

        let mut driver = CompileSketches {
            libraries: Dependencies {
                manager: vec![
                    // lib with implicit latest version
                    ManagerEntry {
                        name: "ArduinoJson".to_string(),
                        version: None,
                        ..Default::default()
                    },
                    // lib with explicit version (not the latest)
                    ManagerEntry {
                        name: "ArduinoJson".to_string(),
                        version: Some("7.0.0".to_string()),
                        ..Default::default()
                    },
                    // lib with literal "latest" version
                    ManagerEntry {
                        name: "ArduinoJson".to_string(),
                        version: Some("latest".to_string()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        };

        let tmp_ws = tempfile::tempdir().unwrap();
        let new_default_paths = DefaultPaths::new_in(tmp_ws.path());
        driver.relocate_paths(new_default_paths);

        driver.install_arduino_cli().await.unwrap();
        driver.install_libraries_from_manager().unwrap();
    }
}
