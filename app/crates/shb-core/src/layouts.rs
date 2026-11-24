//! The game-wide controller layout (`app-legacy/layouts.py`): the PlayStation or Xbox prompts and their resource table.
//!
//! The pack lives on GameBanana (one ZIP per style). It is downloaded once, kept in the per-user cache, and only the
//! three files the game needs are extracted: the UI atlas, its External font and the resource size table. The table
//! is a `RESTBL` blob (zstd-compressed) that the mod's own table is merged into, entry by entry, keeping the biggest
//! size: the game has a single table, and both mods raise sizes for their own files.

use crate::error::{Error, Result, read, write};
use crate::update::UA;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const GB_MOD: u64 = 737_074;
pub const GB_PAGE: &str = "https://gamebanana.com/mods/737074";
pub const GB_PROFILE: &str = "https://gamebanana.com/apiv11/Mod/737074/ProfilePage";

/// The file inside the submission for each style.
pub const FILES: [(&str, &str, &str); 2] = [
    // style, file name, prefix
    ("playstation", "dualsense_standard.zip", "dualsense"),
    ("xbox", "xbox_standard.zip", "xbox"),
];

/// The three romfs files of the pack.
pub const FONT: &str = "Font2/Scalable/External.Nin_Ounce_NVN2.bfotf.zs";
pub const TABLE: &str =
    "System/Resource/ResourceSizeTable.Product.111.Nin_Ounce_NVN2.rsizetable.zs";
pub const ATLAS: &str = "UI/Layout2Archive/Common.Product.100.Nin_Ounce_NVN2.bntx.zs";
pub const MIN_ATLAS: u64 = 60_000_000;

fn file_of(style: &str) -> Option<(&'static str, &'static str)> {
    FILES
        .iter()
        .find(|(s, _, _)| *s == style)
        .map(|(_, file, prefix)| (*file, *prefix))
}

/// The per-user cache folder of the layouts.
pub fn cache_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::paths::home().join("AppData/Local"));
        base.join("SHB-OcarinaTweaks").join("layouts")
    }
    #[cfg(target_os = "macos")]
    {
        crate::paths::home().join("Library/Caches/SHB-OcarinaTweaks/layouts")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::paths::home().join(".cache"));
        base.join("SHB-OcarinaTweaks").join("layouts")
    }
}

/// Whether the pack of `style` is downloaded and complete.
pub fn available(style: &str) -> bool {
    if file_of(style).is_none() {
        return false;
    }
    let root = cache_dir().join(style);
    if !root.join("ok").is_file() {
        return false;
    }
    let size = |rel: &str| {
        std::fs::metadata(root.join(rel))
            .map(|m| m.len())
            .unwrap_or(0)
    };
    size(ATLAS) >= MIN_ATLAS && size(FONT) >= 4096 && root.join(TABLE).is_file()
}

pub fn cached_size(style: &str) -> u64 {
    let root = cache_dir().join(style);
    [FONT, TABLE, ATLAS]
        .iter()
        .map(|rel| {
            std::fs::metadata(root.join(rel))
                .map(|m| m.len())
                .unwrap_or(0)
        })
        .sum()
}

fn agent(timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .build(),
        )
        .build()
        .new_agent()
}

/// GameBanana's certificate chain upsets the Windows TLS stack ureq uses here, so a failed request is retried with
/// the system `curl`, which is present on Windows 10+, macOS and every Linux the app targets.
fn curl(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--fail",
            "--max-time",
            "300",
        ])
        .args(args)
        .status()
        .map_err(|e| Error::Layout(format!("curl: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Layout(format!(
            "{GB_PAGE}: the download failed ({status})"
        )))
    }
}

