//! The enemy mods: the speed and aggression packs.
//!
//! The game keeps its enemy numbers in compressed `.pack.zs` files. The payload carries a decompressed copy of
//! each one that has a knob in this mod (`enemy_speed_base/`) plus a manifest of the offsets and their values, and
//! a shared zstd dictionary (`enemy_speed_pack.zsdic`); the installer scales the values and writes the pack back
//! compressed with the same dictionary, so the game reads it exactly as it reads its own.
//!
//! The manifest's `k` is the component:
//! - `m` movement speed, `a` attack cadence (divided: a smaller interval is faster), `p` projectile speed
//! - `d` the radius at which a spotting enemy alerts the others
//! - `g` the shared pause during which only one enemy attacks, `k` how far an enemy chases before giving up,
//!   `i` how long a melee enemy waits on screen before fighting, `o` "wait until on screen" (a flag, stored as a
//!   u32 and set to 0 only by the maximum aggression level)

use crate::error::{Error, Result, read, write};
use crate::install::copy_tree;
use crate::settings::Settings;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

/// A level of the enemy options and the factor it means (`ENEMY_FACTORS` of the Python app).
pub const LEVELS: [(u8, f64); 11] = [
    (1, 1.5),
    (2, 2.0),
    (3, 3.0),
    (4, 1.25),
    (5, 1.75),
    (6, 2.5),
    (7, 4.0),
    (8, 5.0),
    (9, 6.0),
    (10, 0.5),
    (11, 0.75),
];

/// The animation folders of the payload, one per level.
pub const ANIM_DIRS: [(u8, &str); 11] = [
    (1, "enemy_anim_x15"),
    (2, "enemy_anim_x2"),
    (3, "enemy_anim_x3"),
    (4, "enemy_anim_x125"),
    (5, "enemy_anim_x175"),
    (6, "enemy_anim_x25"),
    (7, "enemy_anim_x4"),
    (8, "enemy_anim_x5"),
    (9, "enemy_anim_x6"),
    (10, "enemy_anim_x05"),
    (11, "enemy_anim_x075"),
];

/// Bosses and minibosses have their own folders: they are left out of the enemy options.
pub const BOSS_ANIM_DIRS: [(u8, &str); 11] = [
    (1, "boss_anim_x15"),
    (2, "boss_anim_x2"),
    (3, "boss_anim_x3"),
    (4, "boss_anim_x125"),
    (5, "boss_anim_x175"),
    (6, "boss_anim_x25"),
    (7, "boss_anim_x4"),
    (8, "boss_anim_x5"),
    (9, "boss_anim_x6"),
    (10, "boss_anim_x05"),
    (11, "boss_anim_x075"),
];

/// The aggression levels (0 off, 1 aggressive, 2 maximum) and what each does to the manifest's fields.
#[derive(Debug, Clone, Copy)]
struct Aggro {
    /// shared pause during which only one enemy attacks (`g`)
    g: f64,
    /// how far an enemy chases before giving up (`k`)
    k: f64,
    /// how long a melee enemy waits on screen before fighting (`i`)
    i: f64,
    /// "wait until on screen before fighting": the only level that clears the flag (`o`)
    off_screen: bool,
}

const AGGRESSION: [(u8, Aggro); 2] = [
    (
        1,
        Aggro {
            g: 0.5,
            k: 1.5,
            i: 1.0,
            off_screen: false,
        },
    ),
    (
        2,
        Aggro {
            g: 0.0,
            k: 2.0,
            i: 0.0,
            off_screen: true,
        },
    ),
];

pub const AGGRO_LEVELS: [u8; 3] = [0, 1, 2];

/// The factor of a level, when it is one of ours.
pub fn factor(level: u8) -> Option<f64> {
    LEVELS.iter().find(|(l, _)| *l == level).map(|(_, f)| *f)
}

fn level_or_off(level: u8) -> Option<u8> {
    factor(level).map(|_| level)
}

/// The four enemy components: movement, animation, attack cadence, projectile speed.
///
/// The deprecated `enemy_speed` boolean meant the original three at x2 (projectiles stayed off).
pub fn enemy_levels(s: &Settings) -> (u8, u8, u8, u8) {
    let levels = (
        level_or_off(s.enemy_move).unwrap_or(0),
        level_or_off(s.enemy_anim).unwrap_or(0),
        level_or_off(s.enemy_attack).unwrap_or(0),
        level_or_off(s.enemy_proj).unwrap_or(0),
    );
    if s.enemy_speed && levels == (0, 0, 0, 0) {
        return (2, 2, 2, 0);
    }
    levels
}

/// The bosses' own two components.
pub fn boss_levels(s: &Settings) -> (u8, u8) {
    (
        level_or_off(s.boss_move).unwrap_or(0),
        level_or_off(s.boss_anim).unwrap_or(0),
    )
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    packs: Vec<Pack>,
}

