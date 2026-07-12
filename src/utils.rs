use crate::{
    error::{CompileSketchesError, Result},
    serde_types::PrEventInfo,
};
use std::{env, ffi::OsStr, fs, path::Path, process::Command, time::Duration};

pub(crate) fn get_base_ref() -> Option<String> {
    if let Ok(event_name) = env::var("GITHUB_EVENT_NAME")
        && let Ok(event_path) = env::var("GITHUB_EVENT_PATH")
        && let Ok(f) = fs::File::open(event_path)
    {
        match event_name.as_str() {
            "pull_request" => {
                if let Ok(info) = serde_json::from_reader::<_, PrEventInfo>(f) {
                    return Some(info.pull_request.base.sha.clone());
                }
            }
            "push" => {
                if let Ok(info) = serde_json::from_reader::<_, serde_json::Value>(f)
                    && let Some(head_ref) = info.get("before").and_then(|v| v.as_str())
                {
                    return Some(head_ref.to_string());
                }
            }
            _ => {} // treat all other events as unsupported
        }
    }
    log::warn!("Failed to get base ref from event payload");

    let ws = env::var("GITHUB_WORKSPACE").unwrap_or_else(|_| ".".to_string());
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "HEAD~1"])
        .current_dir(ws)
        .output()
        && output.status.success()
        && let Ok(s) = String::from_utf8(output.stdout)
    {
        return Some(s.trim().to_string());
    }
    None
}

pub(crate) fn get_head_ref() -> Option<String> {
    if let Ok(event_name) = env::var("GITHUB_EVENT_NAME") {
        match event_name.as_str() {
            "pull_request" => {
                if let Ok(event_path) = env::var("GITHUB_EVENT_PATH")
                    && let Ok(payload) = fs::read_to_string(event_path)
                    && let Ok(info) = serde_json::from_str::<PrEventInfo>(&payload)
                {
                    return Some(info.pull_request.head.sha.clone());
                }
                log::warn!("Failed to get head ref from PR event payload");
            }
            "push" => {
                if let Ok(head) = env::var("GITHUB_SHA") {
                    return Some(head);
                }
                log::warn!("Failed to get head ref for push event from env var");
            }
            _ => {} // treat all other events as unsupported
        }
    }
    log::warn!("Failed to get head ref from event payload");

    let ws = env::var("GITHUB_WORKSPACE").unwrap_or_else(|_| ".".to_string());
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(ws)
        .output()
        && output.status.success()
        && let Ok(s) = String::from_utf8(output.stdout)
    {
        return Some(s.trim().to_string());
    }
    None
}

pub(crate) fn fmt_duration(duration: &Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 60 {
        let minutes = secs / 60;
        if secs >= 3600 {
            let hours = secs / 3600;
            let minutes = minutes % 60;
            return format!("{hours}h {minutes}m {}s", secs % 60);
        }
        format!("{minutes}m {}s", secs % 60)
    } else {
        format!("{}s", secs)
    }
}

pub(crate) fn create_symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    log::debug!(
        "Creating symlink at {} for target {}",
        dst.to_string_lossy(),
        src.to_string_lossy(),
    );

    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst)?;

    #[cfg(windows)]
    {
        if src.is_dir() {
            std::os::windows::fs::symlink_dir(src, dst)?;
        } else {
            std::os::windows::fs::symlink_file(src, dst)?;
        }
    }
    Ok(())
}

/// Check if a directory is hidden (i.e., its name starts with a dot).
fn is_dir_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

/// Recursively visit files/directories under `path`, calling `cb` for each file or directory found.
pub(crate) fn visit_dirs_recursive<F>(path: &Path, cb: &mut F) -> std::io::Result<()>
where
    F: FnMut(&Path),
{
    if path.is_dir() && !is_dir_hidden(path) {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            cb(&p);
            if p.is_dir() && !is_dir_hidden(&p) {
                visit_dirs_recursive(&p, cb)?;
            }
        }
    }
    Ok(())
}

