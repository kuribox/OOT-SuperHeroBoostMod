//! Rekarina's build, driven from this app.
//!
//! Rekarina is a static recompiler that ships its own pipeline in a `setup/<version>/` folder: a private interpreter,
//! `recomp/gen.py` (the lift), `tools/build_game.py` (the compile), its patch converters and its LLVM/Ninja. Those are
//! Rekarina's tools, so they are run as they are — but everything this app decides lives here: which module goes in,
//! the settings block, the extra `main` seeds our hooks need, the expected-hash table, the state files, and the page
//! of progress the window shows. `app/rekarina.py`'s `build()` is the reference this mirrors.
//!
//! Only writes under the game's `work` folder (`.shb-rekarina/`) and its status files; never into the setup.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::recomp_build::{BuildEvent, STAGE_RANGE};
use crate::rekarina::{MODULE, Rekarina, WORK};

/// The base the module is linked for, relative to `main` (Rekarina's layout: main, subsdk0, subsdk9, sdk).
pub const EXPECTED_BASE: u64 = 0x520_0000;
/// Where Rekarina's modules are placed in memory.
const MODULES: u64 = 0x70_0000_0000;

/// The tools of one Rekarina release, once they are known to be there.
#[derive(Debug, Clone)]
pub struct Setup {
    pub root: PathBuf,
    pub python: PathBuf,
    pub tools: PathBuf,
    pub recomp: PathBuf,
    pub llvm: PathBuf,
    pub ninja: PathBuf,
    pub deps: PathBuf,
    /// `data/pgo-oot.profdata` when the release ships one.
    pub pgo: Option<PathBuf>,
    pub base_patches: PathBuf,
    pub expected_src: PathBuf,
}

fn is_file(path: &Path) -> bool {
    path.is_file()
}

/// The Rekarina release that holds the tools we drive: `<parent>/setup/<version>` with `installer/oot.py`,
/// `recomp/gen.py` and `tools/build_game.py`. The newest version wins.
pub fn find_setup(game: &Path) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(parent) = game.parent() {
        roots.push(parent.to_path_buf());
        if let Some(grand) = parent.parent() {
            roots.push(grand.to_path_buf());
        }
    }
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let setup = root.join("setup");
        let Ok(entries) = std::fs::read_dir(&setup) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if is_file(&dir.join("installer").join("oot.py"))
                && is_file(&dir.join("recomp").join("gen.py"))
                && is_file(&dir.join("tools").join("build_game.py"))
            {
                found.push(dir);
            }
        }
    }
    found.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    found.pop()
}

