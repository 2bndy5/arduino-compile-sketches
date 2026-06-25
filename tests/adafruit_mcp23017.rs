//! Integration tests that compile real Arduino sketches against two pinned SHAs
//! of `adafruit/Adafruit-MCP23017-Arduino-Library` (PR #89).
#![cfg(feature = "bin")]

use arduino_compile_sketches::{CompileSketches, driver::DefaultPaths, logger};
use arduino_report_size_deltas::report_structs::{Report, SizeValue, SketchSizeKind};
use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

// ── Pinned test-subject constants ─────────────────────────────────────────────

const TEST_REPO: &str = "adafruit/Adafruit-MCP23017-Arduino-Library";

/// Master commit just before PR #89 was merged.
const BASE_SHA: &str = "0e82c8d873037ff2522b9167ac20433ac23ef0d4";

/// HEAD commit of PR #89
/// This is the only SHA that differs for `mcp23xxx_interrupt`.
const HEAD_SHA: &str = "6f92c72afa71e7b47eef2227580c7c3f30bffc26";

/// Example whose tree-SHA is **identical** at both commits -> near-zero delta.
const SKETCH_STABLE: &str = "examples/mcp23xxx_blink";

/// Example modified by PR #89 -> measurable size delta when base vs head differ.
const SKETCH_MODIFIED: &str = "examples/mcp23xxx_interrupt";

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_posix_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn run_git(current_dir: &Path, args: &[&str], task: &str) {
    let mut cmd = Command::new("git");
    log::info!("Using git to {task}: git {}", args.join(" "));
    cmd.current_dir(current_dir)
        .args(["-c", "advice.detachedHead=false"])
        .args(args);
    if env::var("CI").is_ok_and(|v| v == "true") {
        cmd.env("GIT_AUTHOR_NAME", "ci-test")
            .env("GIT_AUTHOR_EMAIL", "ci@example.invalid")
            .env("GIT_COMMITTER_NAME", "ci-test")
            .env("GIT_COMMITTER_EMAIL", "ci@example.invalid");
    }

    let status = cmd.status().expect("spawn git command");
    assert!(
        status.success(),
        "git command failed while trying to {task}"
    );
}

fn ensure_head_cache(repo: &str, head_sha: &str) -> PathBuf {
    let cache_root = env::temp_dir().join("arduino-compile-sketches-tests");
    fs::create_dir_all(&cache_root).expect("create cache root");

    let lock_path = cache_root.join("cache.lock");
    let lock_file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .expect("open cache lock file");
    lock_file.lock().expect("lock cache lock file");

    // only use the repo name as cache dir name; otherwise "adafruit" would appear twice in the same dir name.
    let repo_dir = cache_root.join(repo.split_once('/').unwrap().1);
    let repo_url = format!("https://github.com/{repo}.git");
    if !repo_dir.exists() {
        run_git(
            &cache_root,
            &[
                "clone",
                &repo_url,
                &repo_dir.to_string_lossy(),
                "--depth",
                "3",
                "--revision",
                head_sha,
                "--recurse-submodules",
                "--shallow-submodules",
            ],
            "clone cached test repository",
        );
    } else {
        run_git(
            &repo_dir,
            &["fetch", "origin", head_sha, "--depth", "3"],
            "fetch cached head commit",
        );
        run_git(
            &repo_dir,
            &["checkout", "-f", head_sha],
            "checkout cached head commit",
        );
    }
    lock_file.unlock().expect("unlock cache lock file");
    repo_dir
}

fn clone_cached_head_workspace(repo: &str, head_sha: &str) -> TempDir {
    let repo_dir = ensure_head_cache(repo, head_sha);
    let workspace = TempDir::new().expect("create temp dir for workspace clone");

    run_git(
        workspace.path(),
        &["clone", repo_dir.to_string_lossy().as_ref(), "."],
        "clone workspace from local cache",
    );
    run_git(
        workspace.path(),
        &["checkout", "-f", head_sha],
        "checkout workspace head commit",
    );

    workspace
}

