use crate::error::{CompileSketchesError, Result};
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
            "clone", "--depth", "1", "--branch", base_ref, &repo_url, ".",
        ])
        .current_dir(tmp_path)
        .status()
    {
        if status.success() {
            return Ok(Some(BaseRefCheckout {
                base_ref: base_ref.to_string(),
                temp_dir: tmp,
            }));
        } else {
            log::warn!("Shallow clone of ref '{base_ref}' failed with status: {status}.");
        }
    }

    log::warn!("Falling back to full clone of base ref.");
    // Fall back to a full clone and an explicit fetch+checkout of the base ref.
    if let Ok(status) = Command::new("git")
        .args(["clone", &repo_url, "."])
        .current_dir(tmp_path)
        .status()
        && status.success()
    {
        let _ = Command::new("git")
            .current_dir(tmp_path)
            .args(["fetch", "origin", base_ref, "--depth", "1"])
            .status();
        if let Ok(co_status) = Command::new("git")
            .current_dir(tmp_path)
            .args(["checkout", base_ref])
            .status()
            && co_status.success()
        {
            return Ok(Some(BaseRefCheckout {
                base_ref: base_ref.to_string(),
                temp_dir: tmp,
            }));
        }
    }

    Ok(None)
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
        if self.arduino_cli_data_directory_path.exists() {
            cmd.env(
                "ARDUINO_DIRECTORIES_DATA",
                self.arduino_cli_data_directory_path
                    .to_string_lossy()
                    .to_string(),
            );
        }
        if self.arduino_cli_user_directory_path.exists() {
            cmd.env(
                "ARDUINO_DIRECTORIES_USER",
                self.arduino_cli_user_directory_path
                    .to_string_lossy()
                    .to_string(),
            );
        }
        Ok(cmd)
    }
}
