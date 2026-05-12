use arduino_report_size_deltas::report_structs::Report;

/// Error type that is returned when something went wrong.
#[derive(Debug, thiserror::Error)]
pub enum CompileSketchesError {
    /// Propagates errors from the [`reqwest`] crate.
    #[error(transparent)]
    Request(#[from] reqwest::Error),

    /// Propagates errors from the [`std::io`] module.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Propagates errors from the [`serde_json`] crate.
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    /// Propagates errors from the [`url`] crate.
    #[error("Failed to parse URL")]
    UrlParse(#[from] url::ParseError),

    /// Propagates errors from the [`regex`] crate.
    #[error(transparent)]
    Regex(#[from] regex::Error),

    /// Propagates errors from the [`zip`] crate.
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),

    /// An error that occurs when parallel tasks fail to complete successfully.
    #[error("Parallel task failed to complete successfully")]
    TokioJoin(#[from] tokio::task::JoinError),

    /// An error that occurs when reading from an environment variable fails.
    #[error("Failed to read from env var: {0}")]
    EnvVar(&'static str, #[source] std::env::VarError),

    /// An error that occurs when a Git reference is not found for a repository.
    #[error("Failed to find {0} ref for repository")]
    UnknownGitRef(&'static str),

    /// An error that occurs when decoding a YAML list of dependencies fails.
    #[error("Failed to decode YAML list of {dep_type}: {input}")]
    DecodeYamlDepList {
        /// The type of dependency (library or platform) being decoded.
        dep_type: &'static str,
        /// The input string that could not be decoded.
        input: String,
        /// The underlying error that occurred during decoding.
        #[source]
        source: Box<serde_saphyr::Error>,
    },

    /// An error that occurs when no sketches are found for the provided paths.
    #[error("No sketches found for provided paths")]
    NoSketchesFound,

    /// An error that occurs when parsing a YAML map of dependencies fails.
    #[error("Failed to parse dependency mapping")]
    ParseDependencyMapping,

    /// An error that occurs when an `arduino-cli` command fails (emits a non-zero exit code).
    #[error("arduino-cli command failed: {output}")]
    ArduinoCliCommandFailed {
        /// The output of the `arduino-cli` command that failed.
        output: String,
    },

    /// An error that occurs when the `arduino-cli` binary is not found in the cache.
    #[error("arduino-cli binary not found in cache")]
    ArduinoCliNotFound,

    /// An error that occurs when invoking an `arduino-cli` command fails.
    ///
    /// This is different from [`Self::ArduinoCliCommandFailed`],
    /// which occurs when an `arduino-cli` command emits a non-zero exit code.
    /// This error is more akin to a shell-level error (e.g. a command not found or permission denied).
    #[error("Failed to invoke `arduino-cli {command}`")]
    InvokeArduinoCli {
        /// The command that failed to execute.
        command: String,
        /// The error that occurred while trying to execute the command.
        #[source]
        error: std::io::Error,
    },

    /// An error that occurs when one or more compilations fail.
    ///
    /// Can be avoided by setting
    /// [`CompileSketches::fail_on_compile_error`](crate::CompileSketches::fail_on_compile_error)
    /// to `false`.
    #[error("one or more compilations failed")]
    CompilationFailed,

    /// An error that occurs when a git command fails.
    #[error("git command failed while trying to {task}")]
    GitCommandFailed {
        /// The task that failed to execute.
        task: &'static str,
    },

    /// An error that occurs when a git command fails due to an I/O error.
    #[error("Failed to run git command while trying to {task}")]
    GitCommandIo {
        /// The task that failed to execute.
        task: &'static str,
        /// The I/O error that occurred.
        #[source]
        source: std::io::Error,
    },

    /// An error that occurs when the installed platforms JSON cannot be parsed.
    #[error("Failed to parse installed platforms JSON")]
    ParseInstalledPlatformsJson {
        /// The JSON error that occurred.
        #[source]
        source: serde_json::Error,
    },

    /// An error that occurs when a required platform field is missing.
    #[error("Missing required platform field `{key}` for `{id}`")]
    PlatformDepMissingField {
        /// The key of the missing field.
        key: &'static str,
        /// The ID of the platform that is missing the field.
        id: String,
    },

    /// An error that occurs when a path cannot be resolved.
    #[error("Failed to resolve the absolute {path_for} path {path}")]
    ResolvePath {
        /// The type of path being resolved.
        ///
        /// Can be empty or an additional description of the path (e.g. "library" or "platform").
        path_for: &'static str,
        /// The path being resolved.
        path: String,
        /// The error that occurred while resolving the path.
        #[source]
        source: std::io::Error,
    },

    /// An error that occurs when the working directory cannot be detected.
    ///
    /// Should only happen if file permission is inadequate.
    #[error("Failed to detect the path to the working directory")]
    DetectWorkingDirectory {
        /// The error that occurred while detecting the working directory.
        #[source]
        source: std::io::Error,
    },

    /// An error that occurs when the destination path for the install directory is malformed.
    ///
    /// Should only happen if the path ends in a relative component (e.g. `..` or `.`).
    #[error("Destination path for install directory is malformed: {0}")]
    MalformedInstallDestPathName(String),

    /// An error that occurs when failing to purge an existing destination path for a dependency install.
    #[error("Failed to delete existing installed path {path}")]
    DeleteExistingInstalledPath {
        /// The destination path.
        path: String,
        /// The underlying I/O error that caused the failure.
        #[source]
        source: std::io::Error,
    },

    /// An error that occurs when the installation path already exists.
    #[error("Installation path already exists at {path}.")]
    InstallPathAlreadyExists {
        /// The path that already exists.
        path: String,
    },

    /// An error that occurs when failing to create a symlink.
    #[error("Failed to create symlink from {source_path} to {destination_path}")]
    CreateSymlink {
        /// The source path of the symlink.
        source_path: String,
        /// The destination path of the symlink.
        destination_path: String,
        /// The underlying I/O error that caused the failure.
        #[source]
        source: std::io::Error,
    },

    /// An error that occurs when failing to perform an operation with a temporary path.
    #[error("Failed temp file/folder operation while trying to {task}")]
    TempPathIo {
        /// The task that failed to complete.
        task: &'static str,
        /// The underlying I/O error that caused the failure.
        #[source]
        source: std::io::Error,
    },

    /// An error that occurs when failing to perform an operation with an archive.
    #[error("Failed archive extraction operation while trying to {task}")]
    ArchiveExtractionIo {
        /// The task that failed to complete.
        task: &'static str,
        /// The underlying I/O error that caused the failure.
        #[source]
        source: std::io::Error,
    },

    /// An error that occurs when the archive format is unsupported.
    #[error("Unsupported Archive format: '{0}'")]
    ArchiveFormatUnsupported(String),

    /// An error that occurs when a generated report does not have enough data to form usable feedback.
    /// 
    /// This happens when [`Report::is_valid()`] returns `false`.
    #[error("The generated report did not have enough data to form usable feedback: {0:#?}")]
    IncompleteReport(Report),
}

/// A convenient alias for `Result<T, CompileSketchesError>`.
pub type Result<T> = std::result::Result<T, CompileSketchesError>;
