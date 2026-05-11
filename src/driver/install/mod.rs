use url::Url;
mod libraries;
mod platforms;

use std::{
    ffi::OsStr,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::{driver::CompileSketches, utils::create_symlink};
use crate::{
    error::{CompileSketchesError, Result},
    utils::fmt_duration,
};

impl CompileSketches {
    /// Installs the Arduino CLI if it is not already installed.
    pub async fn install_arduino_cli(&mut self) -> Result<()> {
        if self.sketch_compiler.arduino_cli_path.is_some() {
            return Ok(());
        }

        let version = &self.cli_version;
        log::info!("Installing arduino-cli version {version}");

        let (archive_file_name, bin_name) = if cfg!(windows) {
            (
                format!("arduino-cli_{version}_Windows_64bit.zip"),
                "arduino-cli.exe",
            )
        } else {
            (
                format!("arduino-cli_{version}_Linux_64bit.tar.gz"),
                "arduino-cli",
            )
        };
        let download_url = format!("https://downloads.arduino.cc/arduino-cli/{archive_file_name}",);

        let cache_base = directories::ProjectDirs::from("org", "arduino", "compile-sketches")
            .map(|project_dir| project_dir.cache_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".").join(".arduino-compile-sketches_cache"));
        let lock_file_path = cache_base.join(".lock");
        let lock_file = fs::File::create(&lock_file_path)?;
        lock_file.lock()?;

        let install_dir = cache_base.join(format!("arduino-cli_{version}"));

        if !install_dir.exists() {
            fs::create_dir_all(&install_dir)?;
        }

        let bin_path = install_dir.join(bin_name);
        let ver_is_latest = version.eq_ignore_ascii_case("latest");
        if bin_path.exists() && !ver_is_latest {
            log::debug!(
                "Reusing cached arduino-cli installation at {}",
                bin_path.to_string_lossy()
            );
        } else if bin_path.exists()
            && ver_is_latest
            && let Ok(last_modified) = fs::metadata(&bin_path).and_then(|m| m.modified())
            && let Ok(elapsed) = last_modified.elapsed()
            && elapsed < Duration::from_hours(24)
        {
            log::debug!(
                "Reusing cached 'latest' arduino-cli installation (last modified {} ago)",
                fmt_duration(&elapsed)
            );
        } else {
            self.install_from_download(&download_url, bin_name, &install_dir, None, ver_is_latest)
                .await?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if bin_path.exists() {
                    let mut perms = fs::metadata(&bin_path)?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&bin_path, perms)?;
                }
            }
        }
        lock_file.unlock()?;

        self.sketch_compiler.arduino_cli_path = Some(bin_path);

        // check version of extracted binary
        let output = self.run_arduino_cli_command(&["version"])?;
        let version = &self.cli_version;
        let version_pattern =
            regex::Regex::new(r"(?:i)[^vV]*Version\:[^0-9]*([0-9]+\.[0-9]+\.[0-9-rc.]+).*")?;
        if let Some(captures) = version_pattern.captures(&output)
            && let Some(ver) = captures.get(1).map(|v| v.as_str())
        {
            if version.as_str() != "latest" && !version.is_empty() && ver != version {
                log::warn!(
                    "Using arduino-cli version {ver} which does not match requested version {version}"
                );
            } else {
                log::info!("Using arduino-cli version {ver}");
            }
        } else {
            log::warn!("Failed to determine version of installed arduino-cli");
        }
        Ok(())
    }

    fn run_arduino_cli_command(&self, args: &[&str]) -> Result<String> {
        let mut cmd = self.sketch_compiler.build_cli_command(args)?;
        let output = cmd
            .output()
            .map_err(|error| CompileSketchesError::InvokeArduinoCli {
                command: args.join(" "),
                error,
            })?;
        let mut combined_output = String::new();
        combined_output.push_str(&String::from_utf8_lossy(&output.stdout));
        combined_output.push_str(&String::from_utf8_lossy(&output.stderr));
        if !output.status.success() {
            Err(CompileSketchesError::ArduinoCliCommandFailed {
                output: combined_output,
            })
        } else {
            if self.sketch_compiler.verbose {
                log::info!(target: "CI_LOG_CMD", "{combined_output}")
            }
            Ok(combined_output)
        }
    }

    fn install_from_path(
        &mut self,
        source_path: &Path,
        destination_parent_path: &Path,
        destination_name: Option<&str>,
        force: bool,
    ) -> Result<()> {
        let dest_name = match destination_name {
            Some(p) => p,
            None => source_path
                .file_name()
                .and_then(|s: &OsStr| s.to_str())
                .ok_or_else(|| {
                    CompileSketchesError::MalformedInstallDestPathName(
                        source_path.to_string_lossy().to_string(),
                    )
                })?,
        };
        let destination_path = destination_parent_path.join(dest_name);

        if destination_path.exists() || destination_path.is_symlink() {
            if force {
                log::debug!(
                    "Overwriting installation at {}",
                    destination_path.to_string_lossy()
                );
                fs::remove_file(&destination_path)
                    .or_else(|_| fs::remove_dir_all(&destination_path))
                    .map_err(|source| CompileSketchesError::DeleteExistingInstalledPath {
                        path: destination_path.to_string_lossy().into_owned(),
                        source,
                    })?;
            } else {
                return Err(CompileSketchesError::InstallPathAlreadyExists {
                    path: destination_path.to_string_lossy().into_owned(),
                });
            }
        }

        if !destination_parent_path.exists() {
            fs::create_dir_all(destination_parent_path)?;
        }

        create_symlink(source_path, &destination_path).map_err(|source| {
            CompileSketchesError::CreateSymlink {
                source_path: source_path.to_string_lossy().into_owned(),
                destination_path: destination_path.to_string_lossy().into_owned(),
                source,
            }
        })?;
        // Handle cleanup of the symlink on normal exit
        self.clean_up_paths.push(destination_path);

        Ok(())
    }

    fn install_from_repository(
        &mut self,
        url: &str,
        git_ref: Option<&str>,
        source_path: &str,
        destination_parent_path: &Path,
        destination_name: Option<&str>,
        force: bool,
    ) -> Result<()> {
        let mut tmp =
            tempfile::TempDir::with_prefix("install_from_repository-").map_err(|source| {
                CompileSketchesError::TempPathIo {
                    task: "create temp directory for repository clone",
                    source,
                }
            })?;
        // Don't let the tempdir be automatically deleted on drop.
        // We'll clean it up when exiting normally
        tmp.disable_cleanup(true);
        let clone_path = tmp.path();
        self.clean_up_paths.push(clone_path.to_path_buf());
        let mut clone_cmd = Command::new("git");
        if git_ref.is_none() {
            clone_cmd.args(["clone", "--depth", "1", "--recursive"]);
        }
        clone_cmd.args([url, clone_path.to_string_lossy().to_string().as_str()]);
        let clone_out =
            clone_cmd
                .output()
                .map_err(|source| CompileSketchesError::GitCommandIo {
                    task: "clone repository",
                    source,
                })?;
        if !clone_out.status.success() {
            return Err(CompileSketchesError::GitCommandFailed {
                task: "clone repository",
            });
        }
        if let Some(gr) = git_ref {
            let git_ref = if gr == "latest" {
                // Resolve "latest" as a git ref; fall back to the latest tag if "latest" doesn't exist.
                let rev_parsed = Command::new("git")
                    .current_dir(clone_path)
                    .args(["rev-parse", gr])
                    .output()
                    .map_err(|source| CompileSketchesError::GitCommandIo {
                        task: "resolve `latest` with `git rev-parse`",
                        source,
                    })?;
                if rev_parsed.status.success() {
                    String::from_utf8_lossy(&rev_parsed.stdout)
                        .trim()
                        .to_string()
                } else {
                    let tag_list = Command::new("git")
                        .current_dir(clone_path)
                        .args([
                            "tag",
                            // spell-checker: disable-next-line
                            "--sort=-creatordate",
                        ])
                        .output()
                        .map_err(|source| CompileSketchesError::GitCommandIo {
                            task: "list tags for resolving `latest`",
                            source,
                        })?;
                    if !tag_list.status.success() {
                        return Err(CompileSketchesError::GitCommandFailed {
                            task: "list tags for resolving `latest`",
                        });
                    }
                    let tags = String::from_utf8_lossy(&tag_list.stdout);
                    tags.lines()
                        .next()
                        .ok_or_else(|| CompileSketchesError::GitCommandFailed {
                            task: "resolve `latest` git ref from available tags",
                        })?
                        .trim()
                        .to_string()
                }
            } else {
                gr.to_string()
            };

            // Checkout the specified git ref
            let checkout = Command::new("git")
                .current_dir(clone_path)
                .args(["checkout", &git_ref])
                .output()
                .map_err(|source| CompileSketchesError::GitCommandIo {
                    task: "checkout requested git ref",
                    source,
                })?;
            if !checkout.status.success() {
                return Err(CompileSketchesError::GitCommandFailed {
                    task: "checkout requested git ref",
                });
            }
        }

        // init submodules as shallow
        let submodule_out = Command::new("git")
            .current_dir(clone_path)
            .args([
                "submodule",
                "update",
                "--init",
                "--recursive",
                "--depth",
                "1",
            ])
            .output()
            .map_err(|source| CompileSketchesError::GitCommandIo {
                task: "initialize git submodules",
                source,
            })?;
        if !submodule_out.status.success() {
            return Err(CompileSketchesError::GitCommandFailed {
                task: "initialize git submodules",
            });
        }

        self.install_from_path(
            &clone_path.join(source_path),
            destination_parent_path,
            destination_name,
            force,
        )
    }

    async fn install_from_download(
        &mut self,
        url: &str,
        source_path: &str,
        destination_parent_path: &Path,
        destination_name: Option<&str>,
        force: bool,
    ) -> Result<()> {
        let mut archive_path =
            tempfile::tempfile().map_err(|source| CompileSketchesError::TempPathIo {
                task: "create temp file for archive download",
                source,
            })?;

        let mut resp = self.http_client.get(url).send().await?;
        resp.error_for_status_ref()?;
        while let Some(data) = resp.chunk().await? {
            archive_path.write_all(&data)?;
        }
        archive_path.flush()?;

        let mut extract_dir =
            tempfile::TempDir::with_prefix("install_from_download-").map_err(|source| {
                CompileSketchesError::TempPathIo {
                    task: "create temp path for archive extraction",
                    source,
                }
            })?;
        // Don't delete the extract dir on drop.
        // We'll clean it up when app exits normally
        extract_dir.disable_cleanup(true);
        self.clean_up_paths.push(extract_dir.path().to_path_buf());
        let url_parsed = Url::parse(url)?;
        let url_path = PathBuf::from(url_parsed.path());
        let filename = url_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        extract_archive(&mut archive_path, extract_dir.path(), filename)?;

        let archive_root = get_archive_root_path(extract_dir.path())?;
        let src = fs::canonicalize(archive_root.join(source_path)).map_err(|source| {
            CompileSketchesError::ArchiveExtractionIo {
                task: "resolve extracted archive root path",
                source,
            }
        })?;
        self.install_from_path(&src, destination_parent_path, destination_name, force)?;
        Ok(())
    }
}

