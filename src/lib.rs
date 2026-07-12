//! This library provides the logic and algorithms that drives the arduino-compile-sketches binary.
//!
//! It is a port of the [arduino-compile-sketches](https://github.com/arduino/arduino-compile-sketches) project.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

/// A module for parsing CLI arguments.
pub mod cli;
/// A module containing behavior that drives sketch compilation and dependency installation.
pub mod driver;
/// A module containing error types.
pub mod error;
/// A module containing logging functionality.
pub mod logger;
/// A module containing serde types.
pub mod serde_types;
/// A module containing utility functions.
pub mod utils;

mod report;

pub use driver::CompileSketches;
pub use error::CompileSketchesError;
