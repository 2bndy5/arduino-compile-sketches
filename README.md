# arduino-compile-sketches

This is a Rust port of the [original Github Action][og-action] (written in Python).
It is meant to be a drop-in replacement with a bunch of improvements.

## Why?

Development on the [original Github action][og-action] seems to have stalled,
aside from occasionally merging dependabot updates.

## Feature Parity

- Install specified dependencies (either `platform` or `libraries`) during setup.
- Compile sketches using a specified version of [arduino-cli].
- Aggregate compilation warnings, errors, and size deltas into JSON artifacts.
- Store JSON artifacts at the specified `sketches-report` path.

### Improvements

- No need for a `github.token`. Instead of using Github REST API, this uses the
  environment variables and event payload (where applicable).
- Compile sketches in parallel. This also applies to compilation of the project's
  base ref when triggered by a PR event and `enable-deltas-report` is `true`.
- Output sketch name on compilation failure.
- Show the compilation command used in the workflows' logs.
- Cache the installed [arduino-cli] for consecutive steps in the same job.
  This does not mean the cache is persisted between runs.
- Check (and log) the version of the installed [arduino-cli].
- Optionally prevent non-zero exit code when any sketch compilation fails.
- Generate JSON reports regardless of compilation success or failure.
- Proper log level filtering (where `verbose` enables debugging level), and
  optionally colored log level prefixes (respects
  [conventional env variables][colored-env-vars]).
- No Python runtime required. This action ships a compiled binary executable instead.

## Inputs

See CLI document for details about supported inputs.

## Example

```yaml
- uses: 2bndy5/arduino-compile-sketches@v1
  with:
    fqbn: "arduino:avr:uno"
    libraries: |
      - name: Servo
      - name: Stepper
        version: 1.1.3
```

[og-action]: https://github.com/arduino/compile-sketches
[arduino-cli]: https://github.com/arduino/arduino-cli
[colored-env-vars]: https://bixense.com/clicolors/
