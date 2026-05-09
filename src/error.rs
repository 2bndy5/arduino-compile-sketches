/// Error type that is returned when something went wrong.
#[derive(Debug, thiserror::Error)]
pub enum CompileSketchesError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error("Failed to parse URL")]
    UrlParse(#[from] url::ParseError),

    #[error(transparent)]
    Regex(#[from] regex::Error),

    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),

    #[error("Parallel task failed to complete successfully")]
    TokioJoin(#[from] tokio::task::JoinError),

    #[error("Failed to read from env var: {0}")]
    EnvVar(&'static str, #[source] std::env::VarError),

    #[error("Failed to find {0} ref for repository")]
    UnknownGitRef(&'static str),

    #[error("Failed to decode YAML list of {dep_type}: {input}")]
    DecodeYamlDepList {
        dep_type: &'static str,
        input: String,
        #[source]
        source: Box<serde_saphyr::Error>,
    },

    #[error("No sketches found for provided paths")]
    NoSketchesFound,

    #[error("Failed to parse dependency mapping")]
    ParseDependencyMapping,

    #[error("arduino-cli command failed: {output}")]
    ArduinoCliCommandFailed { output: String },

    #[error("arduino-cli binary not found in cache")]
    ArduinoCliNotFound,

    #[error("Failed to invoke `arduino-cli {command}`")]
    InvokeArduinoCli {
        command: String,
        #[source]
        error: std::io::Error,
    },

    #[error("one or more compilations failed")]
    CompilationFailed,

    #[error("git command failed while trying to {task}")]
    GitCommandFailed { task: &'static str },

    #[error("Failed to run git command while trying to {task}")]
    GitCommandIo {
        task: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse installed platforms JSON")]
    ParseInstalledPlatformsJson {
        #[source]
        source: serde_json::Error,
    },

    #[error("Missing required platform field `{key}` for `{id}`")]
    PlatformDepMissingField { key: &'static str, id: String },

    #[error("Failed to resolve the absolute {path_for} path {path}")]
    ResolvePath {
        path_for: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to detect the path to the working directory")]
    DetectWorkingDirectory {
        #[source]
        source: std::io::Error,
    },

    #[error("Destination path for install directory is malformed: {0}")]
    MalformedInstallDestPathName(String),

    #[error("Failed to delete existing installed path {path}")]
    DeleteExistingInstalledPath {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Installation path already exists at {path}.")]
    InstallPathAlreadyExists { path: String },

    #[error("Failed to create symlink from {source_path} to {destination_path}")]
    CreateSymlink {
        source_path: String,
        destination_path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed temp file/folder operation while trying to {task}")]
    TempPathIo {
        task: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed archive extraction operation while trying to {task}")]
    ArchiveExtractionIo {
        task: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("Unsupported Archive format: '{0}'")]
    ArchiveFormatUnsupported(String),
}

/// A convenient alias for `Result<T, CompileSketchesError>`.
pub type Result<T> = std::result::Result<T, CompileSketchesError>;
