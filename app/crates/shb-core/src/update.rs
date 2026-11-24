//! The update check and download (`app-legacy/updater.py` of the Python app).
//!
//! The GitHub releases are the only source: ask for the latest non-draft, non-prerelease release, compare its tag
//! with this build, and download the asset of this platform. On Windows the new executable replaces the running one
//! through a small detached PowerShell script that waits for the process to exit.

use crate::error::{Error, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use ureq::tls::{TlsConfig, TlsProvider};

/// An agent that speaks HTTPS through the system's TLS (schannel on Windows): see Cargo.toml for why not rustls.
fn agent(timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .build(),
        )
        .build()
        .new_agent()
}

pub const REPO: &str = "kuribox/OOT-SuperHeroBoostMod";
pub const API_LATEST: &str =
    "https://api.github.com/repos/kuribox/OOT-SuperHeroBoostMod/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/kuribox/OOT-SuperHeroBoostMod/releases";
pub const UA: &str = "SHB-OcarinaTweaks";

#[derive(Debug, Clone, Default)]
pub struct Update {
    pub version: String,
    pub tag: String,
    pub page_url: String,
    pub body: String,
    pub asset_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: u64,
    pub digest: Option<String>,
}

/// The numbers of a tag (`v0.9.3` → `[0, 9, 3]`); empty when there are none.
pub fn version_numbers(text: &str) -> Vec<u64> {
    let trimmed = text.trim().trim_start_matches(['v', 'V']);
    let digits: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    digits
        .split('.')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// Whether `tag` is newer than `current` (`v0.10.0` is newer than `0.9.3`; missing parts count as zero).
pub fn is_newer(tag: &str, current: &str) -> bool {
    let a = version_numbers(tag);
    if a.is_empty() {
        return false;
    }
    let b = version_numbers(current);
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// The asset names of this platform, in the order the Python app tries them.
pub fn platform_assets() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["SHB-OcarinaTweaks.exe", "SuperHeroBoostConfig.exe"]
    }
    #[cfg(target_os = "macos")]
    {
        &[
            "SHB-OcarinaTweaks-macos.dmg",
            "SHB-OcarinaTweaks.app.zip",
            "SHB-OcarinaTweaks-macos.zip",
            "SHB-OcarinaTweaks.dmg",
            "SHB-OcarinaTweaks",
            "SuperHeroBoostConfig-macos.dmg",
            "SuperHeroBoostConfig.app.zip",
            "SuperHeroBoostConfig-macos.zip",
            "SuperHeroBoostConfig.dmg",
            "SuperHeroBoostConfig",
        ]
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        &[
            "SHB-OcarinaTweaks-linux",
            "SHB-OcarinaTweaks",
            "SuperHeroBoostConfig-linux",
            "SuperHeroBoostConfig",
        ]
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// The newest release when it is newer than `current`; `None` when this build is up to date.
pub fn check(current: &str) -> Result<Option<Update>> {
    let response = agent(15)
        .get(API_LATEST)
        .header("User-Agent", &format!("{UA}/{current}"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| Error::Update(format!("{e}")))?;
    let data: Value = serde_json::from_str(
        &response
            .into_body()
            .read_to_string()
            .map_err(|e| Error::Update(format!("{e}")))?,
    )
    .map_err(|e| Error::Update(format!("bad response: {e}")))?;
    if data.get("draft").and_then(|v| v.as_bool()) == Some(true)
        || data.get("prerelease").and_then(|v| v.as_bool()) == Some(true)
    {
        return Ok(None);
    }
    let tag = text(&data, "tag_name");
    if !is_newer(&tag, current) {
        return Ok(None);
    }
    let assets = data
        .get("assets")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    let picked = platform_assets().iter().find_map(|name| {
        assets
            .iter()
            .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(name))
    });
    let get = |key: &str| picked.and_then(|a| a.get(key));
    Ok(Some(Update {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        tag,
        page_url: match text(&data, "html_url") {
            url if url.is_empty() => RELEASES_PAGE.to_string(),
            url => url,
        },
        body: text(&data, "body"),
        asset_url: get("browser_download_url")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        asset_name: get("name").and_then(|v| v.as_str()).map(str::to_string),
        asset_size: get("size").and_then(|v| v.as_u64()).unwrap_or(0),
        digest: get("digest").and_then(|v| v.as_str()).map(str::to_string),
    }))
}

fn writable(folder: &Path) -> bool {
    let probe = folder.join(format!(".shb-update-{}.tmp", std::process::id()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// The per-user cache folder for downloads when the program's own folder cannot be written.
pub fn updates_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::paths::home().join("AppData/Local"));
        base.join("SHB-OcarinaTweaks").join("updates")
    }
    #[cfg(target_os = "macos")]
    {
        crate::paths::home().join("Library/Application Support/SHB-OcarinaTweaks/updates")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::paths::home().join(".cache"));
        base.join("SHB-OcarinaTweaks").join("updates")
    }
}

/// Where the new executable goes, and whether it may replace the running one.
pub fn update_path(version: &str) -> (PathBuf, bool) {
    let name = format!("SHB-OcarinaTweaks-{version}.exe");
    let app_dir = crate::paths::program_dir();
    if cfg!(windows) && writable(&app_dir) {
        return (app_dir.join(name), true);
    }
    (updates_dir().join(name), false)
}

/// Download `update` into `dest`, reporting `(done, total)` as it goes, and check the digest when GitHub has one.
pub fn download(
    update: &Update,
    dest: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf> {
    let url = update
        .asset_url
        .as_deref()
        .ok_or_else(|| Error::Update("no asset for this platform".into()))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut part = dest.as_os_str().to_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    let response = agent(120)
        .get(url)
        .header("User-Agent", UA)
        .call()
        .map_err(|e| Error::Update(format!("{e}")))?;
    let total = update.asset_size;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut file = std::fs::File::create(&part).map_err(|e| Error::io(&part, e))?;
    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0u8; 65536];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| Error::Update(format!("{e}")))?;
        if read == 0 {
            break;
        }
        use std::io::Write;
        file.write_all(&buffer[..read])
            .map_err(|e| Error::io(&part, e))?;
        hasher.update(&buffer[..read]);
        done += read as u64;
        progress(done, total);
    }
    drop(file);
    if let Some(digest) = &update.digest
        && let Some(want) = digest
            .split_once(':')
            .map(|(algo, want)| (algo.to_ascii_lowercase(), want.trim().to_string()))
        && want.0 == "sha256"
    {
        let got = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if got != want.1.to_ascii_lowercase() {
            let _ = std::fs::remove_file(&part);
            return Err(Error::Update(
                "the download does not match its checksum".into(),
            ));
        }
    }
    std::fs::rename(&part, dest).map_err(|e| Error::io(dest, e))?;
    Ok(dest.to_path_buf())
}