/// Open the release's tools, or say exactly what is missing.
pub fn open_setup(root: &Path) -> Result<Setup> {
    let python: PathBuf = [
        root.join("python").join("python.exe"),
        root.join("python").join("bin").join("python3"),
        root.join("python").join("bin").join("python"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| Error::Build {
        key: "rekarina_no_setup".to_string(),
        arg: root.join("python").display().to_string(),
    })?;
    let tools = root.join("tools");
    let recomp = root.join("recomp");
    let llvm = root.join("llvm");
    let ninja = root
        .join("bin")
        .join(if cfg!(windows) { "ninja.exe" } else { "ninja" });
    let deps = root.join("runtime").join("deps");
    let base_patches = recomp.join("patches-oot-v111.txt");
    let expected_src = tools.join("data").join("expected-oot-v1.1.1.json");
    for needed in [
        tools.join("build_game.py"),
        recomp.join("gen.py"),
        base_patches.clone(),
        expected_src.clone(),
        llvm.join("bin")
            .join(if cfg!(windows) { "clang.exe" } else { "clang" }),
        ninja.clone(),
        deps.join("include"),
    ] {
        if !needed.exists() {
            return Err(Error::Build {
                key: "rekarina_no_setup".to_string(),
                arg: needed.display().to_string(),
            });
        }
    }
    let pgo = root.join("data").join("pgo-oot.profdata");
    Ok(Setup {
        root: root.to_path_buf(),
        python,
        tools,
        recomp,
        llvm,
        ninja,
        deps,
        pgo: pgo.is_file().then_some(pgo),
        base_patches,
        expected_src,
    })
}

fn sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A NSO's memory size (its `text`/`rodata`/`data` sizes plus their alignment), from its header.
fn nso_size(head: &[u8]) -> Result<u64> {
    if head.len() < 0x100 || &head[..4] != b"NSO0" {
        return Err(Error::Build {
            key: "rekarina_no_exefs".to_string(),
            arg: "NSO0".to_string(),
        });
    }
    // The module's footprint is its `data` segment (offset 0x30: memory size, decompressed size) plus the bss at
    // 0x3c, aligned up to a page — the three segments end there, so nothing else has to be added.
    let at = |offset: usize| -> u64 {
        u32::from_le_bytes([
            head[offset],
            head[offset + 1],
            head[offset + 2],
            head[offset + 3],
        ]) as u64
    };
    let bss = at(0x3c);
    let mem = at(0x34);
    let dec = at(0x38);
    Ok((mem + dec + bss).div_ceil(0x1000) * 0x1000)
}

/// Rekarina places each module on a 2 MiB boundary.
fn align(value: u64) -> u64 {
    align_up(value, 0x20_0000)
}

fn align_up(value: u64, to: u64) -> u64 {
    value.div_ceil(to) * to
}

/// Rekarina's base for `subsdk9` in this ExeFS, relative to `main`: the four modules are placed one after another in
/// the order main, subsdk0, subsdk9, sdk.
pub fn module_base(game: &Path, module: &[u8]) -> Result<u64> {
    let mut base = MODULES;
    let mut subsdk9 = None;
    for name in ["main", "subsdk0", MODULE, "sdk"] {
        let size = if name == MODULE {
            let size = nso_size(module)?;
            subsdk9 = Some(base);
            size
        } else {
            let path = game.join("exefs").join(name);
            let head = std::fs::read(&path).map_err(|e| Error::io(&path, e))?;
            nso_size(&head[..head.len().min(0x100)])?
        };
        base = align(base + size);
    }
    Ok(subsdk9.unwrap_or(0) - MODULES)
}

/// Every direct branch target in a `.text` segment (B, BL, B.cond, CBZ/CBNZ, TBZ/TBNZ).
pub fn direct_targets(text: &[u8], pc0: u64) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::new();
    let mut i = 0;
    while i + 4 <= text.len() {
        let word = u32::from_le_bytes([text[i], text[i + 1], text[i + 2], text[i + 3]]);
        let op = word >> 26;
        let imm: Option<i64> = match op {
            0x05 | 0x25 => Some(sign_extend(word & 0x03ff_ffff, 26)),
            0x2a | 0x34 | 0x35 => Some(sign_extend(word & 0x7ffff, 19)),
            0x36 | 0x37 => Some(sign_extend(word & 0x3fff, 14)),
            _ => None,
        };
        if let Some(imm) = imm {
            let target = pc0
                .wrapping_add(i as u64)
                .wrapping_add((imm as u64).wrapping_mul(4));
            out.push(target);
        }
        i += 4;
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn sign_extend(value: u32, bits: u32) -> i64 {
    let shift = 32 - bits;
    (((value << shift) as i32) >> shift) as i64
}

/// The `main` addresses our module branches to from outside its own text: mid-function resume points (a code hook
/// re-runs the original instruction and branches back). Rekarina's lifter only dispatches to function starts, so
/// `gen.py` has to know them as extra seeds.
pub fn external_main_seeds(game: &Path, module: &[u8]) -> Vec<u64> {
    let Ok(main_head) = std::fs::read(game.join("exefs").join("main")) else {
        return Vec::new();
    };
    let main_lo = MODULES;
    let main_hi = main_lo + nso_size(&main_head[..main_head.len().min(0x100)]).unwrap_or(0);
    // The module's own text: `0x10` is its offset, `0x18` its size (the third field of the header's first triple).
    let Some(header) = module.get(0x10..0x1c) else {
        return Vec::new();
    };
    let off = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let size = u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;
    let Some(text) = module.get(off..off + size) else {
        return Vec::new();
    };
    let mod_lo = main_lo + EXPECTED_BASE;
    let mod_hi = mod_lo + size as u64;
    direct_targets(text, mod_lo)
        .into_iter()
        .filter(|target| {
            *target >= main_lo && *target < main_hi && !(*target >= mod_lo && *target < mod_hi)
        })
        .collect()
}

/// `gen.py` emits in multiprocessing workers that re-import the module by name: the wrapper imports it as `gen` and
/// adds our extra `main` seeds. Rekarina's file is never edited.
fn write_gen_wrapper(work: &Path) -> Result<PathBuf> {
    let path = work.join("gen_rekarina.py");
    let script = r#"import os, sys
from pathlib import Path
_setup = os.environ.get('SHB_REKARINA_SETUP')
if _setup:
    sys.path.insert(0, str(Path(_setup) / 'recomp'))
    sys.path.insert(0, str(Path(_setup) / 'tools'))

def _main():
    import json
    import gen
    setup = Path(sys.argv[1]); seeds_file = Path(sys.argv[2]); args = sys.argv[3:]
    extra = {int(x) for x in json.loads(seeds_file.read_text())}
    orig = gen.Module.seeds
    def seeds(self):
        out = list(orig(self))
        if self.name == 'main':
            out += list(extra)
        return out
    gen.Module.seeds = seeds
    sys.argv = ['gen.py', *args]
    raise SystemExit(gen.main())

if __name__ == '__main__':
    _main()
"#;
    std::fs::write(&path, script).map_err(|e| Error::io(&path, e))?;
    Ok(path)
}

/// `build_game.py` adds `-DGAME_NS2 -DGAME_OOT` only when it picks the expected-hash file itself; we pass our own
/// table (the release's plus `subsdk9`), so the wrapper adds the same two flags before calling its `main()`.
fn write_build_wrapper(work: &Path) -> Result<PathBuf> {
    let path = work.join("build_game_rekarina.py");
    let script = r#"import importlib.util, sys
from pathlib import Path
setup = Path(sys.argv[1])
args = sys.argv[2:]
sys.path.insert(0, str(setup / 'tools'))
spec = importlib.util.spec_from_file_location('rek_build_game', setup / 'tools' / 'build_game.py')
mod = importlib.util.module_from_spec(spec)
sys.modules['rek_build_game'] = mod
spec.loader.exec_module(mod)
mod.RT_FLAGS.extend(['-DGAME_NS2', '-DGAME_OOT'])
mod.GEN_FLAGS.extend(['-DGAME_NS2', '-DGAME_OOT'])
sys.argv = ['build_game.py', *args]
raise SystemExit(mod.main())
"#;
    std::fs::write(&path, script).map_err(|e| Error::io(&path, e))?;
    Ok(path)
}

/// Rekarina's environment: unbuffered, UTF-8, no bytecode, the setup folder for the wrappers, LTO at run time, and
/// either the PGO profile or the hot-object list.
fn setup_env(setup: &Setup) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    for (key, value) in std::env::vars() {
        env.push((key, value));
    }
    for (key, value) in [
        ("PYTHONUNBUFFERED", "1".to_string()),
        ("PYTHONIOENCODING", "utf-8".to_string()),
        ("PYTHONUTF8", "1".to_string()),
        ("PYTHONDONTWRITEBYTECODE", "1".to_string()),
        ("REKARINA_RT_LTO", "1".to_string()),
        ("SHB_REKARINA_SETUP", setup.root.display().to_string()),
    ] {
        env.retain(|(existing, _)| existing != key);
        env.push((key.to_string(), value));
    }
    env.retain(|(key, _)| key != "REKARINA_PGO_RUNTIME_ONLY");
    if setup.pgo.is_none() {
        let hot = setup.tools.join("data").join("hot-oot-v111.txt");
        if hot.is_file() {
            env.retain(|(key, _)| key != "REKARINA_HOT_OBJS");
            env.push(("REKARINA_HOT_OBJS".to_string(), hot.display().to_string()));
        }
    } else {
        env.retain(|(key, _)| key != "REKARINA_HOT_OBJS");
    }
    env
}

fn stage_bounds(stage: &str) -> (f32, f32) {
    match stage {
        "read" => (0.00, 0.03),
        "lift" => (0.03, 0.16),
        "compile" => (0.16, 0.98),
        _ => STAGE_RANGE
            .iter()
            .find(|(name, _, _)| *name == stage)
            .map(|(_, lo, hi)| (*lo, *hi))
            .unwrap_or((0.0, 1.0)),
    }
}

/// Run one of Rekarina's tools, streaming its output through `on_line`; stops (and kills it) when `stop` is set.
fn run_tool(
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    stop: &Arc<AtomicBool>,
    mut on_line: impl FnMut(&str),
) -> Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    for (key, value) in env {
        cmd.env(key, value);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|err| Error::Build {
        key: "recomp_no_builder".to_string(),
        arg: format!("{}: {err}", program.display()),
    })?;
    let stderr = child.stderr.take().map(|stream| {
        std::thread::spawn(move || {
            BufReader::new(stream)
                .lines()
                .map_while(std::result::Result::ok)
                .collect::<Vec<String>>()
        })
    });
    let mut tail: Vec<String> = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if stop.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Build {
                    key: "recomp_cancelled".to_string(),
                    arg: String::new(),
                });
            }
            if !line.trim().is_empty() {
                tail.push(line.clone());
                if tail.len() > 40 {
                    tail.remove(0);
                }
                on_line(&line);
            }
        }
    }
    let status = child.wait().map_err(|err| Error::Build {
        key: "recomp_build_failed".to_string(),
        arg: err.to_string(),
    })?;
    if let Some(handle) = stderr
        && let Ok(lines) = handle.join()
    {
        for line in lines {
            if !line.trim().is_empty() {
                on_line(&line);
            }
        }
    }
    if stop.load(Ordering::Relaxed) {
        return Err(Error::Build {
            key: "recomp_cancelled".to_string(),
            arg: String::new(),
        });
    }
    if !status.success() {
        let from = tail.len().saturating_sub(10);
        return Err(Error::Build {
            key: "recomp_build_failed".to_string(),
            arg: tail[from..].join("\n"),
        });
    }
    Ok(())
}