struct LocalGitRepo {
    _working_dir: TempDir,
    _bare_root: TempDir,
    source_url: String,
}

fn write_minimal_platform_files(root: &Path) {
    fs::write(
        root.join("platform.txt"),
        "name=Test Platform\nversion=1.0.0\n",
    )
    .unwrap();
    fs::write(root.join("boards.txt"), "test.board.name=Test Board\n").unwrap();
}

fn create_local_path_platform() -> TempDir {
    let tmp = TempDir::new().expect("create temp dir for path platform");
    write_minimal_platform_files(tmp.path());
    tmp
}

fn create_local_repo_platform() -> LocalGitRepo {
    let work = TempDir::new().expect("create temp dir for repo platform");
    write_minimal_platform_files(work.path());

    run_git(
        work.path(),
        &["init", "--initial-branch=main"],
        "init test platform repo",
    );
    run_git(work.path(), &["add", "."], "add test platform repo files");
    run_git(
        work.path(),
        &["commit", "-m", "init"],
        "commit test platform repo files",
    );

    let bare_root = TempDir::new().expect("create temp dir for bare repo platform");
    let bare_repo_path = bare_root.path().join("TestPlatform.git");
    run_git(
        bare_root.path(),
        &[
            "clone",
            "--bare",
            work.path().to_string_lossy().as_ref(),
            bare_repo_path.to_string_lossy().as_ref(),
        ],
        "clone bare test platform repo",
    );

    LocalGitRepo {
        _working_dir: work,
        _bare_root: bare_root,
        source_url: bare_repo_path.to_string_lossy().to_string(),
    }
}

fn zip_dir_recursive(
    archive: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
    root: &Path,
    dir: &Path,
    top_name: &str,
) {
    let entries = fs::read_dir(dir).expect("read fixture directory");
    for entry in entries {
        let entry = entry.expect("read fixture entry");
        let path = entry.path();
        if path.is_dir() {
            zip_dir_recursive(archive, root, &path, top_name);
            continue;
        }

        let rel = path
            .strip_prefix(root)
            .expect("strip fixture root")
            .to_string_lossy()
            .replace('\\', "/");
        let zip_path = format!("{top_name}/{rel}");
        archive
            .start_file(zip_path, zip::write::SimpleFileOptions::default())
            .expect("start zip file");
        let bytes = fs::read(&path).expect("read fixture file");
        archive.write_all(&bytes).expect("write zip file");
    }
}

/// Pack `src_dir` into an in-memory `.zip` with a single top-level folder.
fn build_zip(src_dir: &Path, top_name: &str) -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    zip_dir_recursive(&mut archive, src_dir, src_dir, top_name);
    archive.finish().expect("finish zip").into_inner()
}

/// Build a GitHub `pull_request` event payload JSON string.
fn pr_event_json(base_sha: &str, head_sha: &str) -> String {
    serde_json::json!({
        "action": "synchronize",
        "pull_request": {
            "base": { "sha": base_sha },
            "head": { "sha": head_sha }
        }
    })
    .to_string()
}

fn to_yaml_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<String>>()
        .join("\n")
}

// ── TestParams + driver ───────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct TestParams {
    fqbn: &'static str,
    /// Install `arduino:avr` via the board manager.
    manager_platform: bool,
    /// Symlink a local custom platform via path dependency.
    use_path_platform: bool,
    /// Clone and install a local git-backed custom platform.
    use_repo_platform: bool,
    /// Download and install a custom platform archive from mockito.
    use_download_platform: bool,
    /// Add `"Adafruit BusIO"` via the library manager.
    use_manager_lib: bool,
    /// Serve `tests/dep_fixtures/download-lib/` via mockito and install it.
    use_download_lib: bool,
    /// `true` -> clone the real repo at HEAD_SHA as the workspace.
    /// `false` -> create a minimal local workspace (for error-path tests).
    use_real_repo: bool,
    /// When `use_real_repo = false`, include a sketch with `#error` in the local workspace.
    include_bad_sketch: bool,
    /// PR event with delta report; `false` -> push event.
    is_pr: bool,
    enable_deltas: bool,
    fail_on_compile_error: bool,
    /// Whether `compile_sketches()` is expected to return `Err`.
    expect_err: bool,
    /// Expected sketch count in the report (checked only when `!expect_err`).
    expected_sketch_count: usize,
}