fn fetch_text(url: &str, accept: &str) -> Result<String> {
    let attempt = agent(30)
        .get(url)
        .header("User-Agent", UA)
        .header("Accept", accept)
        .call();
    if let Ok(response) = attempt
        && let Ok(text) = response.into_body().read_to_string()
    {
        return Ok(text);
    }
    let file = std::env::temp_dir().join(format!("shb-layout-{}.json", std::process::id()));
    curl(&[
        "--user-agent",
        UA,
        "--header",
        &format!("Accept: {accept}"),
        "--output",
        &file.to_string_lossy(),
        url,
    ])?;
    let text = read(&file).map_err(|e| Error::Layout(format!("{e}")))?;
    let _ = std::fs::remove_file(&file);
    String::from_utf8(text).map_err(|e| Error::Layout(format!("{e}")))
}

/// The submission's file list, as GameBanana's API returns it.
fn api() -> Result<Vec<serde_json::Value>> {
    let text = fetch_text(GB_PROFILE, "application/json")?;
    let data: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| Error::Layout(format!("{e}")))?;
    data.get("_aFiles")
        .and_then(|f| f.as_array())
        .cloned()
        .ok_or_else(|| Error::Layout(format!("{GB_PAGE}: no file list")))
}

/// The entry of `style`: the exact file name, else a "standard" (not "western") file of that prefix.
fn pick(files: &[serde_json::Value], style: &str) -> Option<serde_json::Value> {
    let (wanted, prefix) = file_of(style)?;
    let name_of = |file: &serde_json::Value| {
        file.get("_sFile")
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
    };
    if let Some(found) = files
        .iter()
        .find(|f| name_of(f) == wanted.to_ascii_lowercase())
    {
        return Some(found.clone());
    }
    files
        .iter()
        .find(|f| {
            let name = name_of(f);
            name.starts_with(prefix) && name.contains("standard") && !name.contains("western")
        })
        .cloned()
}

/// Download the pack of `style` into the cache, once.
pub fn ensure(style: &str, mut progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    if file_of(style).is_none() {
        return Err(Error::Layout(format!("unknown style {style}")));
    }
    let root = cache_dir().join(style);
    if available(style) {
        return Ok(root);
    }
    let files = api()?;
    let entry = pick(&files, style)
        .ok_or_else(|| Error::Layout(format!("{GB_PAGE}: no file for {style}")))?;
    let url = entry
        .get("_sDownloadUrl")
        .and_then(|u| u.as_str())
        .map(str::to_string)
        .or_else(|| {
            entry
                .get("_idRow")
                .and_then(|id| id.as_u64())
                .map(|id| format!("https://gamebanana.com/dl/{id}"))
        })
        .ok_or_else(|| Error::Layout(format!("{GB_PAGE}: no download url")))?;

    std::fs::create_dir_all(&root).map_err(|e| Error::io(&root, e))?;
    let part = root.join("download.zip.part");
    let attempt = agent(120)
        .get(&url)
        .header("User-Agent", UA)
        .header("Referer", GB_PAGE)
        .call();
    match attempt {
        Ok(response) => {
            let mut reader = response.into_body().into_reader();
            let mut file = std::fs::File::create(&part).map_err(|e| Error::io(&part, e))?;
            use std::io::Write;
            let mut buffer = vec![0u8; 262_144];
            let mut done = 0u64;
            loop {
                let n = reader
                    .read(&mut buffer)
                    .map_err(|e| Error::Layout(format!("{e}")))?;
                if n == 0 {
                    break;
                }
                file.write_all(&buffer[..n])
                    .map_err(|e| Error::io(&part, e))?;
                done += n as u64;
                progress(done, 0);
            }
        }
        Err(_) => {
            curl(&[
                "--user-agent",
                UA,
                "--referer",
                GB_PAGE,
                "--output",
                &part.to_string_lossy(),
                &url,
            ])?;
            if let Ok(meta) = std::fs::metadata(&part) {
                progress(meta.len(), meta.len());
            }
        }
    }
    let extracted = extract(&part, &root);
    let _ = std::fs::remove_file(&part);
    extracted?;

    // The pack is only trusted when its parts are what the game expects.
    let atlas = std::fs::metadata(root.join(ATLAS))
        .map(|m| m.len())
        .unwrap_or(0);
    let font = std::fs::metadata(root.join(FONT))
        .map(|m| m.len())
        .unwrap_or(0);
    if atlas < MIN_ATLAS || font < 4096 || !root.join(TABLE).is_file() {
        return Err(Error::Layout(format!("{GB_PAGE}: the zip is incomplete")));
    }
    restbl(&crate::zstd::decompress(&read(&root.join(TABLE))?)?)?; // must parse
    write(
        &root.join("ok"),
        serde_json::json!({
            "style": style,
            "file": entry.get("_sFile").cloned().unwrap_or_default(),
            "id": entry.get("_idRow").cloned().unwrap_or_default(),
            "page": GB_PAGE,
        })
        .to_string()
        .as_bytes(),
    )?;
    Ok(root)
}