/// Move the patch records that are not ours out of our mod's IPS into a sibling file.
///
/// A mod merger puts the ExeFS patches of every merged mod into one IPS. Rekarina's converter takes a patch file as a
/// whole: one record it cannot lift makes it reject the file, and our hooks go with it. The foreign records move to
/// `<build id>-other-mods.ips`, which the converter classifies and skips on its own.
pub fn split_foreign_ips(game: &Rekarina, payload: &Path) -> Result<usize> {
    const BUILD_ID: &str = "E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348";
    let path = game
        .root
        .join("mods")
        .join(crate::find::MOD_NAME)
        .join("exefs")
        .join(format!("{BUILD_ID}.ips"));
    let Ok(blob) = std::fs::read(&path) else {
        return Ok(0);
    };
    let Ok(records) = crate::ips::read(&blob) else {
        return Ok(0);
    };
    let ours_blob = std::fs::read(payload.join("rekarina").join("ours.ips"))
        .map_err(|e| Error::io(payload.join("rekarina").join("ours.ips"), e))?;
    let ours: Vec<u32> = crate::ips::read(&ours_blob)?
        .into_iter()
        .map(|(offset, _)| offset)
        .collect();
    if ours.is_empty() || !records.iter().any(|(offset, _)| ours.contains(offset)) {
        return Ok(0);
    }
    let mine: Vec<(u32, Vec<u8>)> = records
        .iter()
        .filter(|(o, _)| ours.contains(o))
        .cloned()
        .collect();
    let foreign: Vec<(u32, Vec<u8>)> = records
        .iter()
        .filter(|(o, _)| !ours.contains(o))
        .cloned()
        .collect();
    if foreign.is_empty() {
        return Ok(0);
    }
    let other = path.with_file_name(format!("{BUILD_ID}-other-mods.ips"));
    let mut keep: Vec<(u32, Vec<u8>)> = std::fs::read(&other)
        .ok()
        .and_then(|blob| crate::ips::read(&blob).ok())
        .unwrap_or_default();
    let seen: Vec<u32> = keep.iter().map(|(offset, _)| *offset).collect();
    keep.extend(
        foreign
            .iter()
            .filter(|(offset, _)| !seen.contains(offset))
            .cloned(),
    );
    std::fs::write(&other, write_ips32(&keep)).map_err(|e| Error::io(&other, e))?;
    std::fs::write(&path, write_ips32(&mine)).map_err(|e| Error::io(&path, e))?;
    Ok(foreign.len())
}