async fn run_compile_test(params: TestParams) {
    logger::init();
    log::set_max_level(log::LevelFilter::Debug);

    // ── 1. Workspace setup ────────────────────────────────────────────────────
    let workspace_dir: TempDir;
    let sketch_paths: Vec<PathBuf>;

    if params.use_real_repo {
        workspace_dir = clone_cached_head_workspace(TEST_REPO, HEAD_SHA);
        sketch_paths = vec![
            workspace_dir.path().join(SKETCH_STABLE),
            workspace_dir.path().join(SKETCH_MODIFIED),
        ];
    } else {
        workspace_dir = TempDir::new().unwrap();
        let good_dir = workspace_dir.path().join("good_sketch");
        fs::create_dir_all(&good_dir).unwrap();
        fs::write(
            good_dir.join("good_sketch.ino"),
            "void setup(){} void loop(){}\n",
        )
        .unwrap();
        sketch_paths = if params.include_bad_sketch {
            let bad_dir = workspace_dir.path().join("bad_sketch");
            fs::create_dir_all(&bad_dir).unwrap();
            fs::write(
                bad_dir.join("bad_sketch.ino"),
                "#error \"Forced compile failure\"\nvoid setup(){} void loop(){}\n",
            )
            .unwrap();
            vec![good_dir, bad_dir]
        } else {
            vec![good_dir]
        };
    }

    let workspace_str = workspace_dir.path().to_string_lossy().to_string();
    let unique_suffix = std::process::id().to_string();

    // ── 2. Platform fixtures (optional) ─────────────────────────────────────
    let path_platform_dir = if params.use_path_platform {
        Some(create_local_path_platform())
    } else {
        None
    };
    let repo_platform_dir = if params.use_repo_platform {
        Some(create_local_repo_platform())
    } else {
        None
    };

    // ── 3. mockito download-lib (optional) ────────────────────────────────────
    let mut mock_server = mockito::Server::new_async().await;
    if params.use_download_lib {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dep_fixtures/download-lib");
        let zip_bytes = build_zip(&fixture, "download-lib");
        mock_server
            .mock("GET", "/download-lib.zip")
            .with_body(zip_bytes)
            .create();
    }

    if params.use_download_platform {
        let fixture = create_local_path_platform();
        let zip_bytes = build_zip(fixture.path(), "download-platform");
        mock_server
            .mock("GET", "/download-platform.zip")
            .with_body(zip_bytes)
            .create();
    }

    // ── 4. Event JSON + INPUT_* env setup ────────────────────────────────────
    let event_dir = TempDir::new().unwrap();
    let event_path = event_dir.path().join("event.json");
    if params.is_pr {
        fs::write(&event_path, pr_event_json(BASE_SHA, HEAD_SHA)).unwrap();
    } else {
        fs::write(
            &event_path,
            serde_json::json!({"before": BASE_SHA}).to_string(),
        )
        .unwrap();
    }

    // ── 5. Build INPUT_* YAML values ──────────────────────────────────────────
    let report_dir = TempDir::new().unwrap();
    let mut libraries_yaml = Vec::new();

    if params.use_manager_lib || params.use_real_repo {
        libraries_yaml.push("name: Adafruit BusIO".to_string());
    }

    if params.use_download_lib {
        let url = format!("{}/download-lib.zip", mock_server.url());
        libraries_yaml.push(format!("source-url: {url}"));
    }

    // If we're compiling examples from a cloned repo workspace, expose that
    // workspace root as a path-library so example `#include`/library resolution
    // can find the library sources in-tree.
    if params.use_real_repo {
        let ws_path = to_posix_path(workspace_dir.path());
        libraries_yaml.push(format!(
            "source-path: {ws_path}\n  name: RepoWorkspace_{unique_suffix}"
        ));
    }

    let mut platforms_yaml = Vec::new();
    if params.manager_platform {
        platforms_yaml.push("name: arduino:avr".to_string());
    }

    if let Some(platform_dir) = path_platform_dir.as_ref() {
        let source_path = to_posix_path(platform_dir.path());
        platforms_yaml.push(format!(
            "source-path: {source_path}\n  name: test-path_{unique_suffix}:arch"
        ));
    }

    if let Some(platform_dir) = repo_platform_dir.as_ref() {
        platforms_yaml.push(format!(
            "source-url: {}\n  name: test-repo_{unique_suffix}:arch",
            platform_dir.source_url
        ));
    }

    if params.use_download_platform {
        let url = format!("{}/download-platform.zip", mock_server.url());
        platforms_yaml.push(format!(
            "source-url: {url}\n  destination-name: test-dl_{unique_suffix}:arch"
        ));
    }

    let sketch_paths_yaml = to_yaml_list(
        &sketch_paths
            .iter()
            .map(|path| to_posix_path(path))
            .collect::<Vec<String>>(),
    );

    let mut env_pairs = vec![
        ("INPUT_FQBN".to_string(), params.fqbn.to_string()),
        ("INPUT_PLATFORMS".to_string(), to_yaml_list(&platforms_yaml)),
        ("INPUT_LIBRARIES".to_string(), to_yaml_list(&libraries_yaml)),
        ("INPUT_SKETCH-PATHS".to_string(), sketch_paths_yaml),
        (
            "INPUT_SKETCHES-REPORT-PATH".to_string(),
            to_posix_path(report_dir.path()),
        ),
        (
            "INPUT_FAIL-ON-COMPILE-ERROR".to_string(),
            params.fail_on_compile_error.to_string(),
        ),
        (
            "INPUT_ENABLE-DELTAS-REPORT".to_string(),
            params.enable_deltas.to_string(),
        ),
        (
            "INPUT_ENABLE-WARNINGS-REPORT".to_string(),
            "false".to_string(),
        ),
        ("INPUT_VERBOSE".to_string(), "false".to_string()),
        ("INPUT_CLI-VERSION".to_string(), "latest".to_string()),
        ("GITHUB_EVENT_PATH".to_string(), to_posix_path(&event_path)),
        ("GITHUB_WORKSPACE".to_string(), workspace_str),
        ("GITHUB_REPOSITORY".to_string(), TEST_REPO.to_string()),
    ];
    if params.is_pr {
        env_pairs.push(("GITHUB_EVENT_NAME".to_string(), "pull_request".to_string()));
    } else {
        env_pairs.push(("GITHUB_EVENT_NAME".to_string(), "push".to_string()));
        env_pairs.push(("GITHUB_SHA".to_string(), HEAD_SHA.to_string()));
    }
    for (k, v) in &env_pairs {
        // SAFETY: tests serialize environment access with ENV_LOCK.
        unsafe {
            env::set_var(k, v);
        }
    }

    // ── 7. Construct CompileSketches ──────────────────────────────────────────
    // `new_from_env()` uses clap to process input args via env vars
    let mut app = CompileSketches::from_cli(&[]).expect("build app from INPUT_* env vars");

    // replace paths with test-specific paths
    let new_default_paths = DefaultPaths::new_in(&report_dir.path().join("test-workspace"));
    app.relocate_paths(new_default_paths);

    // ── 8. Run ────────────────────────────────────────────────────────────────
    let result = app.compile_sketches().await;

    if params.expect_err {
        assert!(result.is_err(), "expected compile_sketches() to return Err");
        return;
    }
    result.expect("compile_sketches() should succeed");

    // ── 9. Verify report ──────────────────────────────────────────────────────
    let report_file = report_dir
        .path()
        .join(params.fqbn.replace(':', "-") + ".json");
    assert!(
        report_file.exists(),
        "report JSON not written to {report_file:?}"
    );

    let report_data = fs::read_to_string(report_file).unwrap();
    let report: Report = serde_json::from_str(&report_data)
        .unwrap_or_else(|e| panic!("failed to parse report JSON data: {e:?}\n{report_data}"));

    let sketches = &report.boards[0].sketches;
    assert_eq!(
        sketches.len(),
        params.expected_sketch_count,
        "wrong sketch count in report"
    );

    // For PR+delta runs: at least one sketch should have `previous` size data,
    // confirming the base-ref compilation was matched and merged into deltas.
    if params.is_pr && params.enable_deltas {
        let any_sizes = sketches.iter().any(|sketch| {
            sketch.sizes.iter().any(|size| {
                size.get_size()
                    .delta
                    .as_ref()
                    .is_some_and(|d| matches!(d.absolute, SizeValue::Known(_)))
            })
        });
        let found_previous = sketches.iter().any(|sketch| {
            sketch.sizes.iter().any(|size_kind| {
                if let SketchSizeKind::Flash { size } = size_kind
                    && size.previous.is_some()
                {
                    true
                } else {
                    false
                }
            })
        });

        if any_sizes {
            assert!(
                found_previous,
                "expected at least one flash size to include previous delta data"
            );
        }
    }
}

