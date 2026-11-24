//! The runtime configuration block (`core/config.h`), patched byte by byte inside the prebuilt module.
//!
//! The module keeps a `ShbConfig` struct in its `.data`, found by its `SHBCFG01` magic. The installer writes
//! the fields by offset, which is exactly what the Python app did: the layout is append-only, so an app never
//! writes past the size the module reports, and a newer module keeps working with an older app.
//!
//! Offsets are relative to the magic. `off` below is the single place that knows them: everything else is
//! written through `Fields`, which keeps the bounds checks in one spot.

use crate::error::{Error, Result, read, write};
use crate::roles::{self, Role};
use crate::settings::{LMode, Settings};
use std::path::Path;

pub const MAGIC: &[u8; 8] = b"SHBCFG01";
/// The smallest block this app understands (0.4.x).
const MIN_SIZE: u32 = 0x48;

/// Byte offsets of every field, relative to the magic (the C struct, in order).
pub mod off {
    pub const STAMINA_ENABLED: usize = 16;
    pub const PRESET: usize = 17;
    pub const COST_JUMP: usize = 18;
    pub const COST_ATTACK: usize = 19;
    pub const COST_SPRINT: usize = 20;
    pub const COST_BLOCK: usize = 21;
    pub const COST_SHOOT: usize = 22;
    pub const COST_ROLL: usize = 23;
    pub const COMBAT_ONLY: usize = 24;
    pub const BM_LAYOUT: usize = 25;
    pub const ENEMY_HP_PCT: usize = 26; // u16
    pub const DAMAGE_TAKEN_PCT: usize = 28; // u16
    pub const HUD_RUPEES: usize = 30;
    pub const HUD_CLOCK: usize = 31;
    pub const CLOCK_24H: usize = 32;
    pub const NO_HEART_DROP: usize = 33;
    pub const SPRINT_SPEED_PCT: usize = 34; // u16
    pub const SONGBOOK: usize = 36;
    pub const BUTTON_MAPPING: usize = 37;
    pub const BUTTON_STYLE: usize = 38;
    pub const BM_SIMPLE: usize = 39;
    pub const BM_L_MODE: usize = 40;
    pub const QUICK_BOOTS: usize = 41;
    pub const QUICK_MASK: usize = 42;
    pub const HUD_COLLECTIBLES: usize = 43;
    pub const HUD_X: usize = 44; // i16[4]
    pub const STAMINA_BAR: usize = 52;
    pub const HUD_FLAGS: usize = 53;
    pub const HUD_MODE: usize = 54;
    pub const HUD_SX: usize = 55;
    pub const HUD_SY: usize = 56;
    pub const HUD_TSCALE: usize = 57;
    pub const HUD_ISCALE: usize = 58;
    pub const HUD_PLATES: usize = 59;
    pub const HUD_MAP_HEARTS: usize = 60;
    pub const HUD_Y: usize = 62; // i16 (61 is the struct's padding)
    pub const HUD_IOFF: usize = 64; // i16
    pub const HUD_TX: usize = 66; // i16
    pub const HUD_IOFF2: usize = 68; // i16
    pub const MANUAL_SHIELD: usize = 70;
    pub const TARGET_R3: usize = 71;
    pub const SPRINT_L3: usize = 72;
    pub const AUTO_TARGET: usize = 73;
    pub const SWITCH_TARGET: usize = 74;
    pub const LANG: usize = 75;
    pub const NO_MAGIC_DROP: usize = 76;
    pub const HIDE_MINIMAP: usize = 77;
    pub const SPRINT_L3_TOGGLE: usize = 78;
    pub const BM_REMAP: usize = 79;
    pub const BM_ROLE: usize = 80; // u8[10]
    pub const HUD_CTRL_SCALE: usize = 90;
    pub const HUD_COUNT_SCALE: usize = 91;
    pub const BM_ROLE_DR: usize = 92;
    pub const BM_HOLD: usize = 93; // u8[4]
    /// Custom item HUD (our B / L / R / ZR buttons); 0 when the button mapping is off.
    pub const HUD_CUSTOM_ITEMS: usize = 97;
    /// Long press of L and R (`bm_hold_lr[2]`), then `bm_hold_wait` (a bit per held button).
    pub const BM_HOLD_LR: usize = 98;
    pub const BM_HOLD_WAIT: usize = 100;
    /// `sizeof(ShbConfig)` without its tail padding: the last byte we write is `BM_HOLD_WAIT`.
    pub const SIZE: usize = 101;
}