/// An IPS32 file: header, then `offset` (4 bytes), `size` (2 bytes) and the data of every record.
fn write_ips32(records: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        16 + records
            .iter()
            .map(|(_, data)| data.len() + 6)
            .sum::<usize>(),
    );
    out.extend_from_slice(b"IPS32");
    for (offset, data) in records {
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
    }
    out.extend_from_slice(b"EEOF");
    out
}

/// The game folder of a Rekarina install, checking what the lift needs before any work is done.
fn check_exefs(game: &Rekarina, module: &[u8]) -> Result<()> {
    let exefs = game.root.join("exefs");
    for name in ["main", "subsdk0", "sdk"] {
        if !exefs.join(name).is_file() {
            return Err(Error::Build {
                key: "rekarina_no_exefs".to_string(),
                arg: exefs.join(name).display().to_string(),
            });
        }
    }
    let base = module_base(&game.root, module)?;
    if base != EXPECTED_BASE {
        return Err(Error::Build {
            key: "rekarina_bad_base".to_string(),
            arg: format!("main+{base:#x} (this game needs main+{EXPECTED_BASE:#x})"),
        });
    }
    Ok(())
}

/// Copy the ExeFS the lift reads into the work folder, with our module in it.
fn stage_exefs(game: &Rekarina, work: &Path, module: &[u8]) -> Result<PathBuf> {
    let exefs = work.join("exefs");
    if exefs.exists() {
        let _ = std::fs::remove_dir_all(&exefs);
    }
    std::fs::create_dir_all(&exefs).map_err(|e| Error::io(&exefs, e))?;
    for name in ["main", "main.npdm", "rtld", "sdk", "subsdk0"] {
        let src = game.root.join("exefs").join(name);
        if src.is_file() {
            let dst = exefs.join(name);
            std::fs::copy(&src, &dst).map_err(|e| Error::io(&dst, e))?;
        }
    }
    let dst = exefs.join(MODULE);
    std::fs::write(&dst, module).map_err(|e| Error::io(&dst, e))?;
    Ok(exefs)
}

