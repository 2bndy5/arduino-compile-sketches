# AI AGENT — Rules and Constraints

## Testing

- Use `cargo nextest run` to run tests locally.
- Do not run `cargo test` directly; the project uses `cargo-nextest` for
  test execution to take advantage of parallelism and improved isolation
  (because the most tests set values to env vars).
- For integration tests, the following env var must be set to avoid `clap`
  parsing CLI args passed to cargo-nextest:

  ```sh
  ARDUINO_COMPILE_SKETCHES="INTEGRATION TESTS SKIP CLI ARGS"
  ```

- Place nextest configuration at `.config/nextest.toml` in the repository root.
- Run tests after edits; fix only errors that are directly caused by your
  changes.

## Code style

- use `cargo clippy` to check/fix for lint violations.
- use `cargo fmt` to ensure consistent code formatting.