/// The simple / plain row is drawn at full size with its own spacing: the x of the first block and how much room each
/// block takes before the next one starts (rupee 250, clock 240, skulltulas/hearts 230).
const HUD_SIMPLE_X0: f64 = -1685.0;
const HUD_SIMPLE_ADV: [f64; 4] = [250.0, 240.0, 230.0, 230.0];

/// HUD row geometry (layout units): `hud.c` draws the plates at 0.46 / 0.42, so the installer packs the enabled
/// blocks with the same gap between plate edges.
const HUD_X0: f64 = -1518.0;
const HUD_GAP: f64 = 24.0;
const HUD_PLATE_W: [f64; 4] = [466.4, 466.4, 425.9, 425.9];

pub const HUD_SCALE_MIN: u8 = 50;
pub const HUD_SCALE_MAX: u8 = 200;
pub const HUD_SCALE_STEP: u8 = 5;

/// Where the block lives in a module image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockLocation {
    pub offset: usize,
    pub version: u32,
    pub size: u32,
}

/// Find the single config block of a module image and validate its header.
pub fn locate(nso: &[u8]) -> Result<BlockLocation> {
    let count = nso.windows(8).filter(|w| *w == MAGIC).count();
    if count != 1 {
        return Err(Error::ConfigBlock(if count == 0 {
            "not found"
        } else {
            "appears twice"
        }));
    }
    let offset = nso
        .windows(8)
        .position(|w| w == MAGIC)
        .expect("counted above");
    if offset + 16 > nso.len() {
        return Err(Error::ConfigBlock("truncated"));
    }
    let version = u32::from_le_bytes(nso[offset + 8..offset + 12].try_into().unwrap());
    let size = u32::from_le_bytes(nso[offset + 12..offset + 16].try_into().unwrap());
    if version != 1 {
        return Err(Error::ConfigVersion { version, size });
    }
    if size < MIN_SIZE {
        return Err(Error::ConfigVersion { version, size });
    }
    Ok(BlockLocation {
        offset,
        version,
        size,
    })
}

/// Byte writer over the block's fields, with the offset table in one place and the module's size enforced.
struct Fields<'a> {
    block: &'a mut [u8],
}

impl<'a> Fields<'a> {
    fn new(block: &'a mut [u8]) -> Self {
        Fields { block }
    }

    fn u8(&mut self, offset: usize, value: u8) {
        if offset < self.block.len() {
            self.block[offset] = value;
        }
    }

    fn u8s(&mut self, at: usize, values: &[u8]) {
        for (i, value) in values.iter().enumerate() {
            self.u8(at + i, *value);
        }
    }

    fn bool(&mut self, offset: usize, value: bool) {
        self.u8(offset, u8::from(value));
    }

