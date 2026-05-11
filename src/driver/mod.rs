use arduino_report_size_deltas::report_structs::{Board, Report, Sketch};
use reqwest::Client;
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};
use tokio::task::JoinSet;

use crate::utils::{get_base_ref, get_head_ref, path_is_sketch, path_relative_to_workspace};
use crate::{
    error::{CompileSketchesError, Result},
    utils::visit_dirs_recursive,
};
use crate::{serde_types::Dependencies, utils::fmt_duration};

mod compiler;
mod install;
mod report;

use self::compiler::{
    CompilationTaskResult, CompileRef, CompileTaskEnvelope, checkout_base_ref, compile_sketch_task,
};
pub use compiler::SketchCompiler;
use report::{
    apply_base_report, get_board_sizes_from_summary, get_sizes_from_output,
    get_sizes_summary_report, get_warning_count_from_output,
};

/// Helper struct to provide default paths.
pub struct DefaultPaths {
    /// The path to the Arduino CLI user directory.
    pub arduino_cli_user_directory_path: PathBuf,

    /// The path to Arduino libraries directory.
    pub libraries_path: PathBuf,

    /// The path to the user platforms directory.
    pub user_platforms_path: PathBuf,

    /// The path to the Arduino CLI data directory.
    pub arduino_cli_data_directory_path: PathBuf,

    /// The path to the board manager platforms directory.
    pub board_manager_platforms_path: PathBuf,
}

impl Default for DefaultPaths {
    fn default() -> Self {
        let home = directories::UserDirs::new()
            .map(|usr_dir| usr_dir.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let arduino_cli_user_directory_path = home.join("Arduino");
        let libraries_path = arduino_cli_user_directory_path.join("libraries");
        let user_platforms_path = arduino_cli_user_directory_path.join("hardware");
        let arduino_cli_data_directory_path = home.join(".arduino15");
        let board_manager_platforms_path = arduino_cli_data_directory_path.join("packages");
        Self {
            arduino_cli_user_directory_path,
            libraries_path,
            user_platforms_path,
            arduino_cli_data_directory_path,
            board_manager_platforms_path,
        }
    }
}

/// The main struct used to drive library behavior.
///
/// This struct can be used to setup and execute the sketch compilation process.
/// It is also responsible for generating reports and cleaning up temporary assets.
#[derive(Debug)]
pub struct CompileSketches {
    /// The version of the Arduino CLI to be used.
    pub cli_version: String,

    /// The platform dependencies to be used.
    pub platforms: Dependencies,

    /// The library dependencies to be used.
    pub libraries: Dependencies,

    /// The paths to the sketches to be compiled.
    pub sketch_paths: Vec<PathBuf>,

    /// Whether to fail on compile errors.
    ///
    /// Defaults to `true`.
    pub fail_on_compile_error: bool,

    /// Whether to enable deltas report generation.
    ///
    /// Defaults to `false`.
    /// Only applies to PR events.
    pub enable_deltas_report: bool,

    /// The path for storing generated reports.
    pub sketches_report_path: PathBuf,

    /// The paths used for installing libraries.
    pub libraries_path: PathBuf,

    /// The path used for installing platforms.
    pub user_platforms_path: PathBuf,

    /// The path used for installing platforms from the Arduino Board Manager.
    pub board_manager_platforms_path: PathBuf,

    /// The HTTP client to use for making requests.
    ///
    /// Typically used for downloaded dependencies.
    pub http_client: Client,

    /// Any temporary paths that should be cleaned up.
    ///
    /// These paths will be deleted just before a normal exit.
    /// So, the paths here are not atomic; any propagated errors can prevent purging these paths.
    ///
    /// Typically, this is used for installing dependencies (libraries or platforms)
    /// without using the arduino-cli (which basically drives the Arduino IDE library or platform managers).
    pub clean_up_paths: Vec<PathBuf>,

