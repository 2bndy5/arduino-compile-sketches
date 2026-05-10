use std::{ffi::OsStr, fs, path::PathBuf};

use crate::error::{CompileSketchesError, Result};

use crate::serde_types::{DownloadEntry, PathEntry, RepoEntry};
use crate::{
    driver::CompileSketches,
    serde_types::{InstalledPlatforms, ManagerEntry},
};

#[derive(Default, Debug, Clone)]
struct PlatformInstallPath {
    path: PathBuf,
    is_overwrite: bool,
}

impl CompileSketches {
    /// Installs platform dependencies passed to [`Self::platforms`].
    ///
    /// Note, this will mutate and drain dependencies from [`Self::platforms`] as it installs them,
    /// with the exception of [`Dependencies::manager`](crate::serde_types::Dependencies::manager).
    pub async fn install_platforms(&mut self) -> Result<()> {
        if self.platforms.manager.is_empty()
            && self.platforms.path.is_empty()
            && self.platforms.repository.is_empty()
            && self.platforms.download.is_empty()
        {
            let mut split_iter = self.sketch_compiler.fqbn.split(':');
            if let Some(vendor) = split_iter.next()
                && let Some(arch) = split_iter.next()
            {
                self.platforms.manager.push(ManagerEntry {
                    name: format!("{vendor}:{arch}"),
                    version: None,
                    source_url: None,
                });
            }
        }

        let installed_platforms_json =
            self.run_arduino_cli_command(&["core", "list", "--format", "json"])?;
        let installed_platforms =
            serde_json::from_str::<InstalledPlatforms>(&installed_platforms_json)
                .map_err(|e| CompileSketchesError::ParseInstalledPlatformsJson { source: e })?;
        self.install_platform_from_manager(&installed_platforms)?;
        self.install_platform_from_path(&installed_platforms)?;
        self.install_platform_from_repo(&installed_platforms)?;
        self.install_platform_from_download(&installed_platforms)
            .await?;
        Ok(())
    }

    fn install_platform_from_manager(
        &self,
        installed_platforms: &InstalledPlatforms,
    ) -> Result<()> {
        // Arduino CLI supports doing this all in one command, but it helps troubleshooting to install one at a time
        for platform in &self.platforms.manager {
            // Check if platform is already installed at the requested version
            if installed_platforms
                .is_installed(&platform.name, platform.version.as_deref())
                .is_some()
            {
                continue;
            }

            let mut core_update_index_command = vec!["core", "update-index"];
            let mut core_install_command = vec!["core", "install"];

            // Append additional Boards Manager URLs to the commands, if required
            if let Some(additional_url) = &platform.source_url {
                let additional_urls_option = ["--additional-urls", additional_url.as_str()];
                core_update_index_command.extend(&additional_urls_option);
                core_install_command.extend(&additional_urls_option);
            }
            let manager_dep_name = if let Some(version) = &platform.version
                && version != "latest"
            {
                format!("{}@{}", platform.name, version)
            } else {
                platform.name.clone()
            };
            core_install_command.push(manager_dep_name.as_str());

            // Download the platform index for the platform
            self.run_arduino_cli_command(&core_update_index_command)?;

            // Install the platform
            self.run_arduino_cli_command(&core_install_command)?;
            log::info!("Installed platform {}", platform.name);
        }
        Ok(())
    }

    fn install_platform_from_path(
        &mut self,
        installed_platforms: &InstalledPlatforms,
    ) -> Result<()> {
        let deps = std::mem::take::<Vec<PathEntry>>(&mut self.platforms.path);
        for dep in &deps {
            log::info!("Installing platform from path: {}", dep.source_path);
            let source_path = fs::canonicalize(&dep.source_path).map_err(|source| {
                CompileSketchesError::ResolvePath {
                    path_for: "platform",
                    path: dep.source_path.clone(),
                    source,
                }
            })?;

            let platform_path = self.get_platform_installation_path(
                dep.name
                    .as_ref()
                    .ok_or_else(|| CompileSketchesError::PlatformDepMissingField {
                        key: "name",
                        id: dep.source_path.clone(),
                    })?,
                installed_platforms,
            )?;

            self.install_from_path(
                &source_path,
                platform_path.path.parent().ok_or_else(|| {
                    CompileSketchesError::PlatformDepMissingField {
                        key: "destination parent",
                        id: platform_path.path.to_string_lossy().into_owned(),
                    }
                })?,
                Some(
                    platform_path
                        .path
                        .file_name()
                        .and_then(|s: &OsStr| s.to_str())
                        .ok_or_else(|| CompileSketchesError::PlatformDepMissingField {
                            key: "destination-name",
                            id: platform_path.path.to_string_lossy().into_owned(),
                        })?,
                ),
                platform_path.is_overwrite,
            )?;
        }
        Ok(())
    }