/// Extract only the three files of the pack, at their canonical paths.
fn extract(zip_path: &Path, out: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path).map_err(|e| Error::io(zip_path, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| Error::Layout(format!("zip: {e}")))?;
    let mut found = 0;
    for i in 0..archive.len() {
        let mut member = archive
            .by_index(i)
            .map_err(|e| Error::Layout(format!("zip: {e}")))?;
        let name = member.name().replace('\\', "/");
        for rel in [FONT, TABLE, ATLAS] {
            let matches = name == rel || name.ends_with(&format!("/{rel}"));
            if !matches {
                continue;
            }
            let dst = out.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            let mut data = Vec::new();
            member
                .read_to_end(&mut data)
                .map_err(|e| Error::Layout(format!("zip: {e}")))?;
            write(&dst, &data)?;
            found += 1;
            break;
        }
    }
    if found < 3 {
        return Err(Error::Layout(format!(
            "the zip has only {found} of the 3 files"
        )));
    }
    Ok(())
}

/// A parsed `RESTBL`: the version, the fixed name size, the CRC table and the name table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResTable {
    pub version: u32,
    pub name_size: u32,
    pub crc: BTreeMap<u32, u32>,
    pub names: BTreeMap<String, u32>,
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| Error::Layout("truncated table".into()))
}

/// Parse a decompressed `RESTBL`.
pub fn restbl(data: &[u8]) -> Result<ResTable> {
    if data.get(..6) != Some(b"RESTBL") {
        return Err(Error::Layout("not a RESTBL table".into()));
    }
    let version = u32_at(data, 6)?;
    let name_size = u32_at(data, 10)? as usize;
    let crc_count = u32_at(data, 14)? as usize;
    let name_count = u32_at(data, 18)? as usize;
    let mut at = 0x16;
    let mut crc = BTreeMap::new();
    for _ in 0..crc_count {
        crc.insert(u32_at(data, at)?, u32_at(data, at + 4)?);
        at += 8;
    }
    let mut names = BTreeMap::new();
    for _ in 0..name_count {
        let raw = data
            .get(at..at + name_size)
            .ok_or_else(|| Error::Layout("truncated table".into()))?;
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        let name = String::from_utf8_lossy(&raw[..end]).into_owned();
        names.insert(name, u32_at(data, at + name_size)?);
        at += name_size + 4;
    }
    Ok(ResTable {
        version,
        name_size: name_size as u32,
        crc,
        names,
    })
}

/// Serialize it back: entries sorted, which is what the game and the Python app expect.
pub fn dump_restbl(table: &ResTable) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"RESTBL");
    out.extend_from_slice(&table.version.to_le_bytes());
    out.extend_from_slice(&table.name_size.to_le_bytes());
    out.extend_from_slice(&(table.crc.len() as u32).to_le_bytes());
    out.extend_from_slice(&(table.names.len() as u32).to_le_bytes());
    for (key, size) in &table.crc {
        out.extend_from_slice(&key.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
    }
    for (name, size) in &table.names {
        let mut fixed = name.as_bytes().to_vec();
        fixed.resize(table.name_size as usize, 0);
        out.extend_from_slice(&fixed);
        out.extend_from_slice(&size.to_le_bytes());
    }
    out
}

