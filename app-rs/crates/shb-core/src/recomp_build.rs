//! Rebuilding a static recompilation (Longshot, Hookshot, ReLink), in Rust.
//!
//! This is the driver of `app/recomp.py`'s `build()`: it finds the builder, works out how the game was built (DLSS,
//! FSR, XeSS) so the rebuild does not silently change its renderer, hands the builder's own `nx2/make_game.py` its
//! arguments and environment, and reads the `@@stage` / `@@progress` protocol it prints.
//!
//! What is *not* here, on purpose: the recompiler itself. `make_game.py` and the toolchain it drives belong to
//! Longshot/ReLink; they are external programs, the way a C project's compiler is. Our code — everything this app
//! decides — lives here, so the app is one executable.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use crate::error::{Error, Result};
use crate::game::{self, Flavor};

/// Where each stage of a build sits on the 0..1 bar (the builder reports progress inside a stage, not overall).
pub const STAGE_RANGE: &[(&str, f32, f32)] = &[
    ("read", 0.00, 0.03),
    ("lift", 0.03, 0.15),
    ("compile", 0.15, 0.90),
    ("shaders", 0.90, 0.99),
];

/// One thing the build has to say: how far it is, in which stage, and when it only has a line of output.
#[derive(Debug, Clone)]
pub enum BuildEvent {
    /// `fraction` in 0..=1, the text to show and the stage's name.
    Progress {
        fraction: f32,
        text: String,
        stage: String,
    },
    /// A line of the builder's own output.
    Line(String),
    /// Something worth telling the user (the renderer reused, the game closed…).
    Note(String),
}

fn stage_lo(name: &str) -> f32 {
    STAGE_RANGE
        .iter()
        .find(|(stage, _, _)| *stage == name)
        .map(|(_, lo, _)| *lo)
        .unwrap_or(0.0)
}

fn stage_hi(name: &str) -> f32 {
    STAGE_RANGE
        .iter()
        .find(|(stage, _, _)| *stage == name)
        .map(|(_, _, hi)| *hi)
        .unwrap_or(1.0)
}

/// The prefix of the environment variables the builder reads: `LONGSHOT_*` or `RELINK_*`.
pub fn env_prefix(flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::ReLink => "RELINK",
        _ => "LONGSHOT",
    }
}

/// How the game was built, as its launcher remembers it.
///
/// A rebuild that ignores these silently changes the renderer: the DLSS path is compiled out, the ngx runtime is
/// removed and the game can stop on a setting it no longer has. ReLink keeps them in `launcher.ini`; the Longshot and
/// Hookshot launchers in `settings.json`, with the per-folder entry of `builds.json`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LauncherOptions {
    pub dlss: bool,
    pub dlss_license: bool,
    pub fsr: bool,
    pub fsr_license: bool,
    pub xess: bool,
    pub xess_license: bool,
    /// The `ngx-sdk` folder the launcher keeps beside its settings.
    pub ngx: Option<PathBuf>,
    /// The file the settings were read from.
    pub launcher: Option<PathBuf>,
}

/// The per-user folders a launcher can live in on this OS (the ones that exist).
pub fn data_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for var in ["LOCALAPPDATA", "APPDATA"] {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
        {
            out.push(PathBuf::from(value));
        }
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    for (var, fallback) in [
        ("XDG_DATA_HOME", home.join(".local").join("share")),
        ("XDG_CONFIG_HOME", home.join(".config")),
    ] {
        match std::env::var(var) {
            Ok(value) if !value.is_empty() => out.push(PathBuf::from(value)),
            _ => out.push(fallback),
        }
    }
    #[cfg(target_os = "macos")]
    {
        out.push(home.join("Library").join("Application Support"));
        out.push(home.join("Library").join("Preferences"));
    }
    if cfg!(all(unix, not(target_os = "macos"))) {
        out.push(home.join(".local").join("state"));
    }
    let mut found: Vec<PathBuf> = Vec::new();
    for path in out {
        if path.is_dir() && !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

fn launcher_names(flavor: Flavor) -> &'static [&'static str] {
    match flavor {
        Flavor::ReLink => &[
            "ReLink",
            "Relink",
            "relink",
            "ReLinkLauncher",
            "ReLink Launcher",
            "relink-launcher",
        ],
        Flavor::Hookshot => &["HookshotLauncher", "Hookshot Launcher", "hookshot-launcher"],
        _ => &["LongshotLauncher", "Longshot Launcher", "longshot-launcher"],
    }
}

fn launcher_key(flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::ReLink => "relink",
        Flavor::Hookshot => "hookshot",
        _ => "longshot",
    }
}