#[derive(Debug, Deserialize)]
struct Pack {
    path: String,
    #[serde(default)]
    fields: Vec<Field>,
}

#[derive(Debug, Deserialize)]
struct Field {
    /// byte offset of the f32 in the decompressed pack
    o: usize,
    /// the component (`m`, `a`, `p`, `d`, `g`, `k`, `i`, `o`)
    k: String,
    #[serde(default)]
    v: f64,
}

/// The scales of one install, by component.
struct Scales {
    by_kind: std::collections::BTreeMap<&'static str, f64>,
    off_screen: bool,
}

impl Scales {
    fn new(move_: u8, attack: u8, proj: u8, alert: u8, aggro: u8) -> Self {
        let ag = AGGRESSION
            .iter()
            .find(|(l, _)| *l == aggro)
            .map(|(_, a)| *a);
        let mut by_kind = std::collections::BTreeMap::new();
        by_kind.insert("m", factor(move_).unwrap_or(1.0));
        by_kind.insert(
            "a",
            match factor(attack) {
                // The attack cadence is an interval: dividing it makes the enemy attack faster.
                Some(f) => 1.0 / f,
                None => 1.0,
            },
        );
        by_kind.insert("p", factor(proj).unwrap_or(1.0));
        by_kind.insert("d", factor(alert).unwrap_or(1.0));
        by_kind.insert("g", ag.map(|a| a.g).unwrap_or(1.0));
        by_kind.insert("k", ag.map(|a| a.k).unwrap_or(1.0));
        by_kind.insert("i", ag.map(|a| a.i).unwrap_or(1.0));
        Scales {
            by_kind,
            off_screen: ag.map(|a| a.off_screen).unwrap_or(false),
        }
    }

    fn scale(&self, kind: &str) -> f64 {
        self.by_kind.get(kind).copied().unwrap_or(1.0)
    }

    /// The components this install touches (the packs that carry none of them are left alone).
    fn active(&self) -> BTreeSet<&'static str> {
        let mut out: BTreeSet<&'static str> = self
            .by_kind
            .iter()
            .filter(|(_, v)| **v != 1.0)
            .map(|(k, _)| *k)
            .collect();
        if self.off_screen {
            out.insert("o");
        }
        out
    }
}

/// The components one manifest is scaled with (the boss manifest uses movement only).
#[derive(Debug, Clone, Copy, Default)]
struct Components {
    move_: u8,
    attack: u8,
    proj: u8,
    alert: u8,
    aggro: u8,
}

