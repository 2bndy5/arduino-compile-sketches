use anyhow::Result;
use arduino_compile_sketches::{driver::CompileSketches, logger};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging
    logger::init();
    let mut app = CompileSketches::new_from_env()?;
    log::set_max_level(if app.verbose {
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

    #[test]
    fn blank_sketch() {
        // Setup a temporary workspace with a minimal sketch and a fake arduino-cli
        let td = tempfile::tempdir().unwrap();
        let ws = td.path().to_path_buf();

        // Create a minimal sketch
        let sketch_dir = ws.join("example_sketch");
        fs::create_dir_all(&sketch_dir).unwrap();
        fs::write(
            sketch_dir.join("example_sketch.ino"),
            "void setup() {}\nvoid loop() {}\n",
        )
        .unwrap();

        unsafe {
            env::set_var("GITHUB_REPOSITORY", "2bndy5/arduino-compile-sketches");
            env::set_var("GITHUB_SHA", "head-ref");
            env::set_var("GITHUB_EVENT_NAME", "push");
            env::set_var("GITHUB_WORKSPACE", ws.to_str().unwrap());
            env::set_var("INPUT_CLI-VERSION", "latest");
            env::set_var("INPUT_FQBN", "arduino:avr:uno");
            env::set_var("INPUT_PLATFORMS", "");
            env::set_var("INPUT_LIBRARIES", "");
            env::set_var("INPUT_SKETCH-PATHS", sketch_dir.to_str().unwrap());
            env::set_var(
                "INPUT_SKETCHES-REPORT-PATH",
                ws.join("reports").to_str().unwrap(),
            );
            env::set_var("INPUT_ENABLE-WARNINGS-REPORT", "true");
            env::set_var("INPUT_ENABLE-DELTAS-REPORT", "true");
            env::set_var("INPUT_VERBOSE", "true");
        }

        // Run a single compile to exercise the fake CLI shim
        main().unwrap();
    }
}