/// Every data folder of this flavor's launcher. A launcher packaged under another name is found by its key.
pub fn launcher_dirs(flavor: Flavor) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for base in data_dirs() {
        for name in launcher_names(flavor) {
            let path = base.join(name);
            if path.is_dir() && !found.contains(&path) {
                found.push(path);
            }
        }
    }
    if !found.is_empty() {
        return found;
    }
    let key = launcher_key(flavor);
    for base in data_dirs() {
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| {
                    n.to_string_lossy()
                        .to_lowercase()
                        .replace([' ', '-', '_'], "")
                })
                .unwrap_or_default();
            if name.contains(key) && !found.contains(&path) {
                found.push(path);
            }
        }
    }
    found
}

/// The `key=value` of a launcher ini (`launcher.ini`, `longshot_options.ini`), keys lower-cased.
fn ini_values(path: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_lowercase(), value.trim().to_string()))
        })
        .collect()
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(flag) => *flag,
        Value::Number(num) => num.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(text) => {
            !text.is_empty() && text != "0" && !text.eq_ignore_ascii_case("false")
        }
        _ => false,
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Read how the game was built. The launcher that built this flavor is the one that knows: a Hookshot game reads
/// HookshotLauncher, a Longshot game LongshotLauncher, a ReLink game `launcher.ini`.
pub fn launcher_options(game: &Path, flavor: Flavor) -> LauncherOptions {
    let mut opt = LauncherOptions::default();
    let roots = launcher_dirs(flavor);
    if flavor == Flavor::ReLink {
        let ini = roots
            .iter()
            .map(|base| base.join("launcher.ini"))
            .find(|ini| ini.is_file());
        opt.launcher = ini.clone();
        if let Some(ini) = &ini {
            for (key, value) in ini_values(ini) {
                if key == "nvidia_dlss" {
                    opt.dlss = value == "1";
                }
            }
        }
        let base = ini
            .as_ref()
            .and_then(|ini| ini.parent().map(Path::to_path_buf))
            .or_else(|| roots.first().cloned());
        if let Some(base) = base {
            opt.ngx = Some(base.join("ngx-sdk"));
        }
    } else if let Some(base) = roots
        .iter()
        .find(|base| base.join("settings.json").is_file())
        .or_else(|| roots.first())
    {
        // The settings that decide DLSS/FSR belong to the launcher that built this flavor.
        let settings_path = base.join("settings.json");
        opt.launcher = settings_path.is_file().then(|| settings_path.clone());
        let settings = read_json(&settings_path).unwrap_or(Value::Null);
        for (slot, key) in [
            (&mut opt.dlss, "dlss"),
            (&mut opt.dlss_license, "dlssLicense"),
            (&mut opt.fsr, "fsr"),
            (&mut opt.fsr_license, "fsrLicense"),
            (&mut opt.xess, "xess"),
            (&mut opt.xess_license, "xessLicense"),
        ] {
            *slot = settings.get(key).map(truthy).unwrap_or(false);
        }
        // `builds.json` keeps one entry per built folder and overrides the launcher's global switches.
        if let Some(builds) = read_json(&base.join("builds.json"))
            .and_then(|value| value.get("builds").and_then(Value::as_array).cloned())
        {
            let wanted = game.canonicalize().unwrap_or_else(|_| game.to_path_buf());
            for entry in builds {
                let folder = entry
                    .get("folder")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let folder = Path::new(folder);
                let folder = folder
                    .canonicalize()
                    .unwrap_or_else(|_| folder.to_path_buf());
                if folder != wanted {
                    continue;
                }
                for (slot, key) in [
                    (&mut opt.dlss, "dlss"),
                    (&mut opt.fsr, "fsr"),
                    (&mut opt.xess, "xess"),
                ] {
                    if let Some(value) = entry.get(key) {
                        *slot = truthy(value);
                    }
                }
            }
        }
        opt.ngx = Some(base.join("ngx-sdk"));
    }
    if opt.ngx.as_ref().map(|ngx| !ngx.is_dir()).unwrap_or(true) {
        opt.ngx = None;
    }
    // Last resort: the game's own options (the launcher may not be installed here).
    if !(opt.dlss || opt.fsr || opt.xess) {
        let names: &[&str] = match flavor {
            Flavor::ReLink => &["relink_options.ini"],
            Flavor::Hookshot => &["hookshot_options.ini"],
            _ => &["longshot_options.ini"],
        };
        for name in names {
            let ini = game.join(name);
            if !ini.is_file() {
                continue;
            }
            for (key, value) in ini_values(&ini) {
                if (key == "dlss" || key == "nvidia_dlss") && value == "1" {
                    opt.dlss = true;
                }
            }
            break;
        }
    }
    opt
}

