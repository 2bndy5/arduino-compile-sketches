use anyhow::Result;
use arduino_compile_sketches::{driver::CompileSketches, logger};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging
    logger::init();
    let mut app = CompileSketches::new_from_env()?;
    log::set_max_level(if app.sketch_compiler.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    });
    app.compile_sketches().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::main;
    use std::{env, fs};

    fn blank_sketch(verbose: bool) {
        // Setup a temporary workspace with a minimal sketch and a fake arduino-cli
        let tmp_dir = tempfile::TempDir::with_prefix("arduino-compile-sketches-bin-tests").unwrap();
        let ws = tmp_dir.path().to_path_buf();

        // Create a minimal sketch
        let sketch_dir = ws.join("example_sketch");
        fs::create_dir_all(&sketch_dir).unwrap();
        let mut sketch = String::from("void setup() {}\nvoid loop() {}\n");
        if verbose {
            // use this to differentiate caches created by the arduino-cli.
            sketch.push_str("void verbose() {}");
        }
        fs::write(sketch_dir.join("example_sketch.ino"), &sketch).unwrap();

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
            // avoids race condition to clean cache
            env::set_var("INPUT_ENABLE-WARNINGS-REPORT", "true");
            env::set_var("INPUT_VERBOSE", verbose.to_string());
        }

        // Run a single compile to exercise the fake CLI shim
        main().unwrap();
    }

    #[test]
    fn verbose_blank_sketch() {
        blank_sketch(true);
    }

    #[test]
    fn basic_blank_sketch() {
        blank_sketch(false);
    }
}