// ── Concrete test functions ───────────────────────────────────────────────────

/// Happy path: all 4 library dependency types, PR event with delta report.
#[tokio::test]
async fn pr_delta() {
    run_compile_test(TestParams {
        fqbn: "arduino:avr:uno",
        manager_platform: true,
        // use_manager_lib: true,
        // use_download_lib: true,
        use_real_repo: true,
        is_pr: true,
        enable_deltas: true,
        expected_sketch_count: 2,
        ..Default::default()
    })
    .await;
}

/// Push event, no delta report, manager library only.
#[tokio::test]
async fn push_no_delta() {
    run_compile_test(TestParams {
        fqbn: "arduino:avr:uno",
        manager_platform: true,
        use_manager_lib: true,
        use_real_repo: true,
        expected_sketch_count: 2,
        ..Default::default()
    })
    .await;
}

/// A bogus FQBN causes compilation to fail; `fail_on_compile_error` propagates that as Err.
#[tokio::test]
async fn invalid_fqbn_fails() {
    run_compile_test(TestParams {
        fqbn: "bogus:fake:board",
        manager_platform: false, // unknown vendor – nothing to install
        fail_on_compile_error: true,
        expect_err: true,
        ..Default::default()
    })
    .await;
}

/// A sketch with `#error` fails and `fail_on_compile_error = true` propagates as Err.
#[tokio::test]
async fn compile_error_respected() {
    run_compile_test(TestParams {
        fqbn: "arduino:avr:uno",
        manager_platform: true,
        include_bad_sketch: true,
        fail_on_compile_error: true,
        expect_err: true,
        ..Default::default()
    })
    .await;
}

/// Same bad sketch, but `fail_on_compile_error = false` -> succeeds and writes report.
/// The good sketch still shows up in the report; the bad one has `compilation_success: false`.
#[tokio::test]
async fn compile_error_ignored() {
    run_compile_test(TestParams {
        fqbn: "arduino:avr:uno",
        manager_platform: true,
        include_bad_sketch: true,
        expected_sketch_count: 2, // good_sketch + bad_sketch both appear in report
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn platform_path_dependency() {
    run_compile_test(TestParams {
        fqbn: "arduino:avr:uno",
        manager_platform: true,
        use_path_platform: true,
        use_real_repo: true,
        expected_sketch_count: 2,
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn platform_repo_dependency() {
    run_compile_test(TestParams {
        fqbn: "arduino:avr:uno",
        manager_platform: true,
        use_repo_platform: true,
        use_real_repo: true,
        expected_sketch_count: 2,
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn platform_download_dependency() {
    run_compile_test(TestParams {
        fqbn: "arduino:avr:uno",
        manager_platform: true,
        use_download_platform: true,
        use_real_repo: true,
        expected_sketch_count: 2,
        ..Default::default()
    })
    .await;
}