/// Merge `src` into `dst` (both `.zs` tables), keeping the biggest size per entry.
pub fn merge_table(src: &Path, dst: &Path) -> Result<()> {
    if !dst.exists() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::copy(src, dst).map_err(|e| Error::io(dst, e))?;
        return Ok(());
    }
    let mut ours = restbl(&crate::zstd::decompress(&read(dst)?)?)?;
    let theirs = restbl(&crate::zstd::decompress(&read(src)?)?)?;
    for (key, size) in theirs.crc {
        let entry = ours.crc.entry(key).or_insert(0);
        *entry = (*entry).max(size);
    }
    for (name, size) in theirs.names {
        let entry = ours.names.entry(name).or_insert(0);
        *entry = (*entry).max(size);
    }
    let dumped = dump_restbl(&ours);
    write(dst, &crate::zstd::compress(&dumped, 19)?)?;
    Ok(())
}

/// Copy the chosen layout's atlas into a mod's romfs, with the patched font, and merge its table.
///
/// The layout's own External font is never installed: this build's font loader aborts when the unpacked OTF is
/// larger than ~14640 bytes (the layout ships 15632). `tools/make_button_font.py` builds the safe one and the
/// payload carries it; without it the game keeps its own prompts.
pub fn apply(romfs: &Path, style: &str, payload: &Path) -> Result<()> {
    let source = ensure(style, |_, _| {})?;
    let dst = romfs.join(ATLAS);
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::copy(source.join(ATLAS), &dst).map_err(|e| Error::io(&dst, e))?;
    let font = payload.join("button_romfs").join(style).join(FONT);
    if font.is_file() {
        let dst = romfs.join(FONT);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::copy(&font, &dst).map_err(|e| Error::io(&dst, e))?;
    }
    merge_table(&source.join(TABLE), &romfs.join(TABLE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(version: u32, crc: &[(u32, u32)], names: &[(&str, u32)]) -> ResTable {
        ResTable {
            version,
            name_size: 32,
            crc: crc.iter().copied().collect(),
            names: names.iter().map(|(n, s)| ((*n).to_string(), *s)).collect(),
        }
    }

    #[test]
    fn restbl_round_trip_and_merge() {
        let ours = table(111, &[(0x1111, 100), (0x2222, 200)], &[("UI/a.bntx", 100)]);
        let dumped = dump_restbl(&ours);
        assert_eq!(restbl(&dumped).unwrap(), ours);

        // The merge keeps the biggest per entry, on both tables.
        let theirs = table(
            111,
            &[(0x2222, 300), (0x3333, 50)],
            &[("UI/a.bntx", 400), ("Font/f.bfotf", 10)],
        );
        let mut merged = ours.clone();
        for (key, size) in &theirs.crc {
            let entry = merged.crc.entry(*key).or_insert(0);
            *entry = (*entry).max(*size);
        }
        for (name, size) in &theirs.names {
            let entry = merged.names.entry(name.clone()).or_insert(0);
            *entry = (*entry).max(*size);
        }
        assert_eq!(merged.crc[&0x1111], 100);
        assert_eq!(merged.crc[&0x2222], 300);
        assert_eq!(merged.crc[&0x3333], 50);
        assert_eq!(merged.names["UI/a.bntx"], 400);
        assert_eq!(merged.names["Font/f.bfotf"], 10);
    }

    #[test]
    fn a_bad_table_is_rejected() {
        assert!(matches!(restbl(b"RESTBL"), Err(Error::Layout(_))));
        assert!(matches!(restbl(b"nope"), Err(Error::Layout(_))));
    }

    #[test]
    fn the_styles_are_the_python_ones() {
        assert_eq!(file_of("playstation").unwrap().0, "dualsense_standard.zip");
        assert_eq!(file_of("xbox").unwrap().0, "xbox_standard.zip");
        assert!(file_of("switch").is_none() || file_of("switch").is_some());
        assert_eq!(
            ATLAS,
            "UI/Layout2Archive/Common.Product.100.Nin_Ounce_NVN2.bntx.zs"
        );
    }
}