/// Rebuild the game with our module and return the new executable.
///
/// The steps are Rekarina's own: its patch converters turn the mods' ExeFS patches and cheats into a patch set, its
/// lifter translates the modules (ours among them), its compiler builds them, and the result is an executable that
/// knows our module's hash.
pub fn build(
    game: &Rekarina,
    settings: &crate::settings::Settings,
    payload: &Path,
    stop: Arc<AtomicBool>,
    mut report: impl FnMut(BuildEvent),
) -> Result<PathBuf> {
    let setup_root = find_setup(&game.root).ok_or_else(|| Error::Build {
        key: "rekarina_no_setup".to_string(),
        arg: game
            .root
            .parent()
            .map(|parent| parent.join("setup").display().to_string())
            .unwrap_or_default(),
    })?;
    let setup = open_setup(&setup_root)?;
    let work = game.root.join(WORK);
    std::fs::create_dir_all(&work).map_err(|e| Error::io(&work, e))?;

    let module_src = payload.join("rekarina").join(MODULE);
    let module = std::fs::read(&module_src).map_err(|e| Error::io(&module_src, e))?;
    check_exefs(game, &module)?;

    let mut stage = "read".to_string();
    let progress = |report: &mut dyn FnMut(BuildEvent), stage: &str, text: &str, value: f32| {
        report(BuildEvent::Progress {
            fraction: value,
            text: text.to_string(),
            stage: stage.to_string(),
        });
    };
    match split_foreign_ips(game, payload) {
        Ok(0) => {}
        Ok(moved) => report(BuildEvent::Note(format!(
            "Rekarina: {moved} patch record(s) of other mods moved out of our IPS (the converter takes a whole file)"
        ))),
        Err(err) => report(BuildEvent::Note(format!(
            "Rekarina: could not separate the other mods' patches from ours: {err}"
        ))),
    }
    progress(&mut report, &stage, "staging the ExeFS", 0.0);
    let exefs = stage_exefs(game, &work, &module)?;
    let mut fraction = stage_bounds("read").1;
    stage = "lift".to_string();
    progress(&mut report, &stage, "translating the game", fraction);

    let env = setup_env(&setup);
    let mods_root = game.root.join("mods");
    let status_dir = game.root.join("build-cache");
    std::fs::create_dir_all(&status_dir).map_err(|e| Error::io(&status_dir, e))?;
    let mut patches = setup.base_patches.clone();
    let _ = settings;

    // The mods' cheats and ExeFS patches become a patch set beside the port's own, as Rekarina's Lift does.
    let has_inputs = std::fs::read_dir(&mods_root)
        .map(|entries| {
            entries.flatten().any(|entry| {
                let dir = entry.path();
                dir.is_dir()
                    && ["exefs", "romfs", "cheats"]
                        .iter()
                        .any(|sub| dir.join(sub).is_dir())
            })
        })
        .unwrap_or(false);
    if has_inputs {
        let out_cheats = work.join("patches-cheats.txt");
        let args: Vec<String> = vec![
            "-B".into(),
            setup.tools.join("cheats.py").display().to_string(),
            "--exefs".into(),
            exefs.display().to_string(),
            "--mods".into(),
            mods_root.display().to_string(),
            "--patches".into(),
            patches.display().to_string(),
            "--out".into(),
            out_cheats.display().to_string(),
            "--status".into(),
            status_dir.join("cheats.status").display().to_string(),
        ];
        match run_tool(&setup.python, &args, &setup.root, &env, &stop, |line| {
            report(BuildEvent::Line(line.to_string()))
        }) {
            Ok(()) => patches = out_cheats,
            Err(err) if matches!(&err, Error::Build { key, .. } if key == "recomp_cancelled") => {
                return Err(err);
            }
            Err(_) => {
                let _ = std::fs::remove_file(status_dir.join("cheats.status"));
                report(BuildEvent::Note(
                    "Rekarina: the cheats could not be converted; continuing without them"
                        .to_string(),
                ));
                patches = setup.base_patches.clone();
            }
        }
        let out_exefs = work.join("patches-exefs.txt");
        let args: Vec<String> = vec![
            "-B".into(),
            setup.tools.join("exefs_patches.py").display().to_string(),
            "--exefs".into(),
            exefs.display().to_string(),
            "--mods".into(),
            mods_root.display().to_string(),
            "--patches".into(),
            patches.display().to_string(),
            "--out".into(),
            out_exefs.display().to_string(),
            "--status".into(),
            status_dir.join("exefs.status").display().to_string(),
        ];
        let mut why: Vec<String> = Vec::new();
        let done = run_tool(&setup.python, &args, &setup.root, &env, &stop, |line| {
            if line.starts_with("exefs: unsupported") {
                why.push(line.to_string());
            }
            report(BuildEvent::Line(line.to_string()));
        });
        match done {
            Ok(()) => {
                if !has_our_patch(&out_exefs) {
                    let _ = std::fs::remove_file(status_dir.join("exefs.status"));
                    return Err(Error::Build {
                        key: "rekarina_patch_failed".to_string(),
                        arg: why.into_iter().take(3).collect::<Vec<_>>().join("\n"),
                    });
                }
                patches = out_exefs;
            }
            Err(err) => {
                let _ = std::fs::remove_file(status_dir.join("exefs.status"));
                return Err(err);
            }
        }
    } else {
        let _ = std::fs::remove_file(status_dir.join("exefs.status"));
        let _ = std::fs::remove_file(status_dir.join("cheats.status"));
    }

    // The lift: our extra seeds, then gen.py inside the wrapper.
    let seeds = external_main_seeds(&game.root, &module);
    report(BuildEvent::Note(format!(
        "Rekarina: {} extra main seeds (mid-function resume points of the hooks)",
        seeds.len()
    )));
    let seeds_file = work.join("main_extra_seeds.json");
    let json: Vec<String> = seeds.iter().map(|seed| seed.to_string()).collect();
    std::fs::write(&seeds_file, format!("[{}]", json.join(",")))
        .map_err(|e| Error::io(&seeds_file, e))?;
    let gen_runner = write_gen_wrapper(&work)?;
    let gen_out = work.join("gen");
    let modules = format!("main,subsdk0,sdk,{MODULE}");
    let (lo, hi) = stage_bounds("lift");
    let args: Vec<String> = vec![
        "-B".into(),
        gen_runner.display().to_string(),
        setup.root.display().to_string(),
        seeds_file.display().to_string(),
        "--exefs".into(),
        exefs.display().to_string(),
        "--out".into(),
        gen_out.display().to_string(),
        "--modules".into(),
        modules,
        "--patches".into(),
        patches.display().to_string(),
    ];
    let lift_range = hi - lo;
    run_tool(&setup.python, &args, &setup.root, &env, &stop, |line| {
        let low = line.to_lowercase();
        let value = if low.contains("emitting") || low.contains("emitted") {
            lo + lift_range * 0.7
        } else if line
            .split(':')
            .next()
            .is_some_and(|head| head.split_whitespace().count() == 1)
            && line.contains(" functions")
        {
            lo + lift_range * 0.5
        } else {
            fraction
        };
        fraction = value;
        report(BuildEvent::Progress {
            fraction: value,
            text: String::new(),
            stage: "lift".into(),
        });
    })?;

    // The layout the lifter wrote has to agree with the base the module was linked for.
    if let Ok(text) = std::fs::read_to_string(gen_out.join("layout.json"))
        && let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(entries) = doc.as_array()
        && let Some(base) = entries
            .iter()
            .find(|entry| entry.get("name").and_then(|n| n.as_str()) == Some(MODULE))
            .and_then(|entry| entry.get("base").and_then(|b| b.as_str()))
        && let Ok(parsed) = u64::from_str_radix(base.trim_start_matches("0x"), 16)
    {
        let got = parsed.wrapping_sub(MODULES);
        if got != EXPECTED_BASE {
            return Err(Error::Build {
                key: "rekarina_bad_base".to_string(),
                arg: format!("main+{EXPECTED_BASE:#x}, the lifter placed it at main+{got:#x}"),
            });
        }
    }

    // The expected hashes: the release's own table plus our module (the loader checks it at run time).
    let expected_text = std::fs::read_to_string(&setup.expected_src)
        .map_err(|e| Error::io(&setup.expected_src, e))?;
    let mut expected: serde_json::Value =
        serde_json::from_str(&expected_text).map_err(|e| Error::Build {
            key: "rekarina_no_setup".to_string(),
            arg: e.to_string(),
        })?;
    if !expected.is_object() {
        expected = serde_json::json!({});
    }
    let object = expected.as_object_mut().expect("an object");
    let hashes = object
        .entry("exefs_sha256")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(hashes) = hashes.as_object_mut() {
        hashes.insert(MODULE.to_string(), serde_json::json!(sha256(&module)));
    }
    let work_expected = work.join("expected.json");
    std::fs::write(&work_expected, expected.to_string())
        .map_err(|e| Error::io(&work_expected, e))?;

    stage = "compile".to_string();
    let (lo, hi) = stage_bounds("compile");
    file_progress(&mut report, &stage, "compiling", lo);
    let build_runner = write_build_wrapper(&work)?;
    let out = work.join("build");
    let mut args: Vec<String> = vec![
        "-B".into(),
        build_runner.display().to_string(),
        setup.root.display().to_string(),
        "--version".into(),
        "oot".into(),
        "--gen".into(),
        gen_out.display().to_string(),
        "--out".into(),
        out.display().to_string(),
        "--toolchain".into(),
        setup.llvm.display().to_string(),
        "--ninja".into(),
        setup.ninja.display().to_string(),
        "--deps".into(),
        setup.deps.display().to_string(),
        "--exefs".into(),
        game.root.join("exefs").display().to_string(),
        "--no-romfs".into(),
        "--expected".into(),
        work_expected.display().to_string(),
    ];
    if let Some(pgo) = &setup.pgo {
        args.push("--pgo".into());
        args.push(pgo.display().to_string());
    }
    if cpu_is_v2() {
        args.push("--cpu".into());
        args.push("v2".into());
    }
    let compile_range = hi - lo;
    run_tool(&setup.python, &args, &setup.root, &env, &stop, |line| {
        // Ninja prints `[done/total]`; that is the compile's own progress.
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('[')
            && let Some((done, total)) = rest.split_once('/')
            && let (Ok(done), Ok(total)) = (
                done.trim().parse::<f32>(),
                total.split(']').next().unwrap_or("0").trim().parse::<f32>(),
            )
        {
            let value = lo + compile_range * (done / total.max(1.0));
            report(BuildEvent::Progress {
                fraction: value,
                text: String::new(),
                stage: "compile".into(),
            });
        }
        report(BuildEvent::Line(trimmed.to_string()));
    })?;

    let exe = out.join(if cfg!(windows) {
        "rekarina.exe"
    } else {
        "rekarina"
    });
    if !exe.is_file() {
        return Err(Error::Build {
            key: "recomp_build_failed".to_string(),
            arg: "rekarina.exe was not produced".to_string(),
        });
    }
    report(BuildEvent::Progress {
        fraction: 1.0,
        text: "done".to_string(),
        stage: "done".to_string(),
    });
    Ok(exe)
}