    /// The sketch compiler to use for compiling sketches.
    ///
    /// The is used to keep thread-safe resources shared (cloned actually)
    /// across parallel compilation tasks.
    pub sketch_compiler: SketchCompiler,
}

impl Default for CompileSketches {
    /// Provides default values for [`CompileSketches`] fields.
    ///
    /// This is only useful if wrapping this library in your own CLI (or for testing).
    ///
    /// It provides reasonable defaults for paths, `fqbn` (`"arduino:avr:uno"`),
    /// and a HTTP client with a UserAgent header based on this library's name and version.
    ///
    /// ## Panics
    ///
    /// This function will panic if building the HTTP client fails, which can happen if
    /// the TLS backend fails to instantiate. See [`reqwest::ClientBuilder::build`] for more details.
    ///
    /// To use your own customized HTTP client, construct a [`CompileSketches`] that uses
    /// [`DefaultPaths`] as a helper.
    fn default() -> Self {
        let default_paths = DefaultPaths::default();
        let sketch_compiler = SketchCompiler {
            fqbn: "arduino:avr:uno".to_string(),
            cli_compile_flags: vec![],
            arduino_cli_path: None,
            arduino_cli_user_directory_path: default_paths.arduino_cli_user_directory_path,
            arduino_cli_data_directory_path: default_paths.arduino_cli_data_directory_path,
            enable_warnings_report: false,
            verbose: false,
        };
        #[allow(
            clippy::expect_used,
            reason = "fn default() signature cannot return a Result"
        )]
        let client = reqwest::ClientBuilder::new()
            .user_agent(concat!(
                "arduino-compile-sketches/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("Failed to build HTTP client");
        Self {
            cli_version: "latest".to_string(),
            platforms: Dependencies::default(),
            libraries: Dependencies::default(),
            sketch_paths: vec![],
            fail_on_compile_error: true,
            enable_deltas_report: false,
            sketches_report_path: PathBuf::from("Reports"),
            libraries_path: default_paths.libraries_path,
            user_platforms_path: default_paths.user_platforms_path,
            board_manager_platforms_path: default_paths.board_manager_platforms_path,
            http_client: client,
            clean_up_paths: vec![],
            sketch_compiler,
        }
    }
}

impl CompileSketches {
    /// Creates a new [`CompileSketches`] instance from environment variables and CLI arguments.
    #[cfg(feature = "bin")]
    pub fn new_from_env() -> Result<Self> {
        use crate::cli::CliArgs;
        use clap::Parser;
        use std::collections::HashMap;

        let args = CliArgs::parse_from(
            // compile-time only check for integration testing
            if option_env!("ARDUINO_COMPILE_SKETCHES")
                .is_some_and(|v| v == "INTEGRATION TESTS SKIP CLI ARGS")
            {
                vec![]
            } else {
                env::args().collect::<Vec<String>>()
            },
        );

        let platforms =
            match serde_saphyr::from_str::<Vec<HashMap<String, String>>>(&args.platforms) {
                Ok(list) => Dependencies::from_input(list)?,
                Err(e) => {
                    return Err(CompileSketchesError::DecodeYamlDepList {
                        dep_type: "platforms",
                        input: args.platforms,
                        source: Box::new(e),
                    });
                }
            };
        let libraries =
            match serde_saphyr::from_str::<Vec<HashMap<String, String>>>(&args.libraries) {
                Ok(list) => Dependencies::from_input(list)?,
                Err(e) => {
                    return Err(CompileSketchesError::DecodeYamlDepList {
                        dep_type: "libraries",
                        input: args.libraries,
                        source: Box::new(e),
                    });
                }
            };
        let sketch_paths = match serde_saphyr::from_str::<Vec<String>>(&args.sketch_paths) {
            Ok(vec) => vec.into_iter().map(PathBuf::from).collect(),
            Err(_) => args
                .sketch_paths
                .split_whitespace()
                .map(PathBuf::from)
                .collect(),
        };
        let cli_compile_flags: Vec<String> = match &args.cli_compile_flags {
            Some(flags) => serde_saphyr::from_str::<Vec<String>>(flags).unwrap_or(
                flags
                    .split_whitespace()
                    .map(|p| p.to_string())
                    .collect::<Vec<String>>(),
            ),
            None => vec![],
        };

        let default_paths = DefaultPaths::default();

        // Build HTTP client with default User-Agent
        let http_client = Client::builder()
            .user_agent(concat!(
                "arduino-compile-sketches/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        let sketch_compiler = SketchCompiler {
            fqbn: args.fqbn,
            cli_compile_flags,
            arduino_cli_path: None,
            arduino_cli_user_directory_path: default_paths.arduino_cli_user_directory_path,
            arduino_cli_data_directory_path: default_paths.arduino_cli_data_directory_path,
            enable_warnings_report: args.enable_warnings_report,
            verbose: args.verbose,
        };

        Ok(Self {
            cli_version: args.cli_version,
            platforms,
            libraries,
            sketch_paths,
            fail_on_compile_error: args.fail_on_compile_error,
            enable_deltas_report: args.enable_deltas_report,
            sketches_report_path: args.sketches_report_path,
            libraries_path: default_paths.libraries_path,
            user_platforms_path: default_paths.user_platforms_path,
            board_manager_platforms_path: default_paths.board_manager_platforms_path,
            http_client,
            clean_up_paths: Vec::new(),
            sketch_compiler,
        })
    }

    /// Compiles sketches and generates reports.
    ///
    /// This also installs all dependencies and arduino-cli per fields in the [`CompileSketches`] struct.
    pub async fn compile_sketches(&mut self) -> Result<()> {
        let sketches = self.find_sketches()?;
        let sketch_count = sketches.len();
        if sketches.is_empty() {
            log::warn!("No sketches found for paths: {:?}", self.sketch_paths);
            return Err(CompileSketchesError::NoSketchesFound);
        }
        self.install_arduino_cli().await?;
        self.install_platforms().await?;
        self.install_libraries().await?;
        if !self.sketches_report_path.exists() {
            fs::create_dir_all(&self.sketches_report_path)?;
        }
        let repo = env::var("GITHUB_REPOSITORY")
            .map_err(|e| CompileSketchesError::EnvVar("GITHUB_REPOSITORY", e))?;

        // Only compile the base ref for pull requests.
        let is_pr_event = env::var("GITHUB_EVENT_NAME").is_ok_and(|v| v == "pull_request");
        let base_ref_checkout = if self.enable_deltas_report
            && is_pr_event
            && let base_ref =
                get_base_ref().ok_or_else(|| CompileSketchesError::UnknownGitRef("base"))?
        {
            // Clone base ref once so both head and base compilations can share one JoinSet pipeline.
            if let Some(base_checkout) = checkout_base_ref(&base_ref, &repo)? {
                Some(base_checkout)
            } else {
                log::warn!("Failed to checkout base ref {base_ref}; deltas will be skipped");
                None
            }
        } else {
            None
        };

        let mut compile_jobs = JoinSet::new();
        let mut sketch_reports = Vec::with_capacity(sketch_count);
        let mut base_sketch_reports = Vec::with_capacity(sketch_count);

        for sketch in sketches.into_iter() {
            let compiler = self.sketch_compiler.clone();
            let relative_sketch_path = path_relative_to_workspace(&sketch)?;

            // Head ref task.
            let sketch_for_head = sketch.clone();
            let rel_for_head = relative_sketch_path.clone();
            compile_jobs.spawn_blocking(move || CompileTaskEnvelope {
                compile_ref: CompileRef::Head,
                result: compile_sketch_task(compiler, sketch_for_head, rel_for_head),
            });

            // Base ref task (optional).
            if let Some(base_ref) = &base_ref_checkout {
                let compiler = self.sketch_compiler.clone();
                let sketch_in_base = base_ref.temp_dir.path().join(&relative_sketch_path);
                if sketch_in_base.exists() {
                    compile_jobs.spawn_blocking(move || CompileTaskEnvelope {
                        compile_ref: CompileRef::Base,
                        result: compile_sketch_task(compiler, sketch_in_base, relative_sketch_path),
                    });
                } else {
                    log::info!(
                        "Sketch path {} does not exist in base ref {}; likely introduced on head ref. Skipping compilation for this sketch on base ref.",
                        relative_sketch_path,
                        base_ref.base_ref
                    );
                }
            }
        }

        let mut all_compilations_successful = true;
        while let Some(task_result) = compile_jobs.join_next().await {
            let task_result = task_result?;

            let base_ref_str = base_ref_checkout
                .as_ref()
                .and_then(|base| {
                    if matches!(task_result.compile_ref, CompileRef::Base) {
                        Some(format!(" (at base ref {})", base.base_ref))
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            match task_result.result {
                CompilationTaskResult::Ok {
                    relative_sketch_path,
                    output,
                    success,
                    invoked_cmd,
                    duration,
                } => {
                    // task completed, so log it.
                    log::info!(
                        target: "CI_LOG_CMD",
                        "::group::Compilation for {relative_sketch_path}{base_ref_str} with {invoked_cmd}",
                    );
                    if !success {
                        log::error!(
                            target: "CI_LOG_CMD",
                            "::error::Compilation failed for {relative_sketch_path}{base_ref_str}",
                        );
                        if matches!(task_result.compile_ref, CompileRef::Head) {
                            all_compilations_successful = false;
                        }
                        log::error!(target: "CI_LOG_CMD", "{output}");
                    } else if self.sketch_compiler.verbose {
                        log::debug!(target: "CI_LOG_CMD", "{output}");
                    }
                    log::info!(target: "CI_LOG_CMD", "::endgroup::");
                    log::info!("Compilation time elapsed: {}", fmt_duration(&duration));

                    // now extract data for reports
                    let sizes = if success {
                        get_sizes_from_output(&output)?
                    } else {
                        Vec::new()
                    };
                    let warnings = if self.sketch_compiler.enable_warnings_report {
                        Some(get_warning_count_from_output(&output)?)
                    } else {
                        None
                    };

                    let sketch = Sketch {
                        name: relative_sketch_path,
                        compilation_success: success,
                        sizes,
                        warnings,
                    };
                    if matches!(task_result.compile_ref, CompileRef::Base) {
                        base_sketch_reports.push(sketch);
                    } else {
                        sketch_reports.push(sketch);
                    }
                }
                CompilationTaskResult::Err {
                    relative_sketch_path,
                    error,
                    duration,
                } => {
                    // if task failed to execute (I/O problems): just log it and move on.
                    log::info!(
                        target: "CI_LOG_CMD",
                        "::group::Compilation task failed for {}{base_ref_str}",
                        relative_sketch_path
                    );
                    log::error!(target: "CI_LOG_CMD", "::error::{error}");
                    if base_ref_str.is_empty() {
                        // overall compilation failure is not affected by any base ref compilations
                        all_compilations_successful = false;
                    }
                    log::info!(target: "CI_LOG_CMD", "::endgroup::");
                    log::info!("Compilation time elapsed: {}", fmt_duration(&duration));
                }
            }
        }

        // we're done with the temp checkout of the base ref.
        // dropping it will automatically purge the temp directory.
        drop(base_ref_checkout);

        let commit_hash =
            get_head_ref().ok_or_else(|| CompileSketchesError::UnknownGitRef("head"))?;
        let commit_url = format!("https://github.com/{repo}/commit/{commit_hash}");

        let board_sizes = if self.enable_deltas_report {
            apply_base_report(&mut sketch_reports, &base_sketch_reports);
            let sizes_summary = get_sizes_summary_report(&sketch_reports);
            get_board_sizes_from_summary(&sizes_summary)
        } else {
            None
        };

        let report = Report {
            commit_hash,
            commit_url,
            boards: vec![Board {
                board: self.sketch_compiler.fqbn.clone(),
                sketches: sketch_reports,
                sizes: board_sizes,
            }],
        };

        let out_path = self
            .sketches_report_path
            .join(self.sketch_compiler.fqbn.replace(':', "-") + ".json");

        // Serialize and write the canonical `Report`.
        let json = serde_json::to_string(&report)?;
        fs::write(out_path, json)?;

        log::info!(
            "Sketches report written to {}",
            self.sketches_report_path.to_string_lossy()
        );

        self.clean_up_tmp_assets()?;

        if self.fail_on_compile_error && !all_compilations_successful {
            log::error!(target: "CI_LOG_CMD", "::error::One or more compilations failed");
            return Err(CompileSketchesError::CompilationFailed);
        }
        Ok(())
    }

    /// Finds the sketches according to [`Self::sketch_paths`].
    ///
    /// Uses recursive search of the provided paths,
    /// but ignores any directory that is hidden (those whose name starts with a dot).
    ///
    /// Skips any given path that does not exist and emits a warning.
    pub fn find_sketches(&self) -> Result<Vec<PathBuf>> {
        let mut found = HashSet::new();
        for p in &self.sketch_paths {
            if !p.exists() {
                log::warn!("Sketch path does not exist: {}", p.to_string_lossy());
                continue;
            }
            if p.is_file() {
                if path_is_sketch(p) {
                    found.insert(p.clone());
                }
                continue;
            }

            let mut collect_sketch_path = |f: &Path| {
                if path_is_sketch(f)
                    // arduino sketches, by convention, must be in a containing directory,
                    // so `Path::parent()` should be infallible if `path_is_sketch()` returns true.
                    && let Some(parent) = f.parent()
                {
                    found.insert(parent.to_path_buf());
                }
            };
            visit_dirs_recursive(p, &mut collect_sketch_path)?;
        }
        Ok(found.into_iter().collect())
    }

    /// Cleans up assets aggregated in [`CompileSketches::clean_up_paths`].
    fn clean_up_tmp_assets(&mut self) -> Result<()> {
        for path in &self.clean_up_paths {
            if path.exists() {
                if path.is_file() {
                    fs::remove_file(path)?;
                } else if path.is_dir() {
                    fs::remove_dir_all(path)?;
                }
            }
        }
        Ok(())
    }
}