/// Replace the running executable with `downloaded` and start it again (Windows only).
pub fn apply_and_restart(downloaded: &Path) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let current = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return false,
    };
    let work = std::env::temp_dir().join(format!("shb-update-{}", std::process::id()));
    if std::fs::create_dir_all(&work).is_err() {
        return false;
    }
    let script = work.join("apply.ps1");
    let quote = |path: &Path| {
        path.to_string_lossy()
            .replace('\\', "/")
            .replace('\'', "''")
    };
    let body = format!(
        "$ErrorActionPreference = 'SilentlyContinue'\n\
         Wait-Process -Id {} -ErrorAction SilentlyContinue\n\
         Start-Sleep -Milliseconds 700\n\
         $ok = $false\n\
         for ($i = 0; $i -lt 40; $i++) {{\n\
         \x20   Copy-Item -LiteralPath '{}' -Destination '{}' -Force\n\
         \x20   if ($?) {{ $ok = $true; break }}\n\
         \x20   Start-Sleep -Milliseconds 500\n\
         }}\n\
         if ($ok) {{\n\
         \x20   Remove-Item -LiteralPath '{}' -Force\n\
         \x20   Start-Process -FilePath '{}'\n\
         }} else {{\n\
         \x20   Start-Process -FilePath '{}'\n\
         }}\n\
         Remove-Item -LiteralPath $PSCommandPath -Force\n",
        std::process::id(),
        quote(downloaded),
        quote(&current),
        quote(downloaded),
        quote(&current),
        quote(downloaded),
    );
    // PowerShell reads a BOM-marked file as UTF-8; without it, non-ASCII paths (a user name) break.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(body.as_bytes());
    if std::fs::write(&script, bytes).is_err() {
        return false;
    }
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
            &script.to_string_lossy(),
        ])
        .spawn()
        .is_ok()
}

/// Show `path` in the file manager.
pub fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

/// The releases page, for the "check by hand" case.
pub fn open_releases() {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", RELEASES_PAGE])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(RELEASES_PAGE)
            .spawn();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(RELEASES_PAGE)
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_numbers_and_order() {
        assert_eq!(version_numbers("v0.9.3"), vec![0, 9, 3]);
        assert_eq!(version_numbers("1.2"), vec![1, 2]);
        assert!(version_numbers("nightly").is_empty());
        assert!(is_newer("v0.10.0", "0.9.3"));
        assert!(is_newer("v0.9.4", "0.9.3"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.9.3", "0.9.3"));
        assert!(!is_newer("v0.9.2", "0.9.3"));
        assert!(!is_newer("nonsense", "0.9.3"));
    }

    #[test]
    fn the_platform_asset_names_are_the_python_ones() {
        #[cfg(windows)]
        assert_eq!(
            platform_assets(),
            &["SHB-OcarinaTweaks.exe", "SuperHeroBoostConfig.exe"]
        );
        assert!(!platform_assets().is_empty());
    }
}
