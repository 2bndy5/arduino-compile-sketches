#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
pub mod cli;
pub mod driver;
pub mod error;
pub mod logger;
pub mod serde_types;
pub mod utils;

pub use driver::CompileSketches;
pub use error::CompileSketchesError;