    fn u16(&mut self, offset: usize, value: u16) {
        if offset + 2 <= self.block.len() {
            self.block[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }

    fn i16(&mut self, offset: usize, value: i16) {
        if offset + 2 <= self.block.len() {
            self.block[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }

    fn i16s(&mut self, offset: usize, values: &[i16]) {
        for (i, value) in values.iter().enumerate() {
            self.i16(offset + i * 2, *value);
        }
    }
}

/// Python's `round()`: half to even (`-1027.5` → `-1028`, `0.5` → `0`). Rust's `f64::round` rounds half away
/// from zero, which would put a HUD block one unit off, so the parity with the Python app needs this.
fn py_round(value: f64) -> i64 {
    let floor = value.floor();
    if value - floor == 0.5 {
        // A tie goes to the even neighbour (Python's round()), not away from zero.
        if (floor as i64) % 2 == 0 {
            return floor as i64;
        }
        return (floor + 1.0) as i64;
    }
    value.round() as i64
}

/// The HUD scale percent clamped to the slider's range (the module reads 0 as 100).
pub fn hud_scale(value: i32) -> u8 {
    value.clamp(HUD_SCALE_MIN as i32, HUD_SCALE_MAX as i32) as u8
}

/// `off` for a percent value the user typed (the Python app rounded a float).
pub fn hud_scale_rounded(value: f64) -> u8 {
    hud_scale(py_round(value) as i32)
}

/// Write the user's settings into the module image. Returns a new image; the input is not modified.
pub fn patch_config(nso: &[u8], s: &Settings) -> Result<Vec<u8>> {
    let loc = locate(nso)?;
    // The fields we may touch: everything the module says it has, and never past the end of the image.
    let end = (loc.offset + loc.size as usize).min(nso.len());
    let mut image = nso.to_vec();
    {
        let mut f = Fields::new(&mut image[loc.offset..end]);
        let size = loc.size;

        // ---- always present (0.4.x layout) ----
        f.bool(off::STAMINA_ENABLED, s.stamina);
        f.u8(off::PRESET, s.preset);
        f.bool(off::COST_JUMP, s.jump);
        f.bool(off::COST_ATTACK, s.attack);
        f.bool(off::COST_SPRINT, s.sprint);
        f.bool(off::COST_BLOCK, s.block);
        f.bool(off::COST_SHOOT, s.shoot);
        f.bool(off::COST_ROLL, s.roll);
        f.bool(off::COMBAT_ONLY, s.combat_only);
        f.u8(off::BM_LAYOUT, bm_layout_byte(s));
        f.u16(off::ENEMY_HP_PCT, s.enemy_hp);
        f.u16(off::DAMAGE_TAKEN_PCT, s.damage);
        f.bool(off::CLOCK_24H, s.clock_24h);
        f.bool(off::NO_HEART_DROP, s.no_heart_drop);
        f.u16(off::SPRINT_SPEED_PCT, s.sprint_speed);
        f.bool(off::SONGBOOK, s.songbook);
        f.bool(off::BUTTON_MAPPING, s.button_mapping);
        f.u8(off::BUTTON_STYLE, button_style_byte(s));
        f.bool(off::BM_SIMPLE, s.bm_simple);
        f.u8(off::BM_L_MODE, l_mode_byte(s));
        f.bool(off::QUICK_BOOTS, s.quick_boots);
        f.bool(off::QUICK_MASK, s.quick_mask);

        // ---- HUD layout: four block x positions plus the plate / text / icon scales ----
        let hud = HudLayout::from_settings(s);
        f.bool(off::HUD_RUPEES, hud.rupees);
        f.bool(off::HUD_CLOCK, hud.clock);
        f.bool(off::HUD_COLLECTIBLES, hud.collectibles);
        f.i16s(off::HUD_X, &hud.x);
        f.bool(off::STAMINA_BAR, s.stamina_bar);
        f.u8(off::HUD_FLAGS, 1); // the layout fields below are valid
        f.u8(off::HUD_MODE, hud.mode);
        f.u8(off::HUD_SX, hud.sx);
        f.u8(off::HUD_SY, hud.sy);
        f.u8(off::HUD_TSCALE, hud.tscale);
        f.u8(off::HUD_ISCALE, hud.iscale);
        f.u8(off::HUD_PLATES, hud.plates);
        f.u8(off::HUD_MAP_HEARTS, hud.map_hearts);
        f.i16(off::HUD_Y, hud.y);
        f.i16(off::HUD_IOFF, hud.ioff);
        f.i16(off::HUD_TX, hud.tx);
        f.i16(off::HUD_IOFF2, hud.ioff2);
        f.bool(off::MANUAL_SHIELD, s.manual_shield);

        // ---- appended bytes: only when the module is new enough ----
        if size >= 0x49 {
            f.bool(off::TARGET_R3, s.target_r3);
            f.bool(off::SPRINT_L3, s.sprint_l3);
            if size >= 0x4C {
                f.bool(off::AUTO_TARGET, s.auto_target);
                f.bool(off::SWITCH_TARGET, s.switch_target);
                f.u8(off::LANG, s.lang.byte());
                if size >= 0x50 {
                    f.bool(off::NO_MAGIC_DROP, s.no_magic_drop);
                    f.bool(off::HIDE_MINIMAP, s.hide_minimap);
                    f.bool(off::SPRINT_L3_TOGGLE, s.sprint_l3_toggle);
                    if size >= 0x5A && s.bm_remap && s.button_mapping {
                        write_remap(&mut f, s);
                    } else if size >= 0x5A {
                        // Button mapping off: nothing that rides on it may stay active in the module. The table stays
                        // in the app's own settings, so turning it back on restores it; the module gets the plain
                        // controls.
                        f.u8(off::BM_REMAP, 0);
                        f.u8(off::BM_L_MODE, 0); // L keeps the module's default (the table is what decides)
                        f.bool(off::MANUAL_SHIELD, false);
                        f.bool(off::TARGET_R3, false);
                        f.bool(off::SPRINT_L3, false);
                        f.bool(off::AUTO_TARGET, false);
                        f.bool(off::SWITCH_TARGET, false);
                        f.bool(off::SPRINT_L3_TOGGLE, false);
                        if size >= 0x5D {
                            f.u8(off::BM_ROLE_DR, 0);
                        }
                        if size >= 0x61 {
                            f.u8s(off::BM_HOLD, &[0; 4]);
                        }
                        if size >= 0x65 {
                            f.u8s(off::BM_HOLD_LR, &[0; 3]);
                        }
                    }
                }
            }
        }
        if size >= 0x5C {
            f.u8(off::HUD_CTRL_SCALE, hud_scale(s.hud_ctrl_scale as i32));
            f.u8(off::HUD_COUNT_SCALE, hud_scale(s.hud_count_scale as i32));
        }
        if size >= 0x62 {
            // The custom item HUD only exists with the mapping: no mapping, no item side.
            f.bool(off::HUD_CUSTOM_ITEMS, s.hud_items && s.button_mapping);
        }
    }
    Ok(image)
}

/// The button remap block, plus the options the table implies. It runs after the HUD and the control bytes
/// because it overrides some of them (the same order the Python app used).
fn write_remap(f: &mut Fields<'_>, s: &Settings) {
    let mut table = roles::normalize(&s.bm_roles);
    if !s.manual_shield {
        // The Shield role only works with the manual shield.
        for role in table.values_mut() {
            if *role == Role::Shield {
                *role = Role::None;
            }
        }
    }
    f.u8(off::BM_REMAP, 1);
    for (i, button) in roles::REMAP_BUTTONS[..10].iter().enumerate() {
        let role = table.get(button).copied().unwrap_or(Role::None);
        f.u8(off::BM_ROLE + i, role.index());
    }
    // bm_role_dr and bm_hold: the long presses, appended after the HUD scales.
    let block_len = f.block.len();
    if block_len > off::BM_ROLE_DR {
        let role = table
            .get(&crate::roles::Button::Dr)
            .copied()
            .unwrap_or(Role::None);
        f.u8(off::BM_ROLE_DR, role.index());
    }
    if block_len >= off::BM_HOLD + 4 {
        for (i, key) in roles::HOLD_KEYS[..4].iter().enumerate() {
            let role = table.get(key).copied().unwrap_or(Role::None);
            f.u8(off::BM_HOLD + i, role.index());
        }
    }
    // The long press of L and R, and how each held button tells a short press from a long one.
    if block_len >= off::SIZE {
        for (i, key) in [crate::roles::Button::LHold, crate::roles::Button::RHold]
            .iter()
            .enumerate()
        {
            let role = table.get(key).copied().unwrap_or(Role::None);
            f.u8(off::BM_HOLD_LR + i, role.index());
        }
        f.u8(off::BM_HOLD_WAIT, roles::hold_wait_byte(&s.bm_hold_wait));
    }
    // The options the table implies (they used to be separate switches).
    let flags = roles::roles_flags(&table);
    if s.bm_layout != crate::settings::BmLayout::Gamecube {
        f.u8(off::BM_LAYOUT, 0); // preserve prior custom-remap geometry
    }
    f.u8(off::BM_L_MODE, 0); // the selector is a role (VBTN_SELECTOR), not L
    f.bool(off::MANUAL_SHIELD, s.manual_shield);
    f.u8(off::TARGET_R3, 0); // target can be on R3 itself
    f.bool(off::SPRINT_L3, flags.sprint_l3);
}

fn bm_layout_byte(s: &Settings) -> u8 {
    match s.bm_layout {
        crate::settings::BmLayout::Nintendo => 0,
        crate::settings::BmLayout::Ab => 1,
        crate::settings::BmLayout::Xbox => 2,
        crate::settings::BmLayout::Gamecube => 3,
    }
}

fn button_style_byte(s: &Settings) -> u8 {
    match s.button_style {
        crate::settings::ButtonStyle::Switch => 0,
        crate::settings::ButtonStyle::Xbox => 1,
        crate::settings::ButtonStyle::PlayStation => 2,
    }
}

fn l_mode_byte(s: &Settings) -> u8 {
    match s.bm_l_mode {
        LMode::Assign => 0,
        LMode::Selector => 2,
    }
}

/// The four counters and the layout numbers for the chosen HUD mode.
struct HudLayout {
    rupees: bool,
    clock: bool,
    collectibles: bool,
    x: [i16; 4],
    mode: u8,
    sx: u8,
    sy: u8,
    tscale: u8,
    iscale: u8,
    plates: u8,
    map_hearts: u8,
    y: i16,
    ioff: i16,
    tx: i16,
    ioff2: i16,
}

impl HudLayout {
    fn from_settings(s: &Settings) -> Self {
        use crate::settings::HudMode;
        match s.hud_mode {
            // One row behind a soft translucent bar, or the same row with no bar at all.
            HudMode::Simple | HudMode::Plain => {
                // The row keeps the player's element switches and packs what is left: a counter that is off is gone
                // and the others move left (the soft bar behind them follows: core/hud.c sizes it from the ends).
                let mut x = [-32768i16; 4];
                let mut slots: Vec<usize> = Vec::new();
                if s.hud_rupees {
                    slots.push(0);
                }
                if s.hud_clock {
                    slots.push(1);
                }
                if s.hud_collectibles {
                    slots.extend([2, 3]);
                }
                let mut cursor = HUD_SIMPLE_X0;
                for block in &slots {
                    x[*block] = py_round(cursor) as i16;
                    cursor += HUD_SIMPLE_ADV[*block];
                }
                HudLayout {
                    rupees: s.hud_rupees,
                    clock: s.hud_clock,
                    collectibles: s.hud_collectibles,
                    x,
                    mode: if s.hud_mode == HudMode::Simple { 1 } else { 2 },
                    sx: 100,
                    sy: 48,
                    tscale: 50,
                    iscale: 45,
                    plates: if s.hud_mode == HudMode::Simple { 2 } else { 0 },
                    map_hearts: 0,
                    y: -1010,
                    ioff: -140,
                    tx: 10,
                    ioff2: -130,
                }
            }
            // The wide layout keeps the user's element switches and packs the enabled blocks with an equal gap.
            HudMode::Wide => {
                let mut x = [-32768i16; 4]; // -32768 = off
                let mut slots: Vec<usize> = Vec::new();
                if s.hud_rupees {
                    slots.push(0);
                }
                if s.hud_clock {
                    slots.push(1);
                }
                if s.hud_collectibles {
                    slots.extend([2, 3]);
                }
                let mut cursor = HUD_X0;
                for (i, block) in slots.iter().enumerate() {
                    x[*block] = py_round(cursor) as i16;
                    if i + 1 < slots.len() {
                        cursor += (HUD_PLATE_W[*block] + HUD_PLATE_W[slots[i + 1]]) * 0.5 + HUD_GAP;
                    }
                }
                HudLayout {
                    rupees: s.hud_rupees,
                    clock: s.hud_clock,
                    collectibles: s.hud_collectibles,
                    x,
                    mode: 0,
                    sx: 46,
                    sy: 62,
                    tscale: 0,
                    iscale: 85,
                    plates: 1,
                    map_hearts: 0,
                    y: -960,
                    ioff: -165,
                    tx: 30,
                    ioff2: -165,
                }
            }
        }
    }
}

impl Role {
    /// The byte the module reads for this role (`ROLE_*`).
    pub fn index(self) -> u8 {
        Role::ALL
            .iter()
            .position(|r| *r == self)
            .expect("role is in ALL") as u8
    }
}

/// What the module's config block currently says, for `shb-cli show`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledConfig {
    pub stamina: bool,
    pub preset: u8,
    pub enemy_hp: u16,
    pub damage: u16,
    pub sprint_speed: u16,
    pub hud_mode: u8,
    pub lang: u8,
    pub bm_remap: bool,
    pub roles: Vec<(crate::roles::Button, u8)>,
}

pub fn read_config(nso: &[u8]) -> Result<InstalledConfig> {
    let loc = locate(nso)?;
    let b = &nso[loc.offset..(loc.offset + loc.size as usize).min(nso.len())];
    let u8_at = |o: usize| b.get(o).copied().unwrap_or(0);
    let u16_at = |o: usize| -> u16 {
        b.get(o..o + 2)
            .map(|s| u16::from_le_bytes([s[0], s[1]]))
            .unwrap_or(0)
    };
    Ok(InstalledConfig {
        stamina: u8_at(off::STAMINA_ENABLED) != 0,
        preset: u8_at(off::PRESET),
        enemy_hp: u16_at(off::ENEMY_HP_PCT),
        damage: u16_at(off::DAMAGE_TAKEN_PCT),
        sprint_speed: u16_at(off::SPRINT_SPEED_PCT),
        hud_mode: u8_at(off::HUD_MODE),
        lang: u8_at(off::LANG),
        bm_remap: u8_at(off::BM_REMAP) != 0,
        roles: roles::REMAP_BUTTONS[..10]
            .iter()
            .enumerate()
            .map(|(i, b_)| (*b_, u8_at(off::BM_ROLE + i)))
            .collect(),
    })
}

/// Patch only the HUD-language byte of an already installed module: changing the language does not need a
/// reinstall, it takes effect the next time the game starts.
pub fn set_installed_lang(module: &Path, lang: crate::settings::Lang) -> Result<bool> {
    let mut data = match read(module) {
        Ok(d) => d,
        Err(_) => return Ok(false),
    };
    let loc = match locate(&data) {
        Ok(l) => l,
        Err(_) => return Ok(false),
    };
    let size = loc.size as usize;
    if size < 0x4C || loc.offset + size > data.len() {
        return Ok(false);
    }
    data[loc.offset + off::LANG] = lang.byte();
    write(module, &data)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{BmLayout, ButtonStyle, HudMode, Lang};

    /// Where the fake module keeps its block.
    const BLOCK: usize = 0x400;

    /// A fake module: some code, then a config block of the module's real layout.
    fn module(size: usize) -> Vec<u8> {
        let mut image = vec![0xAAu8; 0x1000];
        let block = BLOCK;
        image[block..block + 8].copy_from_slice(MAGIC);
        image[block + 8..block + 12].copy_from_slice(&1u32.to_le_bytes());
        image[block + 12..block + 16].copy_from_slice(&(size as u32).to_le_bytes());
        image
    }

    fn read_u8(image: &[u8], offset: usize) -> u8 {
        image[BLOCK + offset]
    }

    fn read_i16(image: &[u8], offset: usize) -> i16 {
        let o = BLOCK + offset;
        i16::from_le_bytes([image[o], image[o + 1]])
    }

    #[test]
    fn magic_is_found_once() {
        let image = module(0x61);
        let loc = locate(&image).unwrap();
        assert_eq!(loc.size, 0x61);
        assert_eq!(loc.version, 1);

        let mut twice = image.clone();
        let block = BLOCK;
        twice[block + 0x40..block + 0x48].copy_from_slice(MAGIC);
        assert!(matches!(locate(&twice), Err(Error::ConfigBlock(_))));
        assert!(matches!(locate(&[0u8; 64]), Err(Error::ConfigBlock(_))));
    }

    #[test]
    fn fields_land_on_their_offsets() {
        let s = Settings {
            stamina: false,
            preset: 2,
            enemy_hp: 150,
            damage: 200,
            sprint_speed: 140,
            hud_mode: HudMode::Wide,
            hud_rupees: true,
            hud_clock: false,
            hud_collectibles: true,
            lang: Lang::Es,
            button_style: ButtonStyle::PlayStation,
            bm_layout: BmLayout::Xbox,
            manual_shield: true,
            hud_ctrl_scale: 120,
            hud_count_scale: 80,
            ..Settings::default()
        };
        let image = patch_config(&module(0x61), &s).unwrap();

        assert_eq!(read_u8(&image, off::STAMINA_ENABLED), 0);
        assert_eq!(read_u8(&image, off::PRESET), 2);
        assert_eq!(read_u8(&image, off::ENEMY_HP_PCT), 150);
        assert_eq!(read_u8(&image, off::DAMAGE_TAKEN_PCT), 200);
        assert_eq!(read_u8(&image, off::SPRINT_SPEED_PCT), 140);
        assert_eq!(read_u8(&image, off::HUD_RUPEES), 1);
        assert_eq!(read_u8(&image, off::HUD_CLOCK), 0);
        assert_eq!(read_u8(&image, off::HUD_COLLECTIBLES), 1);
        assert_eq!(read_u8(&image, off::HUD_FLAGS), 1);
        assert_eq!(read_u8(&image, off::LANG), 1);
        assert_eq!(read_u8(&image, off::BUTTON_STYLE), 2);
        assert_eq!(read_u8(&image, off::BUTTON_MAPPING), 1);
        assert_eq!(read_u8(&image, off::MANUAL_SHIELD), 1);
        assert_eq!(read_u8(&image, off::HUD_CTRL_SCALE), 120);
        assert_eq!(read_u8(&image, off::HUD_COUNT_SCALE), 80);
        // Rupee block first, then the two collectible plates: two plates apart plus the gap.
        assert_eq!(read_i16(&image, off::HUD_X), -1518);
        assert_eq!(read_i16(&image, off::HUD_X + 2), -32768, "the clock is off");
        assert_eq!(read_i16(&image, off::HUD_X + 4), -1048);
        assert_eq!(read_i16(&image, off::HUD_X + 6), -598);
    }

    #[test]
    fn an_old_block_only_gets_the_fields_it_has() {
        // A 0.4.x block (0x48 bytes) has neither the control bytes nor the remap: nothing past the end is written.
        let image = patch_config(&module(0x48), &Settings::default()).unwrap();
        let block = BLOCK;
        assert_eq!(
            image[block + 0x48],
            0xAA,
            "the byte after the block is untouched"
        );
        assert!(read_config(&image).unwrap().enemy_hp == 100);
    }

    #[test]
    fn python_rounding_is_half_to_even() {
        assert_eq!(py_round(-1027.5), -1028);
        assert_eq!(py_round(0.5), 0);
        assert_eq!(py_round(1.5), 2);
        assert_eq!(py_round(-1518.0), -1518);
        assert_eq!(py_round(-617.6), -618);
    }

    #[test]
    fn hud_scale_is_clamped() {
        assert_eq!(hud_scale(10), HUD_SCALE_MIN);
        assert_eq!(hud_scale(500), HUD_SCALE_MAX);
        assert_eq!(hud_scale(100), 100);
    }

    #[test]
    fn the_remap_block_is_written_when_the_module_has_it() {
        let s = Settings {
            bm_remap: true,
            bm_roles: roles::remap_preset("triggers").unwrap(),
            manual_shield: true,
            ..Settings::default()
        };
        let image = patch_config(&module(0x61), &s).unwrap();
        assert_eq!(read_u8(&image, off::BM_REMAP), 1);
        assert_eq!(read_u8(&image, off::BM_ROLE), Role::Shield.index()); // L
        assert_eq!(read_u8(&image, off::BM_ROLE + 6), Role::Selector.index()); // B
        assert_eq!(read_u8(&image, off::BM_ROLE_DR), Role::None.index());
        assert_eq!(read_u8(&image, off::BM_HOLD + 1), Role::None.index()); // held B: B is the selector, no hold
        assert_eq!(read_u8(&image, off::BM_HOLD + 2), Role::Back.index()); // held X
        // The table implies the sprint (triggers has one on L3) and drops the separate options.
        assert_eq!(read_u8(&image, off::SPRINT_L3), 1);
        assert_eq!(read_u8(&image, off::TARGET_R3), 0);
        assert_eq!(read_u8(&image, off::BM_L_MODE), 0);
    }

    #[test]
    fn shield_without_the_manual_shield_is_dropped() {
        let s = Settings {
            manual_shield: false,
            bm_roles: roles::remap_preset("shield").unwrap(),
            ..Settings::default()
        };
        let image = patch_config(&module(0x61), &s).unwrap();
        assert_eq!(
            read_u8(&image, off::BM_ROLE),
            Role::None.index(),
            "L was the shield"
        );
    }
}
