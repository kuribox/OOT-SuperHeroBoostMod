//! Errors of the companion app.
//!
//! The Python app raised `InstallError(key, arg)` and let the window localize the message. Here the key idea is
//! the same but the messages are attached at the call site, so a CLI user sees something useful without a
//! translation table in the middle.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    /// The module (`subsdk9`) does not carry the config block, or carries it twice.
    ConfigBlock(&'static str),
    /// The block was built by an incompatible module version.
    ConfigVersion { version: u32, size: u32 },
    /// The emulator folder is not one we can install into.
    NoModsDir(PathBuf),
    /// A file of the payload is missing (the payload is not committed; `tools/ci_payload.py` writes it).
    PayloadMissing(PathBuf),
    /// The target folder exists and was not installed by us, so we refuse to delete it.
    NotOurs(PathBuf),
    /// Two IPS files patch the same byte: the merged patch would be ambiguous.
    IpsOverlap(usize),
    /// Not an IPS32 file, or a record type we do not produce.
    IpsFormat(&'static str),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ConfigBlock(what) => write!(f, "bad module: config block {what}"),
            Error::ConfigVersion { version, size } => {
                write!(f, "bad module: config version {version}, size {size:#x}")
            }
            Error::NoModsDir(p) => write!(f, "no mods folder found under {}", p.display()),
            Error::PayloadMissing(p) => write!(f, "payload file missing: {}", p.display()),
            Error::NotOurs(p) => write!(
                f,
                "{} exists and was not installed by this app",
                p.display()
            ),
            Error::IpsOverlap(off) => write!(f, "IPS files overlap at main+{off:#x}"),
            Error::IpsFormat(why) => write!(f, "bad IPS file: {why}"),
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Read a whole file, reporting its path on failure.
pub fn read(path: &std::path::Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error::io(path, e))
}

/// Write a whole file, reporting its path on failure.
pub fn write(path: &std::path::Path, data: &[u8]) -> Result<()> {
    std::fs::write(path, data).map_err(|e| Error::io(path, e))
}
