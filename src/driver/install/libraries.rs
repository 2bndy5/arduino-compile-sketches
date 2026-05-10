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
