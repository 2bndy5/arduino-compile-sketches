use tokio::task::JoinSet;

use crate::{
    CompileSketches,
    error::{CompileSketchesError, Result},
    report::{get_sizes_from_output, get_warning_count_from_output},
    utils::fmt_duration,
};
use arduino_report_size_deltas::report_structs::{Sketch, SketchSize, SketchSizeKind};

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(super) enum CompileRef {
    Head,
    Base,
}

pub(super) struct CompileTaskEnvelope {
    pub(super) compile_ref: CompileRef,
    pub(super) result: CompilationTaskResult,
}

#[derive(Debug)]
pub(super) enum CompilationTaskResult {
    Ok {
        relative_sketch_path: String,
        output: String,
        success: bool,
        invoked_cmd: String,
        duration: Duration,
    },
    Err {
        relative_sketch_path: String,
        error: CompileSketchesError,
        duration: Duration,
    },
}

/// A struct to distribute commonly used resources during parallel compilation.
#[derive(Debug, Clone, Default)]
pub struct SketchCompiler {
    /// The fully qualified board name (FQBN) for which to compile.
    pub fqbn: String,

    /// The user-specified extra arguments passed to `arduino-cli compile` command.
    pub cli_compile_flags: Vec<String>,

    /// The path to the Arduino CLI executable.
    pub arduino_cli_path: Option<PathBuf>,

    /// The path to the Arduino CLI user directory.
    pub arduino_cli_user_directory_path: PathBuf,

    /// The path to the Arduino CLI data directory.
    pub arduino_cli_data_directory_path: PathBuf,

    /// Whether to enable warnings report.
    ///
    /// When enabled, requires arduino-cli v0.14.0-rc.1 or later.
    pub enable_warnings_report: bool,

    /// Whether to enable verbose output.
    ///
    /// When enabled, this
    ///
    /// - emits the `arduino-cli compile` command output to the console.
    /// - passes the `--verbose` flag to `arduino-cli compile` command.
    /// - enables [`log::debug!()`] level logs.
    pub verbose: bool,
}

pub(super) struct CompilationResult {
    pub(super) output: String,
    pub(super) success: bool,
    pub(super) invoked_cmd: String,
}

pub(super) struct BaseRefCheckout {
    pub base_ref: String,
    pub temp_dir: tempfile::TempDir,
}

pub(super) fn checkout_base_ref(base_ref: &str, repo: &str) -> Result<Option<BaseRefCheckout>> {
    let repo_url = format!("https://github.com/{repo}.git");

    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path();
    // Try a shallow clone of the specific ref into the temp dir.
    if let Ok(status) = Command::new("git")
        .args([
            "-c",
            "advice.detachedHead=false",
            "clone",
            "--recurse-submodules",
            "--shallow-submodules",
            "--depth",
            "1",
            "--revision",
            base_ref,
            &repo_url,
            ".",
        ])
        .current_dir(tmp_path)
        .status()
        && status.success()
    {
        Ok(Some(BaseRefCheckout {
            base_ref: base_ref.to_string(),
            temp_dir: tmp,
        }))
    } else {
        log::warn!("Shallow clone of ref '{base_ref}' failed.");
        Ok(None)
    }
}

pub(super) fn compile_sketch_task(
    compiler: SketchCompiler,
    sketch: PathBuf,
    relative_sketch_path: String,
) -> CompilationTaskResult {
    let instant = Instant::now();
    let result = compiler.compile_sketch(&sketch);
    let elapsed = instant.elapsed();
    match result {
        Ok(result) => CompilationTaskResult::Ok {
            relative_sketch_path,
            output: result.output,
            success: result.success,
            invoked_cmd: result.invoked_cmd,
            duration: elapsed,
        },
        Err(error) => CompilationTaskResult::Err {
            relative_sketch_path,
            error,
            duration: elapsed,
        },
    }
}

impl SketchCompiler {
    pub(super) fn compile_sketch(&self, sketch_path: &Path) -> Result<CompilationResult> {
        let mut cmd = self.build_cli_command(&["compile", "--fqbn", &self.fqbn])?;
        cmd.arg(sketch_path);
        if !self.cli_compile_flags.is_empty() {
            for f in &self.cli_compile_flags {
                cmd.arg(f);
            }
        }
        if self.verbose {
            cmd.arg("--verbose");
        }
        if self.enable_warnings_report {
            // `--clean` requires arduino-cli v0.14.0-rc.1
            // `--clean` is used so that reusing the build cache does not hide any old warnings
            cmd.args(["--clean", "--warnings", "all"]);
        }
        // NOTE: I believe any user-provided `--warnings` options will override the above
        // `--warnings all` passed when `enable_warnings_report` is enabled.
        // This could be used as a filter for the counted warnings, if desirable.
        let invoked_command = format!(
            "{} {}",
            cmd.get_program().to_string_lossy(),
            cmd.get_args()
                .map(|arg| arg.to_string_lossy().to_string())
                .collect::<Vec<String>>()
                .join(" ")
        );

        let output = cmd.output()?;
        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&output.stdout));
        combined.push_str(&String::from_utf8_lossy(&output.stderr));

        Ok(CompilationResult {
            output: combined,
            success: output.status.success(),
            invoked_cmd: invoked_command,
        })
    }

    pub(crate) fn build_cli_command(&self, args: &[&str]) -> Result<Command> {
        let mut cmd = match &self.arduino_cli_path {
            Some(cli_path) => {
                let mut c = Command::new(cli_path);
                c.args(args);
                c
            }
            None => {
                return Err(CompileSketchesError::ArduinoCliNotFound);
            }
        };
        cmd.env(
            "ARDUINO_DIRECTORIES_DATA",
            self.arduino_cli_data_directory_path
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "ARDUINO_DIRECTORIES_USER",
            self.arduino_cli_user_directory_path
                .to_string_lossy()
                .to_string(),
        );
        Ok(cmd)
    }
}