/// Determine the archive root folder given an extraction directory.
///
/// If the extraction contains a single top-level directory, return that directory,
/// otherwise return the extraction directory itself.
fn get_archive_root_path(extract_dir: &Path) -> Result<PathBuf> {
    let mut found_dir: Option<PathBuf> = None;
    for entry in
        fs::read_dir(extract_dir).map_err(|source| CompileSketchesError::ArchiveExtractionIo {
            task: "read extracted archive directory",
            source,
        })?
    {
        let p = entry
            .map_err(|source| CompileSketchesError::ArchiveExtractionIo {
                task: "access extracted archive sub-path",
                source,
            })?
            .path();
        if p.is_dir() {
            if let Some(name) = p.file_name().and_then(|s| s.to_str())
                && name != "__MACOSX"
            {
                if found_dir.is_none() {
                    // first dir found
                    found_dir = Some(p);
                } else {
                    // multiple directories found
                    found_dir = Some(extract_dir.to_path_buf());
                    break;
                }
            }
        } else {
            // file found => return extract_dir
            found_dir = Some(extract_dir.to_path_buf());
            break;
        }
    }
    Ok(found_dir.unwrap_or_else(|| extract_dir.to_path_buf()))
}

fn extract_archive(archive_path: &mut fs::File, extract_dir: &Path, filename: &str) -> Result<()> {
    if filename.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(archive_path)?;
        log::debug!("Extracting ZIP archive with {} entries", zip.len());
        zip.extract(extract_dir)?;
    } else if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        let dec = flate2::read::GzDecoder::new(archive_path);
        let mut ar = tar::Archive::new(dec);
        ar.unpack(extract_dir)?;
    } else if filename.ends_with(".tar") {
        let mut ar = tar::Archive::new(archive_path);
        ar.unpack(extract_dir)?;
    } else {
        return Err(CompileSketchesError::ArchiveFormatUnsupported(
            filename.to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // #![allow(clippy::panic)]

    use std::path::PathBuf;

    use crate::CompileSketchesError;

    use super::get_archive_root_path;

    #[test]
    fn find_archive_root() {
        let test_assets_path = PathBuf::from("tests/archive_root_assets");
        let has_file_path = test_assets_path.join("has-file");
        let result = get_archive_root_path(&has_file_path).unwrap();
        assert_eq!(result, has_file_path);

        let has_folders_path = test_assets_path.join("has-folders");
        let result = get_archive_root_path(&has_folders_path).unwrap();
        assert_eq!(result, has_folders_path);

        let has_root_path = test_assets_path.join("has-root");
        let result = get_archive_root_path(&has_root_path).unwrap();
        assert_eq!(result, has_root_path.join("root"));

        let non_existent_path = PathBuf::from("does-not-exist");
        let err = get_archive_root_path(&non_existent_path).unwrap_err();
        let CompileSketchesError::ArchiveExtractionIo { task, source: _ } = err else {
            panic!("Expected ArchiveExtractionIo error, got: {:?}", err);
        };
        assert_eq!(task, "read extracted archive directory");
    }
}
