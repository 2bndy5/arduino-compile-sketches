use arduino_compile_sketches::{driver::CompileSketches, error::Result, logger};

/// Run the application with the provided command-line arguments.
///
/// This is abstracted from main() to control what gets parsed as CLI args.
/// Otherwise, cargo and cargo-nextest args would be parsed as CLI args, which is causes errors.
async fn run(args: &[String]) -> Result<()> {
    // Initialize structured logging
    logger::init();
    let mut app = CompileSketches::from_cli(args)?;
    log::set_max_level(if app.sketch_compiler.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    });
    app.compile_sketches().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    run(&std::env::args().collect::<Vec<String>>()).await
}

#[cfg(test)]
mod tests {
    use arduino_compile_sketches::CompileSketchesError;

    use super::run;
    use std::{env, fs};

    async fn blank_sketch(verbose: bool) {
        // Setup a temporary workspace with a minimal sketch and a fake arduino-cli
        let tmp_dir = tempfile::TempDir::with_prefix("arduino-compile-sketches-bin-tests").unwrap();
        let ws = tmp_dir.path().to_path_buf();

        if !verbose {
            // Create a minimal sketch
            let sketch_dir = ws.join("example_sketch");
            fs::create_dir_all(&sketch_dir).unwrap();
            let sketch = String::from("void setup() {}\nvoid loop() {}\n");
            fs::write(sketch_dir.join("example_sketch.ino"), &sketch).unwrap();
        }

        unsafe {
            env::set_var("GITHUB_REPOSITORY", "2bndy5/arduino-compile-sketches");
            env::set_var("GITHUB_SHA", "head-ref");
            env::set_var("GITHUB_EVENT_NAME", "push");
            env::set_var("GITHUB_WORKSPACE", ws.to_str().unwrap());
            env::set_var("INPUT_CLI-VERSION", "latest");
            env::set_var("INPUT_FQBN", "arduino:avr:uno");
            env::set_var("INPUT_PLATFORMS", "");
            env::set_var("INPUT_LIBRARIES", "");
            env::set_var("INPUT_SKETCH-PATHS", ws.to_str().unwrap());
            env::set_var(
                "INPUT_SKETCHES-REPORT-PATH",
                ws.join("reports").to_str().unwrap(),
            );
            // avoids race condition about reusing cache
            env::set_var("INPUT_ENABLE-WARNINGS-REPORT", "true");
            env::set_var("INPUT_VERBOSE", verbose.to_string());
        }

        // Run a single compile to exercise the fake CLI shim
        let result = run(&[]).await;
        if !verbose {
            assert!(
                result.is_ok(),
                "Expected compilation to succeed, got error: {result:?}"
            );
        } else {
            assert!(
                result
                    .as_ref()
                    .is_err_and(|e| matches!(e, CompileSketchesError::NoSketchesFound)),
                "Expected NoSketchesFound error. Got: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn verbose_blank_sketch() {
        blank_sketch(true).await;
    }

    #[tokio::test]
    async fn basic_blank_sketch() {
        blank_sketch(false).await;
    }
}