impl CompileSketches {
    /// Join the given `compile_jobs` and extract data from stdout for each sketch.
    ///
    /// Returns a tuple of:
    /// - `Vec<Sketch>`: the compilation reports for sketches compiled from the head ref
    /// - `Vec<Sketch>`: the compilation reports for sketches compiled from the base ref
    /// - `bool`: summary success of head ref compilations (not base ref compilations)
    pub(super) async fn join_tasks(
        &self,
        mut compile_jobs: JoinSet<CompileTaskEnvelope>,
        base_ref_checkout: Option<BaseRefCheckout>,
        sketch_count: usize,
    ) -> Result<(Vec<Sketch>, Vec<Sketch>, bool)> {
        let mut sketch_reports = Vec::with_capacity(sketch_count);
        let mut base_sketch_reports = Vec::with_capacity(if base_ref_checkout.is_some() {
            sketch_count
        } else {
            0
        });
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

            match task_result {
                CompileTaskEnvelope {
                    compile_ref,
                    result:
                        CompilationTaskResult::Ok {
                            relative_sketch_path,
                            output,
                            success,
                            invoked_cmd,
                            duration,
                        },
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
                        if matches!(compile_ref, CompileRef::Head) {
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
                        vec![
                            SketchSizeKind::Ram {
                                size: SketchSize::default(),
                            },
                            SketchSizeKind::Flash {
                                size: SketchSize::default(),
                            },
                        ]
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
                    if matches!(compile_ref, CompileRef::Base) {
                        base_sketch_reports.push(sketch);
                    } else {
                        sketch_reports.push(sketch);
                    }
                }
                CompileTaskEnvelope {
                    compile_ref,
                    result:
                        CompilationTaskResult::Err {
                            relative_sketch_path,
                            error,
                            duration,
                        },
                } => {
                    // if task failed to execute (I/O problems): just log it and move on.
                    log::info!(
                        target: "CI_LOG_CMD",
                        "::group::Compilation task failed for {relative_sketch_path}{base_ref_str}"
                    );
                    log::error!(target: "CI_LOG_CMD", "::error::{error}");
                    if matches!(compile_ref, CompileRef::Head) {
                        // overall compilation failure is not affected by any base ref compilations
                        all_compilations_successful = false;
                    }
                    log::info!(target: "CI_LOG_CMD", "::endgroup::");
                    log::info!("Compilation time elapsed: {}", fmt_duration(&duration));
                }
            }
        }
        Ok((
            sketch_reports,
            base_sketch_reports,
            all_compilations_successful,
        ))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    #[test]
    fn fail_compile_task_exec() {
        let compiler = SketchCompiler::default();
        let sketch_path = PathBuf::from("sketches");
        let sketch = sketch_path.join("test_sketch.ino");
        let CompilationTaskResult::Err {
            relative_sketch_path: _,
            error,
            duration: _,
        } = compile_sketch_task(compiler, sketch, sketch_path.to_string_lossy().to_string())
        else {
            panic!("Expected error when Arduino CLI path is not set");
        };
        assert!(matches!(error, CompileSketchesError::ArduinoCliNotFound));
    }

    #[tokio::test]
    async fn fail_compile_task_join() {
        #[cfg(feature = "bin")]
        crate::logger::init();

        let compiler = SketchCompiler::default();
        let driver = CompileSketches {
            sketch_compiler: compiler.clone(),
            ..Default::default()
        };

        let sketch_path = PathBuf::from("sketches");
        let sketch = sketch_path.join("test_sketch.ino");

        let mut compile_jobs = JoinSet::new();
        compile_jobs.spawn_blocking(move || CompileTaskEnvelope {
            compile_ref: CompileRef::Head,
            result: compile_sketch_task(
                compiler,
                sketch,
                sketch_path.to_string_lossy().to_string(),
            ),
        });

        let (sketches, base_sketches, success) =
            driver.join_tasks(compile_jobs, None, 1).await.unwrap();
        assert!(!success, "overall compilation reported as successful");
        assert!(sketches.is_empty(), "sketch reports should be empty");
        assert!(
            base_sketches.is_empty(),
            "base sketch reports should be empty"
        );
    }

    #[test]
    fn fail_checkout_base_ref() {
        #[cfg(feature = "bin")]
        crate::logger::init();

        let result = checkout_base_ref("bogus-ref", "bogus-repo").unwrap();
        assert!(
            result.is_none(),
            "Expected `None` when checkout of base ref fails"
        );
    }
}
