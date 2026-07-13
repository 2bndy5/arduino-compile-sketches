
<!-- ANCHOR: INTRO -->

# arduino-compile-sketches

[![Rust CI][rust-ci-badge]][rust-ci-link]
[![codecov][codecov-badge]][codecov-link]
[![Crates.io Version][crates.io-badge]][crates.io-link]
[![docs.rs][docs.rs-badge]][docs.rs-link]
[![action docs][action-docs-badge]][gh-pages]

This is a Rust port of the [original Github Action][og-action] (written in
Python). It is meant to be a drop-in replacement with a bunch of improvements.

## Why?

Development on the [original Github action][og-action] seems to have stalled,
aside from occasionally merging dependabot updates.

## Feature Parity

- Install specified dependencies (either [`platforms`][platforms-input] or
  [`libraries`][libraries-input]) during setup.
- Compile sketches using a specified [`cli-version`][cli-version-input] of [arduino-cli].
- Aggregate compilation [warnings, errors][enable-warnings-report-input], and
  [size deltas][enable-deltas-report-input] into JSON artifacts.
- Store JSON artifacts at the specified [`sketches-report-path`][sketches-report-path-input].

### Improvements

- No need for a `github.token`. Instead of using Github REST API, this uses the
  environment variables and event payload (where applicable).
- Compile sketches in parallel. This also applies to compilation of the
  project's base ref when triggered by a PR event and
  [`enable-deltas-report`][enable-deltas-report-input] is `true`.
  Parallel compilation of sketches' HEAD ref is done separately from
  compilation of sketches' base ref. Meaning, the same repository path is used for
  both batches of parallel jobs, where the base ref is fetched by `git checkout`.
- Output sketch name on compilation failure.
- Show the compilation command used in the workflows' logs.
- Cache the installed [arduino-cli] for consecutive steps in the same job. This
  does not mean the cache is persisted between runs.
- Check (and log) the version of the installed [arduino-cli].
- Optionally prevent non-zero exit code when any sketch compilation fails; see
  the new [`fail-on-compile-error`][fail-on-compile-error-input] input.
- Generate JSON reports regardless of compilation success or failure.
- Proper log level filtering (where [`verbose`][verbose-input] enables debugging
  level), and optionally colored log level prefixes (respects
  [conventional env variables][colored-env-vars]).
- No Python runtime required. This action ships a compiled binary executable
  instead.

<!-- ANCHOR: CLI_CAVEATS -->

> [!CAUTION]
> When [`enable-warnings-report`][enable-warnings-report-input] is enabled,
> the `--clean` flag is passed to `arduino-cli compile`.
> This means warnings reports can only be generated from
> a version of [arduino-cli] v0.14.0-rc.1 or later.
>
> The [original Github action][og-action] does not use the `--clean` flag.
> Instead it arbitrarily flushed all cached folders in `/tmp/arduino*`. This
> approach causes numerous problems when doing parallel compilations (and
> tests).

<!-- ANCHOR_END: CLI_CAVEATS -->

## Inputs

See the [Inputs document][inputs-link] for details about supported inputs.

## Example

```yaml
- uses: 2bndy5/arduino-compile-sketches@v0.1.0
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
[codecov-badge]: https://codecov.io/gh/2bndy5/arduino-compile-sketches/graph/badge.svg?token=MNPE7GIXNC
[codecov-link]: https://codecov.io/gh/2bndy5/arduino-compile-sketches
[rust-ci-badge]: https://github.com/2bndy5/arduino-compile-sketches/actions/workflows/rust.yml/badge.svg
[rust-ci-link]: https://github.com/2bndy5/arduino-compile-sketches/actions/workflows/rust.yml
[crates.io-badge]: https://img.shields.io/crates/v/arduino-compile-sketches
[crates.io-link]: https://crates.io/crates/arduino-compile-sketches
[docs.rs-badge]: https://img.shields.io/docsrs/arduino-compile-sketches?logo=rust&label=API%20docs
[docs.rs-link]: https://docs.rs/arduino-compile-sketches/
[action-docs-badge]: https://img.shields.io/github/deployments/2bndy5/arduino-compile-sketches/github-pages?logo=github&label=Action%20Docs
[gh-pages]: https://2bndy5.github.io/arduino-compile-sketches

<!-- ANCHOR_END: INTRO -->
[inputs-link]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html
[enable-warnings-report-input]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html#enable-warnings-report
[enable-deltas-report-input]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html#enable-deltas-report
[sketches-report-path-input]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html#sketches-report-path
[libraries-input]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html#libraries
[platforms-input]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html#platforms
[cli-version-input]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html#cli-version
[verbose-input]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html#verbose
[fail-on-compile-error-input]: https://2bndy5.github.io/arduino-compile-sketches/inputs.html#fail-on-compile-error
