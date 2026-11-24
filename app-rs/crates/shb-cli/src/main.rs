//! `shb`: the command line of the companion app.
//!
//! ```text
//! shb profiles                         list the game profiles
//! shb install --emu <folder> [options]  install into the emulator's mod folder
//! shb uninstall --emu <folder>          remove what this app installed
//! shb show [--emu <folder> | --nso <file>]   what the installed module says
//! shb patch --nso <in> --out <out> [options] patch a module image without installing it
//! shb lang --emu <folder> [--set es]    read / set the mod HUD language of an installed module
//! shb game --game <folder> [options]    the same settings for a recompiled game (Longshot/Hookshot/ReLink)
//! shb game-uninstall --game <folder>    put a recompiled game's original files back
//! ```
//!
//! The options that shape the install are the settings of the Python app (`--profile`, `--hud-mode`,
//! `--enemy-hp`, `--manual-shield`...), and `--settings <file>` loads a JSON the window wrote.

use shb_core::config::{self, read_config};
use shb_core::presets::{self, PROFILE_KEYS};
use shb_core::settings::{HudMode, Lang, Settings};
use shb_core::{Error, Result, install, payload_root};
use shb_core::{find, game, rebuild, rekarina};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
Super Hero Boost companion (Rust port)

  shb profiles                               list the game profiles
  shb install --emu <folder> [options]       install the mod into the emulator's mod folder
  shb uninstall --emu <folder>               remove the folders this app installed
  shb show [--emu <folder> | --nso <file>]   print the configuration inside an installed module
  shb patch --nso <file> --out <file> [options]
                                             patch a module image and write it somewhere
  shb lang --emu <folder> [--set <code>]     read or set the mod HUD language of an installed module
  shb game --game <folder> [options]         apply the settings to a recompiled game
  shb game-uninstall --game <folder>         restore a recompiled game's files
  shb update [--download|--apply] [--current <version>]
                                             check GitHub for a newer release (and download / install it)

