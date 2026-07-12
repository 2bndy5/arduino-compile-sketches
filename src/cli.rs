#![cfg(feature = "bin")]
use std::path::PathBuf;

use clap::builder::{BoolishValueParser, FalseyValueParser};

#[allow(
    missing_docs,
    reason = "do not override description (which clap) generated from package.description in Cargo.toml"
)]
#[derive(clap::Parser, Debug)]
#[command(name = "arduino-compile-sketches", version, about, long_about)]
pub struct CliArgs {
    /// The desired version of arduino-cli to use.
    ///
    /// "latest" will use the most recent release,
    /// which includes release candidates.
    #[arg(env = "INPUT_CLI-VERSION", short = 'c', long, default_value = "latest")]
    pub cli_version: String,

    /// The fully qualified board name to target for compilation.
    #[arg(
        env = "INPUT_FQBN",
        short = 'b',
        long,
        default_value = "arduino:avr:uno"
    )]
    pub fqbn: String,

    /// A YAML list of platforms to install before compilation.
    #[arg(env = "INPUT_PLATFORMS", short = 'p', long, default_value = "")]
    pub platforms: String,

    /// A YAML list of libraries to install before compilation.
    #[arg(env = "INPUT_LIBRARIES", short = 'l', long, default_value = "")]
    pub libraries: String,

    /// A YAML list of paths to sketches to compile.
    ///
    /// Does not support globs.
    #[arg(env = "INPUT_SKETCH-PATHS", short = 's', long, default_value = "./")]
    pub sketch_paths: String,

    /// Additional (space-separated) flags to pass to arduino-cli during compilation.
    #[arg(env = "INPUT_CLI-COMPILE-FLAGS", short = 'f', long)]
    pub cli_compile_flags: Option<String>,

    /// Enable verbose logging.
    #[arg(
        env = "INPUT_VERBOSE",
        short = 'v',
        long,
        default_value = "false",
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = BoolishValueParser::new(),
    )]
    pub verbose: bool,

    /// Exit with non-zero code if any compilation fails
    ///
    /// JSON reports are generated regardless, but
    /// setting this to false allows CI workflows to
    /// continue and process the reports.
    ///
    /// Any compilation error of base ref in
    /// pull request events does not exit with
    /// non-zero code, regardless of this setting.
    #[arg(
        env = "INPUT_FAIL-ON-COMPILE-ERROR",
        short = 'e',
        long,
        default_value = "true",
        default_missing_value = "false",
        num_args = 0..=1,
        value_parser = FalseyValueParser::new(),
    )]
    pub fail_on_compile_error: bool,

    /// Enable reports about change in compiled size.
    ///
    /// Only applies to pull request events.
    #[arg(
        env = "INPUT_ENABLE-DELTAS-REPORT",
        short = 'd',
        long,
        default_value = "false",
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = BoolishValueParser::new(),
    )]
    pub enable_deltas_report: bool,

    /// Enable reports about compilation warnings.
    ///
    /// Requires arduino-cli v0.14.0-rc.1 or later.
    #[arg(
        env = "INPUT_ENABLE-WARNINGS-REPORT",
        short = 'w',
        long,
        default_value = "false",
        default_missing_value = "true",
        num_args = 0..=1,
        value_parser = BoolishValueParser::new(),
    )]
    pub enable_warnings_report: bool,

    /// The destination path to save reports (JSON files).
    ///
    /// Used regardless of compilation failures.
    #[arg(
        env = "INPUT_SKETCHES-REPORT-PATH",
        short = 'r',
        long,
        default_value = "reports"
    )]
    pub sketches_report_path: PathBuf,
}