/// The interpreter that runs `make_game.py`: the builder's own (`tools/python/python.exe`, …) or a system python.
///
/// The builder ships its own interpreter because it needs its own packages; using the system one would run a
/// different environment than the one Longshot/ReLink tested.
pub fn builder_python(builder: &Path) -> PathBuf {
    let tools = builder.join("tools");
    for candidate in [
        tools.join("python").join("python.exe"),
        tools.join("python").join("python"),
        tools.join("python").join("bin").join("python3"),
        tools.join("python").join("bin").join("python"),
        tools.join("venv").join("bin").join("python"),
        tools.join("venv").join("bin").join("python3"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    // A packaged app cannot fall back to "itself" as an interpreter: look for one in PATH.
    for name in ["python3", "python", "py"] {
        if let Some(found) = which(name) {
            return found;
        }
    }
    PathBuf::from("python")
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };
    for dir in std::env::split_paths(&path) {
        for ext in exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The arguments `make_game.py` is run with, as `shb` decides them.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The folder the builder runs from (`<builder>/tools`).
    pub cwd: PathBuf,
    pub work: Option<PathBuf>,
    /// Whether the builder has to fetch the ngx SDK first.
    pub fetch_ngx: bool,
}

/// Work out how to call the builder. `err` carries the i18n key of what is missing, if anything.
pub fn plan(
    game: &Path,
    builder: &Path,
    flavor: Flavor,
    work: Option<&Path>,
) -> std::result::Result<(BuildPlan, Vec<BuildEvent>), (String, String)> {
    let mut notes: Vec<BuildEvent> = Vec::new();
    let tools = builder.join("tools");
    if !tools.join("nx2").join("make_game.py").exists() {
        return Err((
            "recomp_no_builder".to_string(),
            builder.display().to_string(),
        ));
    }
    let work = work
        .map(Path::to_path_buf)
        .or_else(|| Some(game.join(".shb-work")));
    if let Some(dir) = &work {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut args = vec![
        "-I".to_string(),
        "nx2/make_game.py".to_string(),
        "--update".to_string(),
        game.display().to_string(),
        "--jobs".to_string(),
        "0".to_string(),
    ];
    if let Some(dir) = &work {
        args.push("--work".to_string());
        args.push(dir.display().to_string());
    }
    let opt = launcher_options(game, flavor);
    if let Some(note) = reuse_renderer_note(game, builder, work.as_deref(), &mut notes) {
        notes.push(BuildEvent::Note(note));
    }
    let mut fetch_ngx = false;
    if opt.dlss {
        if let Some(ngx) = &opt.ngx {
            let name = ngx
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            notes.push(BuildEvent::Note(format!("keeping DLSS ({name})")));
        } else if opt.dlss_license {
            args.push("--fetch-ngx-sdk".to_string());
            args.push("--accept-nvidia-license".to_string());
            fetch_ngx = true;
        } else {
            let what = opt
                .launcher
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| env_prefix(flavor).to_string());
            return Err(("recomp_no_dlss".to_string(), what));
        }
    }
    if flavor != Flavor::ReLink && opt.fsr {
        args.push("--fsr".to_string());
    }
    let plan = BuildPlan {
        program: builder_python(builder),
        args,
        cwd: tools,
        work,
        fetch_ngx,
    };
    Ok((plan, notes))
}

/// The environment the builder needs: the packaged CPM cache, the renderer when it cannot compile its own, and the
/// DLSS SDK the game was built with.
pub fn build_env(
    game: &Path,
    builder: &Path,
    plan: &BuildPlan,
    flavor: Flavor,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    if let Some(value) = std::env::var_os("PATH") {
        env.push(("PATH".to_string(), value.to_string_lossy().to_string()));
    }
    for (key, value) in std::env::vars() {
        if key.ends_with("_NGX_SDK") || key.ends_with("_VK_PREBUILT") || key == "RX_CPM" {
            env.push((key, value));
        }
    }
    let prefix = env_prefix(flavor);
    let cpm = builder.join("tools").join("cpm");
    if cpm.is_dir() && !env.iter().any(|(key, _)| key == "RX_CPM") {
        env.push(("RX_CPM".to_string(), cpm.display().to_string()));
    }
    let opt = launcher_options(game, flavor);
    let ngx_key = format!("{prefix}_NGX_SDK");
    if opt.dlss
        && !env.iter().any(|(key, _)| *key == ngx_key)
        && let Some(ngx) = &opt.ngx
    {
        env.push((ngx_key, ngx.display().to_string()));
    }
    for (key, value) in [
        ("PYTHONUNBUFFERED", "1"),
        ("PYTHONIOENCODING", "utf-8"),
        ("PYTHONUTF8", "1"),
    ] {
        env.push((key.to_string(), value.to_string()));
    }
    let _ = plan;
    env
}

/// Hand the builder a copy of the game's working renderer when it cannot compile one (no Vulkan/VMA headers), and
/// say so. Returns the note to show.
fn reuse_renderer_note(
    game: &Path,
    builder: &Path,
    work: Option<&Path>,
    _notes: &mut Vec<BuildEvent>,
) -> Option<String> {
    let flavor = game::flavor_of(game).unwrap_or(Flavor::Longshot);
    let prefix = env_prefix(flavor);
    if std::env::var(format!("{prefix}_VK_PREBUILT")).is_ok() {
        return None;
    }
    if vulkan_headers_ok(builder, work) {
        return None;
    }
    let names: &[&str] = match flavor {
        Flavor::ReLink => &["relink_vk.dll", "relink_vk.so", "relink_vk.dylib"],
        _ => &["longshot_vk.dll", "longshot_vk.so", "longshot_vk.dylib"],
    };
    for sub in ["bin", ""] {
        for name in names {
            let src = if sub.is_empty() {
                game.join(name)
            } else {
                game.join(sub).join(name)
            };
            if !src.is_file() {
                continue;
            }
            let out = work
                .map(Path::to_path_buf)
                .unwrap_or_else(|| game.join(".shb-work"));
            if std::fs::create_dir_all(&out).is_err() {
                return None;
            }
            let dst = out.join(format!("shb-{name}"));
            if src != dst && std::fs::copy(&src, &dst).is_ok() {
                // The child reads this, so it has to be set for real.
                // SAFETY: single-threaded setup before the builder starts.
                unsafe { std::env::set_var(format!("{prefix}_VK_PREBUILT"), &dst) };
                let name = dst
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                return Some(format!(
                    "Vulkan headers not found: reusing the installed renderer ({name})"
                ));
            }
            return None;
        }
    }
    None
}

/// Ask the builder itself whether it can find the Vulkan, SPIR-V and VMA headers.
fn vulkan_headers_ok(builder: &Path, work: Option<&Path>) -> bool {
    let tools = builder.join("tools");
    let code = "import sys; sys.path.insert(0, 'nx2'); import build; \
                print(1 if all(build.vulkan_headers(sys.argv[1] if len(sys.argv) > 1 else None)) else 0)";
    let mut cmd = Command::new(builder_python(builder));
    cmd.arg("-I").arg("-c").arg(code);
    if let Some(dir) = work {
        cmd.arg(dir);
    }
    cmd.current_dir(&tools)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(out) = cmd.output() else { return true };
    if !out.status.success() {
        return true;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim() == "1")
        == Some(true)
}

/// What a line of the builder's output means.
#[derive(Debug, PartialEq)]
pub enum Line {
    Stage {
        name: String,
        text: String,
    },
    Progress(f32),
    /// `@@plan`, `@@eta` and the rest of the protocol: they move the bar, nothing more.
    Quiet,
    /// `@@error <what>`: the builder failed with that reason.
    Failure(String),
    /// A line of ordinary output.
    Output(String),
}

/// Read one line of the `@@` protocol. `fraction` is the whole build's, `stage` the stage's name.
pub fn parse_line(line: &str, stage: &mut String, fraction: &mut f32) -> Line {
    if let Some(rest) = line.strip_prefix("@@stage ") {
        let (name, text) = rest.split_once(' ').unwrap_or((rest, rest));
        *stage = name.to_string();
        *fraction = stage_lo(name);
        return Line::Stage {
            name: name.to_string(),
            text: text.to_string(),
        };
    }
    if let Some(rest) = line.strip_prefix("@@progress ") {
        if let Ok(value) = rest.trim().parse::<f32>() {
            let lo = stage_lo(stage);
            let hi = stage_hi(stage);
            *fraction = lo + value.clamp(0.0, 1.0) * (hi - lo);
            return Line::Progress(*fraction);
        }
        return Line::Quiet;
    }
    if let Some(rest) = line.strip_prefix("@@error ") {
        return Line::Failure(rest.to_string());
    }
    if line.starts_with("@@") {
        return Line::Quiet;
    }
    Line::Output(line.to_string())
}

/// Run the builder, reporting as it goes. `stop` lets the window cancel: the child is killed.
pub fn build(
    game: &Path,
    builder: &Path,
    work: Option<&Path>,
    stop: Arc<AtomicBool>,
    mut report: impl FnMut(BuildEvent),
) -> Result<()> {
    let flavor = game::flavor_of(game).unwrap_or(Flavor::Longshot);
    let (plan, notes) =
        plan(game, builder, flavor, work).map_err(|(key, arg)| Error::Build { key, arg })?;
    for note in notes {
        report(note);
    }
    // A game left open would stop the builder at its final swap.
    close_running_game(game, &mut report);
    let env = build_env(game, builder, &plan, flavor);
    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args)
        .current_dir(&plan.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .env_clear();
    for (key, value) in &env {
        cmd.env(key, value);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child: Child = cmd.spawn().map_err(|err| Error::Build {
        key: "recomp_no_builder".to_string(),
        arg: err.to_string(),
    })?;
    report(BuildEvent::Progress {
        fraction: 0.0,
        text: "starting".to_string(),
        stage: "read".to_string(),
    });
    let Some(stdout) = child.stdout.take() else {
        return Err(Error::Build {
            key: "recomp_build_failed".to_string(),
            arg: "no output".to_string(),
        });
    };
    let mut stage = "read".to_string();
    let mut fraction = 0.0_f32;
    let mut last: Vec<String> = Vec::new();
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
        match parse_line(line.trim_end(), &mut stage, &mut fraction) {
            Line::Stage { name, text } => report(BuildEvent::Progress {
                fraction,
                text,
                stage: name,
            }),
            Line::Progress(value) => report(BuildEvent::Progress {
                fraction: value,
                text: String::new(),
                stage: stage.clone(),
            }),
            Line::Quiet => {}
            Line::Failure(what) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Build {
                    key: "recomp_build_failed".to_string(),
                    arg: what,
                });
            }
            Line::Output(text) => {
                if !text.is_empty() {
                    last.push(text.clone());
                    last.truncate(40.min(last.len()));
                    last.drain(..last.len().saturating_sub(40));
                    report(BuildEvent::Line(text));
                }
            }
        }
    }
    let status = child.wait().map_err(|err| Error::Build {
        key: "recomp_build_failed".to_string(),
        arg: err.to_string(),
    })?;
    if stop.load(Ordering::Relaxed) {
        return Err(Error::Build {
            key: "recomp_cancelled".to_string(),
            arg: String::new(),
        });
    }
    if !status.success() {
        let tail = last
            .iter()
            .rev()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        return Err(Error::Build {
            key: "recomp_build_failed".to_string(),
            arg: tail.join("\n"),
        });
    }
    report(BuildEvent::Progress {
        fraction: 1.0,
        text: "done".to_string(),
        stage: "done".to_string(),
    });
    Ok(())
}

