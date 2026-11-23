//! The rebuild of a recompiled game or of a Rekarina install: the one thing this port does not reimplement.
//!
//! Lifting and compiling the game is `recomp.build` (Longshot/Hookshot's builder, with its toolchain, its PGO
//! directories and its environment) and `rekarina.build` (Rekarina's own three stages). Both live in the Python app
//! and stay there; this module runs that app's `rebuild.py` for the same folder and streams what it prints, so the
//! window and the CLI show the real build instead of a copy of it that would drift.

use crate::error::{Error, Result};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The Python interpreter to use: `SHB_PYTHON` when set (a path or a command), else the one this platform calls
/// `python3` — Windows has `py -3.12` instead, which is what the app is developed against.
fn interpreter() -> (String, Vec<String>) {
    if let Ok(custom) = std::env::var("SHB_PYTHON")
        && !custom.is_empty()
    {
        return (custom, Vec::new());
    }
    if cfg!(windows) {
        ("py".to_string(), vec!["-3.12".to_string()])
    } else {
        ("python3".to_string(), Vec::new())
    }
}

/// The folder of the Python app: the one that holds `rebuild.py`, found from the payload (`<app>/payload`).
pub fn app_dir(payload: &Path) -> Result<PathBuf> {
    let app = payload
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| payload.to_path_buf());
    if !app.join("rebuild.py").is_file() {
        return Err(Error::Rebuild(format!(
            "{} does not hold rebuild.py (point --app at the Python app)",
            app.display()
        )));
    }
    Ok(app)
}

/// What to rebuild.
#[derive(Debug, Clone)]
pub struct RebuildRequest {
    pub game: PathBuf,
    pub profile: String,
    /// `false` patches the settings only: the code stays as the last build left it.
    pub rebuild: bool,
    /// The Python app folder (the one with `rebuild.py` and `payload/`).
    pub app: PathBuf,
}

/// Run it, passing every line the Python side prints to `line`.
pub fn run(req: &RebuildRequest, mut report: impl FnMut(&str)) -> Result<()> {
    let script = req.app.join("rebuild.py");
    if !script.is_file() {
        return Err(Error::Rebuild(format!("{} is missing", script.display())));
    }
    let (program, prefix) = interpreter();
    let mut command = Command::new(&program);
    command
        .args(&prefix)
        .arg(&script)
        .arg("--game")
        .arg(&req.game);
    command.arg("--profile").arg(&req.profile);
    if !req.rebuild {
        command.arg("--no-rebuild");
    }
    command
        .current_dir(&req.app)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONUNBUFFERED", "1");

    let mut child = command
        .spawn()
        .map_err(|e| Error::Rebuild(format!("could not run {program}: {e}")))?;
    // stderr on its own thread: the build prints there too, and a full pipe would block it.
    let stderr = child.stderr.take();
    let handle = stderr.map(|stream| {
        std::thread::spawn(move || {
            let mut lines = Vec::new();
            for line in BufReader::new(stream)
                .lines()
                .map_while(std::result::Result::ok)
            {
                lines.push(line);
            }
            lines
        })
    });
    if let Some(stdout) = child.stdout.take() {
        for text in BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
        {
            report(&text);
        }
    }
    let status = child.wait().map_err(|e| Error::Rebuild(format!("{e}")))?;
    if let Some(handle) = handle
        && let Ok(lines) = handle.join()
    {
        for text in lines {
            report(&text);
        }
    }
    if status.success() {
        Ok(())
    } else {
        Err(Error::Rebuild(format!("the rebuild failed ({status})")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_without_the_script_is_reported() {
        let root = std::env::temp_dir().join(format!("shb-rs-rebuild-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("payload")).unwrap();
        let err = app_dir(&root.join("payload")).unwrap_err();
        assert!(matches!(err, Error::Rebuild(_)));
        assert!(format!("{err}").contains("rebuild.py"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_app_folder_is_the_payloads_parent() {
        let root = std::env::temp_dir().join(format!("shb-rs-rebuild-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("payload")).unwrap();
        std::fs::write(
            root.join("rebuild.py"),
            b"# the real one is app/rebuild.py\n",
        )
        .unwrap();
        assert_eq!(app_dir(&root.join("payload")).unwrap(), root);
        let _ = std::fs::remove_dir_all(root);
    }
}