fn file_progress(report: &mut impl FnMut(BuildEvent), stage: &str, text: &str, fraction: f32) {
    report(BuildEvent::Progress {
        fraction,
        text: text.to_string(),
        stage: stage.to_string(),
    });
}

/// The converted patch table holds a set for our IPS (the converter names the mod in its comment).
fn has_our_patch(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    text.contains(&format!("# {}/exefs/", crate::find::MOD_NAME)) && text.contains("set mod_")
}

/// `avx2` in this machine's CPU: Rekarina compiles for `v2` when it is missing.
fn cpu_is_v2() -> bool {
    match std::env::var("REKARINA_CPU").as_deref() {
        Ok("v2") => true,
        Ok("v3") => false,
        _ => {
            !cfg!(windows)
                && !std::fs::read_to_string("/proc/cpuinfo")
                    .map(|text| text.contains(" avx2"))
                    .unwrap_or(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_branch_scanner_finds_direct_targets() {
        // bl +8 at 0x1000 -> 0x1008; b +0 at 0x1004 -> 0x1004; an add is not a branch.
        let mut text = Vec::new();
        text.extend_from_slice(&0x9400_0002u32.to_le_bytes()); // bl +8
        text.extend_from_slice(&0x1400_0000u32.to_le_bytes()); // b +0
        text.extend_from_slice(&0x9100_0400u32.to_le_bytes()); // add x0, x0, #1 (not a branch)
        let targets = direct_targets(&text, 0x1000);
        assert_eq!(targets, vec![0x1004, 0x1008]);
    }

    #[test]
    fn the_nso_size_comes_from_the_header() {
        let mut head = vec![0u8; 0x100];
        head[..4].copy_from_slice(b"NSO0");
        head[0x34..0x38].copy_from_slice(&0x800u32.to_le_bytes()); // data, in memory
        head[0x38..0x3c].copy_from_slice(&0x100u32.to_le_bytes()); // data, decompressed
        head[0x3c..0x40].copy_from_slice(&0x300u32.to_le_bytes()); // bss
        assert_eq!(nso_size(&head).unwrap(), 0x1000);
    }

    #[test]
    fn modules_are_placed_on_two_mib_boundaries() {
        assert_eq!(align(0x3c461d0), 0x3e00000);
        assert_eq!(align(0x200000), 0x200000);
    }

    #[test]
    fn the_cpu_answer_is_one_of_the_two() {
        // It only has to answer; the choice is the environment's or the machine's.
        let _ = cpu_is_v2();
    }

    /// Runs against the Rekarina install on this machine (skipped elsewhere): the setup beside the game is found and
    /// every tool the lift needs is there.
    #[test]
    fn the_installed_setup_is_found_and_opened() {
        let game = Path::new(r"F:\Switch2\rekarina\game");
        if !game.is_dir() {
            return;
        }
        let setup = find_setup(game).expect("a setup beside the game");
        let opened = open_setup(&setup).expect("the tools of the release");
        assert!(opened.python.is_file(), "{}", opened.python.display());
        assert!(opened.base_patches.is_file());
        assert!(opened.expected_src.is_file());
        assert!(opened.ninja.is_file());
    }

    /// The base the port computes for the real game must be the one the module was linked for.
    #[test]
    fn the_real_game_puts_the_module_where_the_module_expects_it() {
        let game = Path::new(r"F:\Switch2\rekarina\game");
        let module = Path::new(r"F:\project\super-hero-boost\app\payload\rekarina\subsdk9");
        if !game.is_dir() || !module.is_file() {
            return;
        }
        let data = std::fs::read(module).unwrap();
        let base = module_base(game, &data).expect("the ExeFS sizes");
        assert_eq!(base, EXPECTED_BASE, "main+{base:#x}");
    }

    /// Our hooks branch into `main` from the module: those sites have to come out as seeds, inside `main`.
    #[test]
    fn the_real_module_yields_seeds_inside_main() {
        let game = Path::new(r"F:\Switch2\rekarina\game");
        let module = Path::new(r"F:\project\super-hero-boost\app\payload\rekarina\subsdk9");
        if !game.is_dir() || !module.is_file() {
            return;
        }
        let data = std::fs::read(module).unwrap();
        let seeds = external_main_seeds(game, &data);
        let main_head = std::fs::read(game.join("exefs").join("main")).unwrap();
        let hi = MODULES + nso_size(&main_head[..0x100]).unwrap();
        // The same count the reference (`app/rekarina.py`) gives for this module: 74 mid-function resume points.
        // A module that changes them is a module that changed its hooks — update the number with it.
        assert_eq!(seeds.len(), 74, "{seeds:?}");
        assert!(
            seeds.iter().all(|seed| *seed >= MODULES && *seed < hi),
            "{seeds:?}"
        );
    }
}
