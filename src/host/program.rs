//! Finding the program `karen run` was asked to host. The Karen who reads `PATH` so you don't have
//! to quote it back at her.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// How to start a resolved program: the file to execute and the arguments in front of yours. A
/// `.cmd` or `.bat` file cannot be started directly, so on Windows it goes through `cmd.exe`.
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub prefix: Vec<OsString>,
}

/// The extensions Windows tries, from `PATHEXT` (or its usual default). Empty off Windows.
fn extensions() -> Vec<String> {
    if !cfg!(windows) {
        return vec![String::new()];
    }
    let from_env = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let mut found = vec![String::new()];
    found.extend(
        from_env
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(str::to_lowercase),
    );
    found
}

fn is_runnable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn with_extension(base: &Path, ext: &str) -> PathBuf {
    if ext.is_empty() {
        return base.to_path_buf();
    }
    let mut name = base.as_os_str().to_os_string();
    name.push(ext);
    PathBuf::from(name)
}

/// The file `name` stands for: itself when it has a directory part, else the first match on `path`.
#[must_use]
pub fn resolve(name: &OsStr, path: Option<&OsStr>) -> Option<PathBuf> {
    let exts = extensions();
    let named = Path::new(name);
    let has_directory = named.components().count() > 1 || named.is_absolute();
    let dirs: Vec<PathBuf> = if has_directory {
        vec![PathBuf::new()]
    } else {
        std::env::split_paths(path?).collect()
    };
    dirs.iter()
        .flat_map(|dir| {
            exts.iter()
                .map(move |ext| with_extension(&dir.join(named), ext))
        })
        .find(|candidate| is_runnable(candidate))
}

/// How to start `name`, or `None` when it is not on `path`.
#[must_use]
pub fn launch(name: &OsStr, path: Option<&OsStr>) -> Option<Launch> {
    let found = resolve(name, path)?;
    let is_script = found
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"));
    if cfg!(windows) && is_script {
        return Some(Launch {
            program: PathBuf::from("cmd.exe"),
            prefix: vec!["/d".into(), "/c".into(), found.into_os_string()],
        });
    }
    Some(Launch {
        program: found,
        prefix: Vec::new(),
    })
}

/// The name Karen records for a hosted program: its file stem, lowercased (`claude`, `codex`,
/// `opencode`, or whatever else the owner hosts).
#[must_use]
pub fn runtime_name(name: &OsStr) -> String {
    Path::new(name)
        .file_stem()
        .map_or_else(
            || name.to_string_lossy().into_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        )
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[test]
    fn program_is_found_on_the_path_by_its_bare_name() {
        let dir = tempfile::tempdir().unwrap();
        let file = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        let expected = executable(dir.path(), file);
        let found = resolve(OsStr::new("claude"), Some(dir.path().as_os_str()));
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn program_that_is_not_on_the_path_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve(OsStr::new("nothing-here"), Some(dir.path().as_os_str())),
            None
        );
    }

    #[test]
    fn program_with_a_directory_part_ignores_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = if cfg!(windows) { "tool.exe" } else { "tool" };
        let expected = executable(dir.path(), file);
        let spelled = dir.path().join("tool");
        assert_eq!(
            resolve(spelled.as_os_str(), Some(OsStr::new(""))),
            Some(expected)
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_cmd_shim_is_started_through_cmd() {
        let dir = tempfile::tempdir().unwrap();
        let shim = executable(dir.path(), "opencode.cmd");
        let launch = launch(OsStr::new("opencode"), Some(dir.path().as_os_str())).unwrap();
        assert_eq!(launch.program, PathBuf::from("cmd.exe"));
        assert_eq!(launch.prefix.last(), Some(&shim.into_os_string()));
    }

    #[test]
    fn runtime_is_named_by_its_lowercased_file_stem() {
        assert_eq!(runtime_name(OsStr::new("Claude")), "claude");
        assert_eq!(runtime_name(OsStr::new("C:/bin/OpenCode.exe")), "opencode");
        assert_eq!(runtime_name(OsStr::new("fake-runtime")), "fake-runtime");
    }
}