/// Scale the packs of a manifest into `romfs`. `false` when the payload part is missing or unreadable.
fn patch_packs(
    opts: &Path,
    romfs: &Path,
    base_name: &str,
    manifest_name: &str,
    components: Components,
) -> Result<bool> {
    let base = opts.join(base_name);
    let manifest_path = opts.join(manifest_name);
    let dictionary_path = opts.join("enemy_speed_pack.zsdic");
    if !(base.is_dir() && manifest_path.is_file() && dictionary_path.is_file()) {
        return Ok(false);
    }
    let manifest: Manifest = serde_json::from_slice(&read(&manifest_path)?)
        .map_err(|e| Error::io(&manifest_path, std::io::Error::other(e)))?;
    let dictionary = read(&dictionary_path)?;
    let scales = Scales::new(
        components.move_,
        components.attack,
        components.proj,
        components.alert,
        components.aggro,
    );
    let active = scales.active();

    for pack in &manifest.packs {
        if !pack.fields.iter().any(|f| active.contains(f.k.as_str())) {
            continue;
        }
        let source = base.join(&pack.path);
        if !source.is_file() {
            return Ok(false);
        }
        let compressed = read(&source)?;
        let mut data = match decompress(&compressed, &dictionary) {
            Ok(d) => d,
            Err(_) => return Ok(false),
        };
        for field in &pack.fields {
            if field.k == "o" {
                // A flag, stored inline as a u32: only the maximum aggression clears it.
                if scales.off_screen {
                    put_f32(&mut data, field.o, 0.0)?;
                }
                continue;
            }
            let scale = scales.scale(&field.k);
            if scale != 1.0 {
                // The product is taken in double precision and narrowed once, which is what the Python app's
                // `struct.pack_into("<f", ..., value * scale)` does.
                put_f32(&mut data, field.o, (field.v * scale) as f32)?;
            }
        }
        let out = compress(&data, &dictionary)?;
        let dst = romfs.join(&pack.path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        write(&dst, &out)?;
    }
    Ok(true)
}

fn put_f32(data: &mut [u8], offset: usize, value: f32) -> Result<()> {
    let end = offset + 4;
    if end > data.len() {
        return Err(Error::PayloadMissing(
            Path::new("enemy pack offset out of range").to_path_buf(),
        ));
    }
    data[offset..end].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn decompress(compressed: &[u8], dictionary: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut source = compressed;
    let mut decoder = structured_zstd::decoding::StreamingDecoder::new_with_dictionary_bytes(
        &mut source,
        dictionary,
    )
    .map_err(|e| Error::codec(format!("zstd: {e}")))?;
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| Error::codec(format!("zstd: {e}")))?;
    Ok(out)
}

fn compress(data: &[u8], dictionary: &[u8]) -> Result<Vec<u8>> {
    use structured_zstd::encoding::{CompressionLevel, FrameCompressor};
    // Level 3 is what the Python app's `ZstdCompressor(dict_data=...)` uses.
    let mut compressor: FrameCompressor = FrameCompressor::new(CompressionLevel::from_level(3));
    compressor
        .set_dictionary_from_bytes(dictionary)
        .map_err(|e| Error::codec(format!("zstd dictionary: {e}")))?;
    Ok(compressor.compress_independent_frame(data))
}

/// Install the selected enemy components: patched packs, the animation folder of the chosen level, and the bosses'
/// own movement / animation. Returns the notes to show (`enemy speed` when a payload part was not built).
pub fn install(romfs: &Path, s: &Settings, payload: &Path) -> Result<Vec<String>> {
    let (move_, anim, attack, proj) = enemy_levels(s);
    let (boss_move, boss_anim) = boss_levels(s);
    let alert = level_or_off(s.enemy_alert).unwrap_or(0);
    let aggro = if AGGRO_LEVELS.contains(&s.enemy_aggro) {
        s.enemy_aggro
    } else {
        0
    };
    if (
        move_, anim, attack, proj, boss_move, boss_anim, alert, aggro,
    ) == (0, 0, 0, 0, 0, 0, 0, 0)
    {
        return Ok(Vec::new());
    }
    let opts = payload.join("romfs_opts");
    let mut missing = false;

    if (move_ != 0 || attack != 0 || proj != 0 || alert != 0 || aggro != 0)
        && !patch_packs(
            &opts,
            romfs,
            "enemy_speed_base",
            "enemy_speed_manifest.json",
            Components {
                move_,
                attack,
                proj,
                alert,
                aggro,
            },
        )?
    {
        missing = true;
    }
    if anim != 0 {
        let dir = ANIM_DIRS
            .iter()
            .find(|(l, _)| *l == anim)
            .map(|(_, d)| *d)
            .unwrap_or_default();
        if !opts.join(dir).is_dir() {
            missing = true;
        } else {
            copy_tree(&opts.join(dir), romfs)?;
        }
    }
    if boss_move != 0
        && !patch_packs(
            &opts,
            romfs,
            "boss_speed_base",
            "boss_speed_manifest.json",
            Components {
                move_: boss_move,
                ..Components::default()
            },
        )?
    {
        missing = true;
    }
    if boss_anim != 0 {
        let dir = BOSS_ANIM_DIRS
            .iter()
            .find(|(l, _)| *l == boss_anim)
            .map(|(_, d)| *d)
            .unwrap_or_default();
        if !opts.join(dir).is_dir() {
            missing = true;
        } else {
            copy_tree(&opts.join(dir), romfs)?;
        }
    }
    Ok(if missing {
        vec!["enemy speed: the payload part was not built, skipped".to_string()]
    } else {
        Vec::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use structured_zstd::decoding::StreamingDecoder;
    use structured_zstd::encoding::{CompressionLevel, FrameCompressor};

    /// A dictionary, a manifest and one pack with two knobs and a flag, written into a temp payload.
    fn fixture(root: &Path) -> Vec<u8> {
        let opts = root.join("romfs_opts/enemy_speed_base");
        std::fs::create_dir_all(opts.join("Pack/Actor")).unwrap();
        std::fs::create_dir_all(root.join("romfs_opts")).unwrap();
        // A real (serialized) dictionary: the payload ships one trained with `zstd --train`, and the decoder
        // refuses anything that is not in that format.
        let dictionary = trained_dictionary();
        std::fs::write(root.join("romfs_opts/enemy_speed_pack.zsdic"), &dictionary).unwrap();

        // The pack: 64 bytes, a movement knob at 0, an attack interval at 8 and the flag at 16.
        let mut data = vec![0u8; 64];
        data[0..4].copy_from_slice(&2.0f32.to_le_bytes());
        data[8..12].copy_from_slice(&100.0f32.to_le_bytes());
        data[16..20].copy_from_slice(&1u32.to_le_bytes());
        let mut compressor: FrameCompressor = FrameCompressor::new(CompressionLevel::from_level(3));
        compressor.set_dictionary_from_bytes(&dictionary).unwrap();
        let packed = compressor.compress_independent_frame(&data);
        std::fs::write(opts.join("Pack/Actor/Enemy.pack.zs"), &packed).unwrap();

        std::fs::write(
            root.join("romfs_opts/enemy_speed_manifest.json"),
            r#"{"version": 1, "packs": [{"path": "Pack/Actor/Enemy.pack.zs", "fields": [
                {"o": 0, "k": "m", "v": 2.0},
                {"o": 8, "k": "a", "v": 100.0},
                {"o": 16, "k": "o", "v": 1.0}
            ]}]}"#,
        )
        .unwrap();
        data
    }

    /// A small but genuine zstd dictionary, built from a corpus with the crate's own trainer.
    fn trained_dictionary() -> Vec<u8> {
        use structured_zstd::dictionary::{FinalizeOptions, finalize_raw_dict};
        let mut samples = Vec::new();
        let mut sizes = Vec::new();
        for i in 0..64u32 {
            let line =
                format!("Pack/Actor/Enemy.pack.zs movement=2.0 attack=100.0 flag=1 id={i}\n");
            sizes.push(line.len());
            samples.extend_from_slice(line.as_bytes());
        }
        let content = b"Pack/Actor/Enemy.pack.zs movement= attack= flag= id=";
        finalize_raw_dict(content, &samples, &sizes, 1024, FinalizeOptions::default())
            .expect("a dictionary from the corpus")
    }

    fn read_pack(path: &Path, dictionary: &[u8]) -> Vec<u8> {
        use std::io::Read;
        let compressed = std::fs::read(path).unwrap();
        let mut source = compressed.as_slice();
        let mut decoder =
            StreamingDecoder::new_with_dictionary_bytes(&mut source, dictionary).unwrap();
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).unwrap();
        out
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("shb-rs-enemy-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn levels_and_factors() {
        assert_eq!(factor(2), Some(2.0));
        assert_eq!(factor(10), Some(0.5));
        assert_eq!(factor(0), None);
        let legacy = Settings {
            enemy_speed: true,
            ..Settings::default()
        };
        assert_eq!(
            enemy_levels(&legacy),
            (2, 2, 2, 0),
            "the deprecated flag means the three at x2"
        );
        let s = Settings {
            enemy_move: 3,
            boss_move: 1,
            ..Settings::default()
        };
        assert_eq!(enemy_levels(&s), (3, 0, 0, 0));
        assert_eq!(boss_levels(&s), (1, 0));
    }

    #[test]
    fn packs_are_scaled_and_written_back_with_the_dictionary() {
        let root = temp_dir("scaled");
        let data = fixture(&root);
        let romfs = root.join("romfs");
        // The fixture itself must read back before the install is judged.
        let dictionary = std::fs::read(root.join("romfs_opts/enemy_speed_pack.zsdic")).unwrap();
        let packed =
            std::fs::read(root.join("romfs_opts/enemy_speed_base/Pack/Actor/Enemy.pack.zs"))
                .unwrap();
        let raw = decompress(&packed, &dictionary).expect("the fixture round-trips");
        assert_eq!(raw.len(), 64);
        let s = Settings {
            enemy_move: 2,    // x2
            enemy_attack: 10, // x0.5, and the interval is divided: x2
            enemy_aggro: 2,   // the flag is cleared
            ..Settings::default()
        };
        let notes = install(&romfs, &s, &root).unwrap();
        assert!(notes.is_empty(), "{notes:?}");

        let dictionary = std::fs::read(root.join("romfs_opts/enemy_speed_pack.zsdic")).unwrap();
        let patched = read_pack(&romfs.join("Pack/Actor/Enemy.pack.zs"), &dictionary);
        assert_eq!(patched.len(), data.len());
        assert_eq!(f32::from_le_bytes(patched[0..4].try_into().unwrap()), 4.0);
        assert_eq!(
            f32::from_le_bytes(patched[8..12].try_into().unwrap()),
            200.0
        );
        assert_eq!(u32::from_le_bytes(patched[16..20].try_into().unwrap()), 0);
        // Untouched bytes stay as they were.
        assert_eq!(&patched[4..8], &data[4..8]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn nothing_selected_installs_nothing() {
        let root = temp_dir("none");
        fixture(&root);
        let romfs = root.join("romfs");
        let notes = install(&romfs, &Settings::default(), &root).unwrap();
        assert!(notes.is_empty());
        assert!(!romfs.exists(), "no romfs folder for nothing");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_payload_is_reported_not_fatal() {
        let root = temp_dir("missing");
        let romfs = root.join("romfs");
        let s = Settings {
            enemy_move: 2,
            ..Settings::default()
        };
        let notes = install(&romfs, &s, &root).unwrap();
        assert_eq!(notes.len(), 1, "{notes:?}");
        let _ = std::fs::remove_dir_all(root);
    }
}