Install options (they override the profile):
  --profile <key>          game profile (default: custom)
  --settings <file>        JSON settings to start from (the window's file)
  --payload <folder>       payload folder (default: next to the program, else app/payload)
  --name <folder>          mod folder name (default: zz-Super-Hero-Boost)
  --save-settings          remember the settings as the app's own file
  --dry-run                (game) only report what was found, write nothing
  --no-stamina  --no-button-mapping  --no-songbook  --stamina-bar
  --hud-mode <wide|simple|plain>   --hud-collectibles   --rupees  --clock  --clock24
  --quick-boots  --quick-mask  --hide-minimap
  --enemy-hp <pct>  --damage <pct>  --sprint-speed <pct>
  --manual-shield  --target-r3  --sprint-l3  --sprint-l3-toggle  --auto-target  --switch-target
  --preset <0..3>  --combat-only  --off <jump,attack,...>
  --button-style <switch|xbox|playstation>  --bm-layout <nintendo|ab|xbox|gamecube>  --bm-simple
  --game-layout <none|playstation|xbox>   the game's own prompts (downloaded once, ~70 MB)
  --bm-l <assign|selector>  --controls <classic|action_down|shield|souls|triggers>
  --hud-ctrl-scale <pct>  --hud-count-scale <pct>
  --lang <en|es|pt|fr|de|it>  (default: the game's language when it can be read)
";

fn print_config(cfg: shb_core::config::InstalledConfig) -> Result<()> {
    println!("stamina        {}", if cfg.stamina { "on" } else { "off" });
    println!("preset         {}", cfg.preset);
    println!("enemy hp       {}%", cfg.enemy_hp);
    println!("damage taken   {}%", cfg.damage);
    println!("sprint speed   {}%", cfg.sprint_speed);
    let hud_name = ["wide", "simple", "plain"]
        .get(cfg.hud_mode as usize)
        .copied()
        .unwrap_or("?");
    let lang_name = Lang::ALL
        .get(cfg.lang as usize)
        .map(|l| l.as_str())
        .unwrap_or("?");
    println!("hud mode       {hud_name}");
    println!("hud language   {lang_name}");
    println!(
        "button remap   {}",
        if cfg.bm_remap { "on" } else { "classic" }
    );
    for (button, role) in &cfg.roles {
        let name = shb_core::roles::Role::ALL
            .get(*role as usize)
            .map(|r| r.as_str())
            .unwrap_or("?");
        println!("  {button:<3} {name}");
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct Options {
    emu: Option<PathBuf>,
    game: Option<PathBuf>,
    nso: Option<PathBuf>,
    out: Option<PathBuf>,
    settings: Option<PathBuf>,
    payload: Option<PathBuf>,
    profile: Option<String>,
    name: Option<String>,
    lang: Option<Lang>,
    set_lang: Option<Lang>,
    hud_mode: Option<HudMode>,
    off: Option<String>,
    controls: Option<String>,
    button_style: Option<String>,
    bm_layout: Option<String>,
    game_layout: Option<String>,
    bm_l: Option<String>,
    enemy_hp: Option<u16>,
    damage: Option<u16>,
    sprint_speed: Option<u16>,
    preset: Option<u8>,
    hud_ctrl_scale: Option<u8>,
    hud_count_scale: Option<u8>,
    // switches
    no_stamina: bool,
    no_button_mapping: bool,
    no_songbook: bool,
    stamina_bar: bool,
    hud_collectibles: bool,
    rupees: bool,
    clock: bool,
    clock24: bool,
    quick_boots: bool,
    quick_mask: bool,
    hide_minimap: bool,
    manual_shield: bool,
    target_r3: bool,
    sprint_l3: bool,
    sprint_l3_toggle: bool,
    auto_target: bool,
    switch_target: bool,
    combat_only: bool,
    bm_simple: bool,
    no_heart_drop: bool,
    no_magic_drop: bool,
    enemy_double: bool,
    great_fairy: bool,
    farore_outdoor: bool,
    save_settings: bool,
    dry_run: bool,
    app: Option<PathBuf>,
    builder: Option<PathBuf>,
    mode: Option<String>,
    no_rebuild: bool,
    current: Option<String>,
    download: bool,
    apply: bool,
}

/// The value that follows a flag.
fn value_of(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    it.next()
        .ok_or_else(|| Error::io(PathBuf::from(flag), std::io::Error::other("needs a value")))
}

fn parse_args(args: &[String]) -> Result<(String, Options)> {
    let mut it = args.iter().cloned().peekable();
    let command = it.next().unwrap_or_default();
    let mut o = Options::default();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--emu" => o.emu = Some(PathBuf::from(value_of(&mut it, "--emu")?)),
            "--game" => o.game = Some(PathBuf::from(value_of(&mut it, "--game")?)),
            "--app" => o.app = Some(PathBuf::from(value_of(&mut it, "--app")?)),
            "--no-rebuild" => {
                o.no_rebuild = true;
                o.mode = Some("settings".to_string());
            }
            "--mode" => o.mode = Some(value_of(&mut it, "--mode")?),
            "--builder" => o.builder = Some(PathBuf::from(value_of(&mut it, "--builder")?)),
            "--nso" => o.nso = Some(PathBuf::from(value_of(&mut it, "--nso")?)),
            "--out" => o.out = Some(PathBuf::from(value_of(&mut it, "--out")?)),
            "--settings" => o.settings = Some(PathBuf::from(value_of(&mut it, "--settings")?)),
            "--payload" => o.payload = Some(PathBuf::from(value_of(&mut it, "--payload")?)),
            "--profile" => o.profile = Some(value_of(&mut it, "--profile")?),
            "--name" => o.name = Some(value_of(&mut it, "--name")?),
            "--lang" => {
                let v = value_of(&mut it, "--lang")?;
                o.lang = Some(Lang::parse(&v).ok_or_else(|| unknown("language", &v))?);
            }
            "--set" => {
                let v = value_of(&mut it, "--set")?;
                o.set_lang = Some(Lang::parse(&v).ok_or_else(|| unknown("language", &v))?);
            }
            "--hud-mode" => {
                let v = value_of(&mut it, "--hud-mode")?;
                o.hud_mode = Some(HudMode::parse(&v).ok_or_else(|| unknown("HUD mode", &v))?);
            }
            "--controls" => o.controls = Some(value_of(&mut it, "--controls")?),
            "--button-style" => o.button_style = Some(value_of(&mut it, "--button-style")?),
            "--bm-layout" => o.bm_layout = Some(value_of(&mut it, "--bm-layout")?),
            "--game-layout" => o.game_layout = Some(value_of(&mut it, "--game-layout")?),
            "--bm-l" => o.bm_l = Some(value_of(&mut it, "--bm-l")?),
            "--off" => o.off = Some(value_of(&mut it, "--off")?),
            "--enemy-hp" => o.enemy_hp = Some(number(&value_of(&mut it, "--enemy-hp")?)?),
            "--damage" => o.damage = Some(number(&value_of(&mut it, "--damage")?)?),
            "--sprint-speed" => {
                o.sprint_speed = Some(number(&value_of(&mut it, "--sprint-speed")?)?)
            }
            "--preset" => o.preset = Some(number(&value_of(&mut it, "--preset")?)?),
            "--hud-ctrl-scale" => {
                o.hud_ctrl_scale = Some(number(&value_of(&mut it, "--hud-ctrl-scale")?)?)
            }
            "--hud-count-scale" => {
                o.hud_count_scale = Some(number(&value_of(&mut it, "--hud-count-scale")?)?)
            }
            "--no-stamina" => o.no_stamina = true,
            "--no-button-mapping" => o.no_button_mapping = true,
            "--no-songbook" => o.no_songbook = true,
            "--stamina-bar" => o.stamina_bar = true,
            "--hud-collectibles" => o.hud_collectibles = true,
            "--rupees" => o.rupees = true,
            "--clock" => o.clock = true,
            "--clock24" => o.clock24 = true,
            "--quick-boots" => o.quick_boots = true,
            "--quick-mask" => o.quick_mask = true,
            "--hide-minimap" => o.hide_minimap = true,
            "--manual-shield" => o.manual_shield = true,
            "--target-r3" => o.target_r3 = true,
            "--sprint-l3" => o.sprint_l3 = true,
            "--sprint-l3-toggle" => o.sprint_l3_toggle = true,
            "--auto-target" => o.auto_target = true,
            "--switch-target" => o.switch_target = true,
            "--combat-only" => o.combat_only = true,
            "--bm-simple" => o.bm_simple = true,
            "--no-heart-drop" => o.no_heart_drop = true,
            "--no-magic-drop" => o.no_magic_drop = true,
            "--enemy-double" => o.enemy_double = true,
            "--great-fairy" => o.great_fairy = true,
            "--farore-outdoor" => o.farore_outdoor = true,
            "--save-settings" => o.save_settings = true,
            "--dry-run" => o.dry_run = true,
            "--current" => o.current = Some(value_of(&mut it, "--current")?),
            "--download" => o.download = true,
            "--apply" => o.apply = true,
            "--help" | "-h" => return Ok(("help".to_string(), o)),
            other => {
                return Err(Error::io(
                    PathBuf::from(other),
                    std::io::Error::other("unknown argument (see --help)"),
                ));
            }
        }
    }
    Ok((command, o))
}

fn unknown(kind: &str, value: &str) -> Error {
    Error::io(
        PathBuf::from(value),
        std::io::Error::other(format!("unknown {kind}")),
    )
}

fn number<T: std::str::FromStr>(value: &str) -> Result<T> {
    value
        .parse()
        .map_err(|_| Error::io(PathBuf::from(value), std::io::Error::other("not a number")))
}

/// The settings an invocation means: the profile (or the JSON), then the command line's own options.
fn settings_from(o: &Options) -> Result<Settings> {
    let mut s = if let Some(path) = &o.settings {
        Settings::load(path)?
    } else if let Some(profile) = &o.profile {
        if !PROFILE_KEYS.contains(&profile.as_str()) {
            return Err(unknown("profile", profile));
        }
        presets::settings_for(profile)
    } else {
        presets::settings_for("custom")
    };
    if let Some(profile) = &o.profile {
        if !PROFILE_KEYS.contains(&profile.as_str()) {
            return Err(unknown("profile", profile));
        }
        if o.settings.is_some() {
            // A profile on top of a file: take the profile's values, keep the user's own style/layout choices.
            let from_profile = presets::settings_for(profile);
            let keep = (
                s.button_style,
                s.bm_layout,
                s.game_layout,
                s.bm_l_mode,
                s.lang,
            );
            s = from_profile;
            (
                s.button_style,
                s.bm_layout,
                s.game_layout,
                s.bm_l_mode,
                s.lang,
            ) = keep;
        }
    }

    if let Some(controls) = &o.controls {
        s.bm_roles = shb_core::roles::remap_preset(controls)
            .ok_or_else(|| unknown("button table", controls))?;
    }
    if let Some(off) = &o.off {
        for key in off.split(',').map(str::trim) {
            match key {
                "jump" => s.jump = false,
                "attack" => s.attack = false,
                "sprint" => s.sprint = false,
                "block" => s.block = false,
                "shoot" => s.shoot = false,
                "roll" => s.roll = false,
                "" => {}
                other => return Err(unknown("stamina action", other)),
            }
        }
    }
    if let Some(style) = &o.button_style {
        s.button_style = match style.as_str() {
            "switch" => shb_core::settings::ButtonStyle::Switch,
            "xbox" => shb_core::settings::ButtonStyle::Xbox,
            "playstation" => shb_core::settings::ButtonStyle::PlayStation,
            other => return Err(unknown("button style", other)),
        };
    }
    if let Some(layout) = &o.bm_layout {
        s.bm_layout = match layout.as_str() {
            "nintendo" => shb_core::settings::BmLayout::Nintendo,
            "ab" => shb_core::settings::BmLayout::Ab,
            "xbox" => shb_core::settings::BmLayout::Xbox,
            "gamecube" => shb_core::settings::BmLayout::Gamecube,
            other => return Err(unknown("button layout", other)),
        };
    }
    if let Some(layout) = &o.game_layout {
        s.game_layout = match layout.as_str() {
            "none" => shb_core::settings::GameLayout::None,
            "playstation" => shb_core::settings::GameLayout::PlayStation,
            "xbox" => shb_core::settings::GameLayout::Xbox,
            other => return Err(unknown("game layout", other)),
        };
    }
    if let Some(l) = &o.bm_l {
        s.bm_l_mode = match l.as_str() {
            "assign" => shb_core::settings::LMode::Assign,
            "selector" => shb_core::settings::LMode::Selector,
            other => return Err(unknown("L mode", other)),
        };
    }
    if let Some(mode) = o.hud_mode {
        s.hud_mode = mode;
    }
    if let Some(preset) = o.preset {
        if preset > 3 {
            return Err(unknown("stamina preset", &preset.to_string()));
        }
        s.preset = preset;
    }
    if let Some(v) = o.enemy_hp {
        s.enemy_hp = v;
    }
    if let Some(v) = o.damage {
        s.damage = v;
    }
    if let Some(v) = o.sprint_speed {
        s.sprint_speed = v;
    }
    if let Some(v) = o.hud_ctrl_scale {
        s.hud_ctrl_scale = v;
    }
    if let Some(v) = o.hud_count_scale {
        s.hud_count_scale = v;
    }
    if let Some(lang) = o.lang {
        s.lang = lang;
    }
    if o.no_stamina {
        s.stamina = false;
    }
    if o.no_button_mapping {
        s.button_mapping = false;
    }
    if o.no_songbook {
        s.songbook = false;
    }
    if o.stamina_bar {
        s.stamina_bar = true;
    }
    if o.hud_collectibles {
        s.hud_collectibles = true;
    }
    if o.rupees {
        s.hud_rupees = true;
    }
    if o.clock {
        s.hud_clock = true;
    }
    if o.clock24 {
        s.clock_24h = true;
    }
    if o.quick_boots {
        s.quick_boots = true;
    }
    if o.quick_mask {
        s.quick_mask = true;
    }
    if o.hide_minimap {
        s.hide_minimap = true;
    }
    if o.manual_shield {
        s.manual_shield = true;
    }
    if o.target_r3 {
        s.target_r3 = true;
    }
    if o.sprint_l3 {
        s.sprint_l3 = true;
    }
    if o.sprint_l3_toggle {
        s.sprint_l3_toggle = true;
    }
    if o.auto_target {
        s.auto_target = true;
    }
    if o.switch_target {
        s.switch_target = true;
    }
    if o.combat_only {
        s.combat_only = true;
    }
    if o.bm_simple {
        s.bm_simple = true;
    }
    if o.no_heart_drop {
        s.no_heart_drop = true;
    }
    if o.no_magic_drop {
        s.no_magic_drop = true;
    }
    if o.enemy_double {
        s.enemy_double = true;
    }
    if o.great_fairy {
        s.great_fairy = true;
    }
    if o.farore_outdoor {
        s.farore_outdoor = true;
    }
    Ok(s)
}

fn run(args: &[String]) -> Result<()> {
    let (command, o) = parse_args(args)?;
    match command.as_str() {
        "help" | "" => {
            println!("{USAGE}");
            Ok(())
        }
        "profiles" => {
            for key in PROFILE_KEYS {
                let p = presets::profile(key);
                println!(
                    "{key:<17} stamina {:>9}  enemy hp {:>3}%  damage {:>3}%  hud {}  enemies {}",
                    if p.stamina { "on" } else { "off" },
                    p.enemy_hp,
                    p.damage,
                    p.hud_mode,
                    if p.enemy_double {
                        "x2 duplicated"
                    } else {
                        "vanilla"
                    },
                );
            }
            Ok(())
        }
        "install" => {
            let emu = o.emu.clone().ok_or_else(|| {
                Error::io(
                    PathBuf::from("--emu"),
                    std::io::Error::other("the emulator folder is required"),
                )
            })?;
            let mut settings = settings_from(&o)?;
            if o.lang.is_none() {
                // The mod's own HUD labels follow the language the game is played in.
                if let Some(lang) = find::detect_lang(Some(&emu), None) {
                    settings.lang = lang;
                }
            }
            let payload = payload_root(o.payload.as_deref()).ok_or_else(|| {
                shb_core::Error::PayloadMissing(PathBuf::from(
                    "payload (set --payload or SHB_PAYLOAD)",
                ))
            })?;
            let mut req = install::InstallRequest::new(&emu, settings.clone(), &payload);
            if let Some(name) = &o.name {
                req.name = name.clone();
            }
            let res = install::install(&req)?;
            println!("installed {} ({})", res.target.display(), res.variant);
            for note in &res.notes {
                println!("  note: {note}");
            }
            if o.save_settings {
                let path = shb_core::paths::save_settings(&settings)?;
                println!("settings saved to {}", path.display());
            }
            Ok(())
        }
        "uninstall" => {
            let emu = o.emu.clone().ok_or_else(|| {
                Error::io(
                    PathBuf::from("--emu"),
                    std::io::Error::other("the emulator folder is required"),
                )
            })?;
            if install::uninstall(&emu)? {
                println!("removed the mod folders under {}", emu.display());
            } else {
                println!("nothing to remove under {}", emu.display());
            }
            Ok(())
        }
        "show" => {
            let nso = match (&o.nso, &o.emu) {
                (Some(path), _) => shb_core::error::read(path)?,
                (None, Some(emu)) => {
                    // A Rekarina install keeps the module in the game's ExeFS, not in a mod folder.
                    if let Some(rk) = rekarina::find(emu) {
                        return print_config(rekarina::installed_config(&rk)?);
                    }
                    let info = install::inspect(emu);
                    let module = info
                        .installed
                        .map(|p| p.join("exefs/subsdk9"))
                        .ok_or_else(|| Error::NoModsDir(emu.clone()))?;
                    shb_core::error::read(&module)?
                }
                (None, None) => {
                    return Err(Error::io(
                        PathBuf::from("--nso"),
                        std::io::Error::other("needs --nso <file> or --emu <folder>"),
                    ));
                }
            };
            print_config(read_config(&nso)?)
        }
        "patch" => {
            let nso = o.nso.clone().ok_or_else(|| {
                Error::io(
                    PathBuf::from("--nso"),
                    std::io::Error::other("the module to patch is required"),
                )
            })?;
            let out = o.out.clone().ok_or_else(|| {
                Error::io(
                    PathBuf::from("--out"),
                    std::io::Error::other("where to write is required"),
                )
            })?;
            let settings = settings_from(&o)?;
            let image = shb_core::error::read(&nso)?;
            shb_core::error::write(&out, &config::patch_config(&image, &settings)?)?;
            println!("wrote {}", out.display());
            Ok(())
        }
        "lang" => {
            let emu = o.emu.clone().ok_or_else(|| {
                Error::io(
                    PathBuf::from("--emu"),
                    std::io::Error::other("the emulator folder is required"),
                )
            })?;
            let info = install::inspect(&emu);
            let module = info
                .installed
                .map(|p| p.join("exefs/subsdk9"))
                .ok_or_else(|| Error::NoModsDir(emu.clone()))?;
            let image = shb_core::error::read(&module)?;
            if let Some(lang) = o.set_lang {
                if config::set_installed_lang(&module, lang)? {
                    println!("language set to {lang} (the game picks it up on the next start)");
                } else {
                    println!("the installed module has no language byte: reinstall");
                }
            } else {
                let cfg = read_config(&image)?;
                let name = Lang::ALL
                    .get(cfg.lang as usize)
                    .map(|l| l.as_str())
                    .unwrap_or("?");
                println!("{name}");
            }
            Ok(())
        }
        "play" => {
            let emu = o.emu.clone().or_else(|| o.game.clone()).ok_or_else(|| {
                Error::io(
                    PathBuf::from("--emu"),
                    std::io::Error::other("the emulator or game folder is required"),
                )
            })?;
            let exe = shb_core::find::emulator_exe(&emu)
                .or_else(|| {
                    shb_core::rekarina::find(&emu)
                        .and_then(|rk| shb_core::rekarina::launcher_of(&rk))
                })
                .or_else(|| {
                    shb_core::game::find(&emu)
                        .map(|game| game.root)
                        .and_then(|root| shb_core::find::launcher_of(&root))
                })
                .ok_or_else(|| Error::NotAGame(emu.clone()))?;
            shb_core::find::launch(&exe)?;
            println!("opened {}", exe.display());
            Ok(())
        }
        "rebuild" => {
            let path = o.game.clone().ok_or_else(|| {
                Error::io(
                    PathBuf::from("--game"),
                    std::io::Error::other("the game folder is required"),
                )
            })?;
            let payload = payload_root(o.payload.as_deref()).ok_or_else(|| {
                Error::PayloadMissing(PathBuf::from("payload (set --payload or SHB_PAYLOAD)"))
            })?;
            // The app folder is only where a `payload/` may sit: the executable carries one when it is not there.
            let app = match &o.app {
                Some(dir) => dir.clone(),
                None => payload
                    .parent()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(".")),
            };
            let profile = o
                .profile
                .clone()
                .unwrap_or_else(|| "recommended".to_string());
            let mode = match &o.mode {
                None => rebuild::Mode::Install,
                Some(name) => rebuild::Mode::parse(name).ok_or_else(|| unknown("mode", name))?,
            };
            let request = rebuild::RebuildRequest {
                game: path.clone(),
                profile: profile.clone(),
                mode,
                rebuild: !o.no_rebuild,
                app: app.clone(),
                builder: o.builder.clone(),
            };
            println!(
                "{} {} with profile {profile} ({})",
                mode.name(),
                path.display(),
                if mode.compiles() {
                    "the builder runs"
                } else {
                    "no compiler"
                }
            );
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            rebuild::run(&request, stop, |event| match event {
                shb_core::recomp_build::BuildEvent::Progress {
                    fraction,
                    text,
                    stage,
                } => {
                    if !text.is_empty() {
                        println!("[{stage}] {:3.0}% {text}", fraction * 100.0);
                    }
                }
                shb_core::recomp_build::BuildEvent::Line(text) => println!("{text}"),
                shb_core::recomp_build::BuildEvent::Note(text) => println!("· {text}"),
            })?;
            println!("done");
            Ok(())
        }
        "game" => {
            let path = o.game.clone().ok_or_else(|| {
                Error::io(
                    PathBuf::from("--game"),
                    std::io::Error::other("the game folder is required"),
                )
            })?;
            let settings = settings_from(&o)?;
            let payload = payload_root(o.payload.as_deref()).ok_or_else(|| {
                Error::PayloadMissing(PathBuf::from("payload (set --payload or SHB_PAYLOAD)"))
            })?;
            if let Some(rekarina) = rekarina::find(&path) {
                println!("game: {} (rekarina)", rekarina.root.display());
                println!(
                    "  built {}",
                    if rekarina::is_built(&rekarina) {
                        "yes"
                    } else {
                        "no (needs Rekarina's build)"
                    }
                );
                if o.dry_run {
                    return Ok(());
                }
                rekarina::apply_settings(&rekarina, &settings, &payload)?;
                println!(
                    "settings written to the module, its hash in the executable, the romfs and the mod order"
                );
                return Ok(());
            }
            let found = game::find(&path).ok_or(Error::NotAGame(path.clone()))?;
            println!(
                "game: {} ({}, {})",
                found.root.display(),
                found.flavor.name(),
                found.exefs.display()
            );
            println!(
                "  module {}",
                if game::is_installed(&found) {
                    "present"
                } else {
                    "missing (needs a build)"
                }
            );
            if o.dry_run {
                return Ok(());
            }
            let image = game::apply_settings(&found, &settings, &payload)?;
            println!(
                "settings written to the module{}",
                if image { " and image.bin" } else { "" }
            );
            println!(
                "the code has to be compiled into the game: that rebuild is not ported to the Rust app yet"
            );
            Ok(())
        }
        "game-uninstall" => {
            let path = o.game.clone().ok_or_else(|| {
                Error::io(
                    PathBuf::from("--game"),
                    std::io::Error::other("the game folder is required"),
                )
            })?;
            if let Some(rekarina) = rekarina::find(&path) {
                return match rekarina::uninstall(&rekarina)? {
                    true => {
                        println!(
                            "restored the original files under {}",
                            rekarina.root.display()
                        );
                        Ok(())
                    }
                    false => {
                        println!("nothing to restore under {}", rekarina.root.display());
                        Ok(())
                    }
                };
            }
            let found = game::find(&path).ok_or(Error::NeedsBuild(path.clone()))?;
            if game::uninstall(&found)? {
                println!("restored the original files under {}", found.root.display());
            } else {
                println!("nothing to restore under {}", found.root.display());
            }
            Ok(())
        }
        "update" => {
            let current = o
                .current
                .clone()
                .unwrap_or_else(|| shb_core::VERSION.to_string());
            println!(
                "this build {current}, checking {}",
                shb_core::update::API_LATEST
            );
            match shb_core::update::check(&current)? {
                None => {
                    println!("up to date");
                    Ok(())
                }
                Some(update) => {
                    println!("new release {} ({})", update.tag, update.page_url);
                    match &update.asset_name {
                        Some(name) => println!("asset {name} ({} bytes)", update.asset_size),
                        None => println!("no asset for this platform"),
                    }
                    if o.download || o.apply {
                        let (dest, _) = shb_core::update::update_path(&update.version);
                        println!("downloading to {}", dest.display());
                        let mut last = -1i64;
                        let saved = shb_core::update::download(&update, &dest, |done, total| {
                            let percent = if total > 0 {
                                (done * 100).checked_div(total).unwrap_or(0) as i64
                            } else {
                                -1
                            };
                            if percent != last {
                                last = percent;
                                print!("\r  {percent}%");
                                use std::io::Write;
                                let _ = std::io::stdout().flush();
                            }
                        })?;
                        println!("\nsaved {}", saved.display());
                        if o.apply {
                            if shb_core::update::apply_and_restart(&saved) {
                                println!(
                                    "the app will restart with the new version once this process exits"
                                );
                            } else {
                                println!(
                                    "cannot replace the running program here; the file is at the path above"
                                );
                            }
                        }
                    }
                    Ok(())
                }
            }
        }
        other => Err(unknown("command", other)),
    }
}