/// Check if the given `path` is an Arduino sketch file.
///
/// Modern sketches use the extension ".ino", but older sketches may still use ".pde".
/// Both extensions are supported nonetheless.
///
/// Returns false if passed a directory.
pub(crate) fn path_is_sketch(path: &Path) -> bool {
    if path.is_file()
        && let Some(ext) = path.extension().and_then(OsStr::to_str)
    {
        return ext.eq_ignore_ascii_case("ino") || ext.eq_ignore_ascii_case("pde");
    }
    false
}

pub(crate) fn path_relative_to_workspace(path: &Path) -> Result<String> {
    let ws = env::current_dir()
        .map_err(|e| CompileSketchesError::DetectWorkingDirectory { source: e })?;
    Ok(path
        .canonicalize()
        .map_err(|e| CompileSketchesError::ResolvePath {
            path_for: "",
            path: path.to_string_lossy().to_string(),
            source: e,
        })?
        .strip_prefix(&ws)
        .map(|p| p.to_path_buf().to_string_lossy().replace("\\", "/"))
        .unwrap_or(path.to_string_lossy().replace("\\", "/")))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::{
        fs::{self, File},
        io::Write,
        process::Command,
    };
    use tempfile::tempdir;

    #[test]
    fn test_path_is_sketch_file_and_dir() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.ino");
        File::create(&file_path).unwrap();
        assert!(path_is_sketch(&file_path));
        // dir is not a sketch file
        assert!(!path_is_sketch(dir.path()));
        // non-sketch
        let other = dir.path().join("readme.txt");
        File::create(&other).unwrap();
        assert!(!path_is_sketch(&other));
    }

    #[test]
    fn base_ref_from_event_payload() {
        // base.sha present
        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("event.json");
        let mut f = File::create(&p).unwrap();
        writeln!(
            f,
            "{}",
            serde_json::json!({
                "pull_request": {
                    "base": { "sha": "base-sha" },
                    "head": { "sha": "head-sha" },
                }
            })
        )
        .unwrap();
        unsafe {
            std::env::set_var("GITHUB_EVENT_NAME", "pull_request");
            std::env::set_var("GITHUB_EVENT_PATH", p.to_str().unwrap());
        }
        let val = get_base_ref();
        assert_eq!(val.unwrap(), "base-sha");
    }

    #[test]
    fn base_ref_from_push_event() {
        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("event.json");
        let mut f = File::create(&p).unwrap();
        writeln!(
            f,
            "{}",
            serde_json::json!({
                "before": "base-sha",
            })
        )
        .unwrap();
        unsafe {
            std::env::set_var("GITHUB_EVENT_NAME", "push");
            std::env::set_var("GITHUB_EVENT_PATH", p.to_str().unwrap());
        }
        let val = get_base_ref();
        assert_eq!(val.unwrap(), "base-sha");
    }

    fn create_ref_from_git_cli(
        event_kind: &str,
        with_base_ref: bool,
    ) -> (String, tempfile::TempDir) {
        #[cfg(feature = "bin")]
        {
            // trigger `log::warn!()` calls
            crate::logger::init();
            log::set_max_level(log::LevelFilter::Debug);
        }

        let tmp_dir = tempfile::tempdir().unwrap();
        let repo = tmp_dir.path().join("tmp-repo-test");
        let event_path = tmp_dir.path().join("event.json");
        fs::write(&event_path, "{}").unwrap();
        unsafe {
            std::env::set_var("GITHUB_WORKSPACE", repo.to_str().unwrap());
            std::env::set_var("GITHUB_EVENT_NAME", event_kind);
            // clear any env to force fallback
            std::env::set_var("GITHUB_EVENT_PATH", event_path.to_str().unwrap());
            // remove the GITHUB_SHA env var to force fallback to git CLI on simulated push event
            std::env::remove_var("GITHUB_SHA");
        }

        // create a git repo with head commit
        fs::create_dir(&repo).unwrap();
        fs::write(repo.join("a.txt"), "one").unwrap();
        Command::new("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .arg("add")
            .arg(".")
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg("c1")
            .current_dir(&repo)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .unwrap();
        if with_base_ref {
            // push a commit to create a new head commit (letting old head become the base ref)
            fs::write(repo.join("a.txt"), "two").unwrap();
            Command::new("git")
                .arg("add")
                .arg(".")
                .current_dir(&repo)
                .status()
                .unwrap();
            Command::new("git")
                .arg("commit")
                .arg("-m")
                .arg("c2")
                .current_dir(&repo)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.com")
                .status()
                .unwrap();
        }
        let out = Command::new("git")
            .current_dir(&repo)
            .args(["rev-parse", if with_base_ref { "HEAD~1" } else { "HEAD" }])
            .output()
            .unwrap();
        let expected = String::from_utf8(out.stdout).unwrap().trim().to_string();
        (expected, tmp_dir)
    }

    #[test]
    fn base_ref_for_push_from_git_cli() {
        let (expected, tmp) = create_ref_from_git_cli("push", true);
        let val = get_base_ref();
        assert_eq!(val.unwrap(), expected);
        drop(tmp);
    }

    #[test]
    fn base_ref_for_pr_from_git_cli() {
        let (expected, tmp) = create_ref_from_git_cli("pull_request", true);
        let val = get_base_ref();
        assert_eq!(val.unwrap(), expected);
        drop(tmp);
    }

    #[test]
    fn base_ref_for_unknown_event_from_git_cli() {
        let (expected, tmp) = create_ref_from_git_cli("unsupported", true);
        let val = get_base_ref();
        assert_eq!(val.unwrap(), expected);
        drop(tmp);
    }

    #[test]
    fn head_ref_for_push_from_git_cli() {
        let (expected, tmp) = create_ref_from_git_cli("push", false);
        let val = get_head_ref();
        assert_eq!(val.unwrap(), expected);
        drop(tmp);
    }

    #[test]
    fn head_ref_for_pr_from_git_cli() {
        let (expected, tmp) = create_ref_from_git_cli("pull_request", false);
        let val = get_head_ref();
        assert_eq!(val.unwrap(), expected);
        drop(tmp);
    }

    #[test]
    fn head_ref_for_push_from_env() {
        let expected = "Some SHA";
        unsafe {
            std::env::set_var("GITHUB_EVENT_NAME", "push");
            std::env::set_var("GITHUB_SHA", expected);
        }
        let val = get_head_ref();
        assert_eq!(val.unwrap(), expected);
    }

    #[test]
    fn head_ref_for_unknown_event_from_git_cli() {
        let (expected, tmp) = create_ref_from_git_cli("unsupported", false);
        let val = get_head_ref();
        assert_eq!(val.unwrap(), expected);
        drop(tmp);
    }

    #[test]
    fn duration_str() {
        assert_eq!(fmt_duration(&Duration::from_secs(45)), "45s");
        assert_eq!(fmt_duration(&Duration::from_secs(75)), "1m 15s");
        assert_eq!(fmt_duration(&Duration::from_secs(3600)), "1h 0m 0s");
        assert_eq!(fmt_duration(&Duration::from_secs(3665)), "1h 1m 5s");
    }

    fn setup_unknown_git_ref() -> tempfile::TempDir {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        unsafe {
            env::set_var("GITHUB_WORKSPACE", tmp_dir.path().to_str().unwrap());
            env::remove_var("GITHUB_EVENT_NAME");
        }
        tmp_dir
    }

    #[test]
    fn unknown_head_ref() {
        let tmp = setup_unknown_git_ref();
        let val = get_head_ref();
        assert!(val.is_none());
        drop(tmp);
    }

    #[test]
    fn unknown_base_ref() {
        let tmp = setup_unknown_git_ref();
        let val = get_base_ref();
        assert!(val.is_none());
        drop(tmp);
    }

    #[test]
    fn skip_hidden_path() {
        let tmp = tempfile::TempDir::with_prefix(".test-dummy").unwrap();
        let mut found = vec![];
        let mut cb = |p: &Path| found.push(p.to_string_lossy().into_owned());
        visit_dirs_recursive(tmp.path(), &mut cb).unwrap();
        assert!(found.is_empty());
        drop(tmp);
    }
}
