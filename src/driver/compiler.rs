use crate::error::{CompileSketchesError, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub(super) enum CompileRef {
    Head,
    Base(String),
}

pub(super) struct CompileTaskEnvelope {
    pub(super) compile_ref: CompileRef,
    pub(super) result: CompilationTaskResult,
}

#[derive(Debug)]
pub(super) struct CompilationTaskResult {
    pub(super) relative_sketch_path: String,
    pub(super) output: String,
    pub(super) success: bool,
    pub(super) invoked_cmd: String,
    pub(super) duration: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct SketchCompiler {
    pub fqbn: String,
    pub cli_compile_flags: Vec<String>,
    pub arduino_cli_path: Option<PathBuf>,
    pub arduino_cli_user_directory_path: PathBuf,
    pub arduino_cli_data_directory_path: PathBuf,
}

pub(super) struct CompilationResult {
    pub(super) output: String,
    pub(super) success: bool,
    pub(super) invoked_cmd: String,
}

pub(super) fn checkout_base_ref(
    base_ref: &str,
    repo: &str,
) -> Result<Option<(tempfile::TempDir, PathBuf)>> {
    let repo_url = format!("https://github.com/{repo}.git");

    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path().to_path_buf();
    let tmp_path_string = tmp_path.to_string_lossy().to_string();

    let clone_status = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            base_ref,
            &repo_url,
            tmp_path_string.as_str(),
        ])
        .status();

    let cloned = match clone_status {
        Ok(status) if status.success() => true,
        _ => {
            let clone_status = Command::new("git")
                .args(["clone", &repo_url, tmp_path_string.as_str()])
                .status()
                .map_err(|source| CompileSketchesError::GitCommandIo {
                    task: "clone repository",
                    source,
                });
            match clone_status {
                Ok(status) if status.success() => {
                    let checkout_status = Command::new("git")
                        .current_dir(&tmp_path)
                        .args(["checkout", base_ref])
                        .status()
                        .map_err(|source| CompileSketchesError::GitCommandIo {
                            task: "checkout base ref",
                            source,
                        });
                    matches!(checkout_status, Ok(status) if status.success())
                }
                _ => false,
            }
        }
    };

    if cloned {
        Ok(Some((tmp, tmp_path)))
    } else {
        Ok(None)
    }
}

pub(super) fn compile_sketch_task(
    compiler: SketchCompiler,
    sketch: PathBuf,
    relative_sketch_path: String,
) -> Result<CompilationTaskResult> {
    let instant = Instant::now();
    let result = compiler.compile_sketch(&sketch)?;

    Ok(CompilationTaskResult {
        relative_sketch_path,
        output: result.output,
        success: result.success,
        invoked_cmd: result.invoked_cmd,
        duration: instant.elapsed(),
    })
}

impl SketchCompiler {
    pub(super) fn compile_sketch(&self, sketch_path: &Path) -> Result<CompilationResult> {
        let mut cmd =
            self.build_cli_command(&["compile", "--warnings", "all", "--fqbn", &self.fqbn])?;
        cmd.arg(sketch_path);
        if !self.cli_compile_flags.is_empty() {
            for f in &self.cli_compile_flags {
                cmd.arg(f);
            }
        }
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