/// Close every copy of the game running from this folder, so the builder can replace it (Windows).
fn close_running_game(game: &Path, report: &mut impl FnMut(BuildEvent)) {
    if !cfg!(windows) {
        return;
    }
    let exe_names: Vec<&str> = match game::flavor_of(game).unwrap_or(Flavor::Longshot) {
        Flavor::ReLink => vec!["relink-game.exe"],
        _ => vec!["longshot.exe", "hookshot.exe"],
    };
    for name in exe_names {
        let Ok(out) = Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {name}"), "/FO", "CSV", "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let pids: Vec<&str> = line
                .split("\",\"")
                .map(|cell| cell.trim_matches('"'))
                .collect();
            let Some(pid) = pids.get(1) else { continue };
            if pid.parse::<u32>().is_err() {
                continue;
            }
            if Command::new("taskkill")
                .args(["/PID", pid, "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok()
            {
                report(BuildEvent::Note(format!(
                    "Closed the running game (process {pid}) so the build can finish"
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_moves_the_bar_by_stage() {
        let mut stage = "read".to_string();
        let mut fraction = 0.0;
        assert!(matches!(
            parse_line(
                "@@stage lift translating the game",
                &mut stage,
                &mut fraction
            ),
            Line::Stage { .. }
        ));
        assert_eq!(stage, "lift");
        assert!((fraction - 0.03).abs() < 1e-6);
        match parse_line("@@progress 0.5", &mut stage, &mut fraction) {
            Line::Progress(value) => assert!((value - 0.09).abs() < 1e-6, "{value}"),
            other => panic!("{other:?}"),
        }
        // A value outside 0..1 is clamped, not trusted.
        match parse_line("@@progress 3", &mut stage, &mut fraction) {
            Line::Progress(value) => assert!((value - 0.15).abs() < 1e-6, "{value}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_protocol_lines_and_errors() {
        let mut stage = "compile".to_string();
        let mut fraction = 0.2;
        assert_eq!(
            parse_line("@@plan 12 files", &mut stage, &mut fraction),
            Line::Quiet
        );
        assert_eq!(
            parse_line("@@eta 3m", &mut stage, &mut fraction),
            Line::Quiet
        );
        assert_eq!(
            parse_line("@@error clang: no such file", &mut stage, &mut fraction),
            Line::Failure("clang: no such file".to_string())
        );
        assert_eq!(
            parse_line("cc1plus: warning", &mut stage, &mut fraction),
            Line::Output("cc1plus: warning".to_string())
        );
    }

    #[test]
    fn the_env_prefix_follows_the_flavor() {
        assert_eq!(env_prefix(Flavor::ReLink), "RELINK");
        assert_eq!(env_prefix(Flavor::Longshot), "LONGSHOT");
        assert_eq!(env_prefix(Flavor::Hookshot), "LONGSHOT");
    }

    #[test]
    fn the_plan_calls_the_builders_own_script() {
        let root = std::env::temp_dir().join("shb-plan-test");
        let builder = root.join("builder");
        std::fs::create_dir_all(builder.join("tools").join("nx2")).unwrap();
        std::fs::write(builder.join("tools").join("nx2").join("make_game.py"), "").unwrap();
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        let (plan, _) = plan(&game, &builder, Flavor::Longshot, Some(&root.join("work"))).unwrap();
        assert_eq!(plan.args[0], "-I");
        assert_eq!(plan.args[1], "nx2/make_game.py");
        assert_eq!(plan.args[2], "--update");
        assert_eq!(plan.args[3], game.display().to_string());
        assert_eq!(plan.args[4], "--jobs");
        assert_eq!(plan.args[5], "0");
        assert!(plan.args.contains(&"--work".to_string()));
        assert!(plan.cwd.ends_with("tools"));
        // Without the builder's own python it still names one, so the error is the builder's, not ours.
        assert!(plan.program.to_string_lossy().contains("python"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_builder_is_an_error_with_a_key() {
        let root = std::env::temp_dir().join("shb-plan-missing");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let err = plan(&root, &root.join("nope"), Flavor::Longshot, None).unwrap_err();
        assert_eq!(err.0, "recomp_no_builder");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_launcher_options_read_the_games_own_ini_when_the_launcher_is_missing() {
        let root = std::env::temp_dir().join("shb-options-test");
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("longshot_options.ini"), "dlss=1\nfsr=0\n").unwrap();
        // No LongshotLauncher in this user's data folders: the game's own file decides.
        let opt = launcher_options(&game, Flavor::Longshot);
        if launcher_dirs(Flavor::Longshot).is_empty() {
            assert!(opt.dlss);
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