    fn install_platform_from_repo(
        &mut self,
        installed_platforms: &InstalledPlatforms,
    ) -> Result<()> {
        let deps = std::mem::take::<Vec<RepoEntry>>(&mut self.platforms.repository);
        for repo in &deps {
            log::info!("Installing platform from repository: {}", repo.source_url);

            let git_ref = repo.version.clone();
            let source_path = repo.source_path.clone().unwrap_or_else(|| ".".into());
            let name_from_url = repo.destination_name.clone().unwrap_or_else(|| {
                repo.source_url
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("platform")
                    .split('.')
                    .next()
                    .unwrap_or("platform")
                    .to_string()
            });
            let dest_path =
                self.get_platform_installation_path(&name_from_url, installed_platforms)?;
            self.install_from_repository(
                &repo.source_url,
                git_ref.as_deref(),
                &source_path,
                dest_path.path.parent().ok_or_else(|| {
                    CompileSketchesError::PlatformDepMissingField {
                        key: "destination parent",
                        id: dest_path.path.to_string_lossy().into_owned(),
                    }
                })?,
                Some(
                    dest_path
                        .path
                        .file_name()
                        .and_then(|s: &OsStr| s.to_str())
                        .ok_or_else(|| CompileSketchesError::PlatformDepMissingField {
                            key: "destination-name",
                            id: dest_path.path.to_string_lossy().into_owned(),
                        })?,
                ),
                dest_path.is_overwrite,
            )?;
        }
        Ok(())
    }

    async fn install_platform_from_download(
        &mut self,
        installed_platforms: &InstalledPlatforms,
    ) -> Result<()> {
        let deps = std::mem::take::<Vec<DownloadEntry>>(&mut self.platforms.download);
        for dep in &deps {
            log::info!("Installing platform from download url: {}", dep.source_url);

            let source_path = match &dep.source_path {
                Some(p) => p.as_str(),
                None => ".",
            };
            let dest_path_info = self.get_platform_installation_path(
                dep.destination_name.as_ref().ok_or_else(|| {
                    CompileSketchesError::PlatformDepMissingField {
                        key: "name",
                        id: dep.source_url.clone(),
                    }
                })?,
                installed_platforms,
            )?;
            self.install_from_download(
                &dep.source_url,
                source_path,
                dest_path_info.path.parent().ok_or_else(|| {
                    CompileSketchesError::PlatformDepMissingField {
                        key: "destination parent",
                        id: dest_path_info.path.to_string_lossy().into_owned(),
                    }
                })?,
                Some(
                    dest_path_info
                        .path
                        .file_name()
                        .and_then(|s: &OsStr| s.to_str())
                        .unwrap_or(""),
                ),
                true,
            )
            .await?;
        }
        Ok(())
    }

    fn get_platform_installation_path(
        &self,
        name: &str,
        installed_platforms: &InstalledPlatforms,
    ) -> Result<PlatformInstallPath> {
        let mut split_iter = name.split(':');
        let vendor =
            split_iter
                .next()
                .ok_or_else(|| CompileSketchesError::PlatformDepMissingField {
                    key: "vendor",
                    id: name.to_string(),
                })?;
        let arch =
            split_iter
                .next()
                .ok_or_else(|| CompileSketchesError::PlatformDepMissingField {
                    key: "architecture",
                    id: name.to_string(),
                })?;

        let mut result = PlatformInstallPath {
            path: self.user_platforms_path.join(vendor).join(arch),
            is_overwrite: false,
        };

        // Note from og action:
        // // I have no clue why this is needed, but arduino-cli core list fails if this isn't done first. The 3rd party
        // // platforms are still shown in the list even if their index URLs are not specified to the command via the
        // // --additional-urls option
        // self.run_arduino_cli_command(&["core", "update-index"]).with_context(|| "Failed to update platform indexes")?;

        // conditionally override install path per already installed platforms
        if let Some(version) = installed_platforms.is_installed(name, None) {
            result = PlatformInstallPath {
                path: self
                    .board_manager_platforms_path
                    .join(vendor)
                    .join("hardware")
                    .join(arch)
                    .join(version),
                is_overwrite: true,
            };
        }

        Ok(result)
    }
}
