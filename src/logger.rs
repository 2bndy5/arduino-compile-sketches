#![cfg(feature = "bin")]

use colored::Colorize;

use std::io::{Write, stdout};

use log::{Level, Metadata, Record};

/// A logger that writes log records to stdout.
struct Logger;

impl Logger {
    fn level_color(level: &Level) -> String {
        let name = format!("{:>5}", level.as_str().to_uppercase());
        match level {
            Level::Error => name.red().bold().to_string(),
            Level::Warn => name.yellow().bold().to_string(),
            Level::Info => name.green().bold().to_string(),
            Level::Debug => name.blue().bold().to_string(),
            Level::Trace => name.magenta().bold().to_string(),
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    #[allow(
        clippy::expect_used,
        reason = "panics if fails to write/flush to stdout"
    )]
    fn log(&self, record: &Record) {
        let mut stdout = stdout().lock();
        if record.target() == "CI_LOG_CMD" {
            // this log is meant to manipulate a CI workflow's log grouping
            writeln!(stdout, "{}", record.args()).expect("Failed to write log command to stdout");
            stdout
                .flush()
                .expect("Failed to flush log command to stdout");
        } else if self.enabled(record.metadata()) {
            let module = record.module_path();
            if module.is_none_or(|v| {
                v.starts_with("arduino_compile_sketches") || v.starts_with("compile_sketches")
            }) {
                writeln!(
                    stdout,
                    "[{}]: {}",
                    Self::level_color(&record.level()),
                    record.args()
                )
                .expect("Failed to write log message to stdout");
            } else if let Some(module) = module {
                writeln!(
                    stdout,
                    "[{}]{{{module}:{}}}: {}",
                    Self::level_color(&record.level()),
                    record.line().unwrap_or_default(),
                    record.args()
                )
                .expect("Failed to write detailed log message to stdout");
            }
            stdout
                .flush()
                .expect("Failed to flush log message to stdout");
        }
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger;

/// Initializes the logger to write log records to stdout.
///
/// Errors are ignored. They are only emitted when the logger already has been set up.
pub fn init() {
    let _ = log::set_logger(&LOGGER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_coverage() {
        init();
        log::set_max_level(log::LevelFilter::Trace);
        log::error!("Some error message");
        log::warn!("Some warning message");
        log::trace!("Some trace message");
        log::logger().flush();
    }
}
