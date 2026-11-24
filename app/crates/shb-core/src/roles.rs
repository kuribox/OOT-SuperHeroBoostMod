//! Button remap: which pad button does what (`core/remap.h` on the module side).
//!
//! The module reads one byte per physical button (`bm_role[]`, plus the D-pad right long press in `bm_role_dr`
//! and the held face buttons in `bm_hold[]`), where the byte is the index of the role in `ROLE_VALUES`. The
//! installer's job is to turn a user table into bytes that the module accepts, which is what this module does:
//! one role per button at most, only where the module allows it, with the classic table as the fallback.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            /// Every variant, in the order the module expects (the byte value is the index).
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|v| v.as_str() == s)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    /// What a button does (core/remap.h `ROLE_*`). The order is the byte value written to the module.
    Role {
        None => "none",
        Jump => "jump",
        Sword => "sword",
        Action => "action",
        Target => "target",
        Item1 => "item1",
        Item2 => "item2",
        Item3 => "item3",
        ItemMain => "item_main",
        Camera => "camera",
        Sprint => "sprint",
        Shield => "shield",
        Selector => "selector",
        Armory => "armory",
        Back => "back",
    }
}

string_enum! {
    /// A physical button, in the module's order (the config bytes `bm_role[0..10]`).
    ///
    /// `Dr` is the D-pad's right arrow: its role is the LONG press (`bm_role_dr`), a short press stays the
    /// game's Navi call. The `*_hold` entries are the long press of A / B / X / Y (`bm_hold[0..3]`): they are not
    /// buttons of the pad, so they carry no HUD art and cannot hold an item.
    Button {
        L => "L",
        X => "X",
        R => "R",
        Y => "Y",
        A => "A",
        Zr => "ZR",
        B => "B",
        Zl => "ZL",
        L3 => "L3",
        R3 => "R3",
        Dr => "DR",
        AHold => "A_hold",
        BHold => "B_hold",
        XHold => "X_hold",
        YHold => "Y_hold",
        LHold => "L_hold",
        RHold => "R_hold",
    }
}

pub const REMAP_BUTTONS: [Button; 11] = [
    Button::L,
    Button::X,
    Button::R,
    Button::Y,
    Button::A,
    Button::Zr,
    Button::B,
    Button::Zl,
    Button::L3,
    Button::R3,
    Button::Dr,
];
/// The long press of A, B, X, Y, L and R: rows of the table, not buttons of the pad.
pub const HOLD_KEYS: [Button; 6] = [
    Button::AHold,
    Button::BHold,
    Button::XHold,
    Button::YHold,
    Button::LHold,
    Button::RHold,
];

/// The defaults of `bm_hold_wait`: the arrow waits (its short press is the Navi call), the others act at once.
pub fn default_hold_wait() -> BTreeMap<Button, bool> {
    HOLD_WAIT_KEYS
        .iter()
        .map(|button| (*button, *button == Button::Dr))
        .collect()
}

/// `bm_hold_wait` as the module reads it: a bit per `HOLD_WAIT_KEYS` entry (a missing button takes its default).
/// Bit 0 (D-pad right) is always set: its short press is the game's Navi call and has to wait for the hold to know
/// which press it is, so that is not a choice of the player's.
pub fn hold_wait_byte(wait: &BTreeMap<Button, bool>) -> u8 {
    let mut have = default_hold_wait();
    have.extend(wait.iter().map(|(b, v)| (*b, *v)));
    let bits = HOLD_WAIT_KEYS
        .iter()
        .enumerate()
        .filter(|(_, button)| have.get(button).copied().unwrap_or(false))
        .fold(0u8, |bits, (i, _)| bits | (1 << i));
    bits | 1
}

/// The button a long press belongs to (`A_hold` -> `A`).
pub fn hold_owner(button: Button) -> Option<Button> {
    match button {
        Button::AHold => Some(Button::A),
        Button::BHold => Some(Button::B),
        Button::XHold => Some(Button::X),
        Button::YHold => Some(Button::Y),
        Button::LHold => Some(Button::L),
        Button::RHold => Some(Button::R),
        _ => None,
    }
}

/// The buttons that choose how a short press is told from a long one (`bm_hold_wait`, one bit each, in this order).
/// D-pad right waits by default: its short press is the game's Navi call.
pub const HOLD_WAIT_KEYS: [Button; 7] = [
    Button::Dr,
    Button::A,
    Button::B,
    Button::X,
    Button::Y,
    Button::L,
    Button::R,
];

/// Roles that act held or at once: a button that has one has no long press, and none of them is a hold role.
/// D-pad right is the exception: its short press is Navi's, not a role of ours.
pub const HOLD_BLOCK_ROLES: [Role; 9] = [
    Role::Shield,
    Role::Target,
    Role::Sprint,
    Role::Selector,
    Role::Armory,
    Role::Item1,
    Role::Item2,
    Role::Item3,
    Role::ItemMain,
];

/// Roles that may sit on two buttons: the game needs one of each, so a second one is a shortcut.
pub const DUP_ROLES: [Role; 5] = [
    Role::Jump,
    Role::Sword,
    Role::Selector,
    Role::Armory,
    Role::Back,
];

/// The game's own basic actions: the controls list says which of them have no button at all.
pub const BASE_ROLES: [Role; 4] = [Role::Jump, Role::Sword, Role::Action, Role::Target];
/// Every key of a role table: the buttons, then the long presses.
pub const ROLE_KEYS: [Button; 17] = [
    Button::L,
    Button::X,
    Button::R,
    Button::Y,
    Button::A,
    Button::Zr,
    Button::B,
    Button::Zl,
    Button::L3,
    Button::R3,
    Button::Dr,
    Button::AHold,
    Button::BHold,
    Button::XHold,
    Button::YHold,
    Button::LHold,
    Button::RHold,
];
pub const ITEM_ROLES: [Role; 4] = [Role::Item1, Role::Item2, Role::Item3, Role::ItemMain];

/// Buttons that can carry a role with HUD art: everything but the sticks (and the holds).
const NO_STICKS: [Button; 8] = [
    Button::L,
    Button::X,
    Button::R,
    Button::Y,
    Button::A,
    Button::Zr,
    Button::B,
    Button::Zl,
];
/// Roles the D-pad's right arrow accepts (they need no button art of their own).
const DR_ROLES: [Role; 8] = [
    Role::Jump,
    Role::Sword,
    Role::Camera,
    Role::Sprint,
    Role::Shield,
    Role::Selector,
    Role::Armory,
    Role::Back,
];
/// Roles a held face button accepts.
const HOLD_ROLES: [Role; 5] = [
    Role::Jump,
    Role::Sword,
    Role::Action,
    Role::Camera,
    Role::Back,
];

/// A role table: one entry per key, complete.
pub type Roles = BTreeMap<Button, Role>;

/// The table the module used before the remap existed.
pub fn classic_roles() -> Roles {
    let mut t = Roles::new();
    for (button, role) in [
        (Button::L, Role::Item2),
        (Button::X, Role::Jump),
        (Button::R, Role::Item3),
        (Button::Y, Role::Sword),
        (Button::A, Role::Action),
        (Button::Zr, Role::ItemMain),
        (Button::B, Role::Item1),
        (Button::Zl, Role::Target),
        (Button::L3, Role::None),
        (Button::R3, Role::Camera),
        (Button::Dr, Role::None),
        (Button::AHold, Role::None),
        (Button::BHold, Role::None),
        (Button::XHold, Role::None),
        (Button::YHold, Role::None),
        (Button::LHold, Role::None),
        (Button::RHold, Role::None),
    ] {
        t.insert(button, role);
    }
    t
}

/// The ready-made tables of the app's preset list (`classic`, `action_down`, `shield`, `souls`, `triggers`).
pub fn remap_preset(key: &str) -> Option<Roles> {
    let classic = classic_roles();
    let mut t = classic.clone();
    match key {
        "classic" => {}
        // B no longer sends the game's B in the tables where it is not item 1: a held button does, as the Back role.
        // Here the action sits on B, so Back is on the held X: a preset never asks for a wait on the button it holds.
        "action_down" => {
            t.insert(Button::A, Role::Item1);
            t.insert(Button::B, Role::Action);
            t.insert(Button::XHold, Role::Back);
        }
        "shield" => {
            t.insert(Button::L, Role::Shield);
            t.insert(Button::L3, Role::Sprint);
        }
        "souls" => {
            t.insert(Button::L, Role::Shield);
            t.insert(Button::X, Role::Jump);
            t.insert(Button::R, Role::Item2);
            t.insert(Button::Y, Role::Sword);
            t.insert(Button::A, Role::Action);
            t.insert(Button::Zr, Role::ItemMain);
            t.insert(Button::B, Role::Item1);
            t.insert(Button::Zl, Role::Item3);
            t.insert(Button::L3, Role::Sprint);
            t.insert(Button::R3, Role::Target);
        }
        // The author's own table: items on the triggers, the item selector on B, the shield on L.
        "triggers" => {
            t.insert(Button::L, Role::Shield);
            t.insert(Button::X, Role::Jump);
            t.insert(Button::R, Role::Item1);
            t.insert(Button::Y, Role::Sword);
            t.insert(Button::A, Role::Action);
            t.insert(Button::Zr, Role::ItemMain);
            t.insert(Button::B, Role::Selector);
            t.insert(Button::Zl, Role::Item2);
            t.insert(Button::L3, Role::Sprint);
            t.insert(Button::R3, Role::Target);
            t.insert(Button::XHold, Role::Back);
        }
        _ => return None,
    }
    Some(normalize(&t))
}

pub const REMAP_PRESET_KEYS: [&str; 5] = ["classic", "action_down", "shield", "souls", "triggers"];

/// Can `role` sit on `button`?
pub fn role_allowed(role: Role, button: Button) -> bool {
    if HOLD_KEYS.contains(&button) {
        return role == Role::None || HOLD_ROLES.contains(&role);
    }
    if button == Button::Dr {
        return role == Role::None || DR_ROLES.contains(&role);
    }
    match role {
        // Any button but the sticks.
        Role::Selector => NO_STICKS.contains(&button) || button == Button::Dr,
        Role::Target => matches!(button, Button::Zl | Button::Zr | Button::L3 | Button::R3),
        // A face button only: the game's context prompt draws one of A / B / X / Y.
        Role::Action => matches!(button, Button::A | Button::B | Button::X | Button::Y),
        // Items need button art, so no sticks.
        Role::Item1 | Role::Item2 | Role::Item3 | Role::ItemMain => NO_STICKS.contains(&button),
        _ => true,
    }
}

/// Every role but `None` on one button at most (the first one keeps it), only where it is allowed, unknown or
/// missing buttons falling back to their classic role when it is still free.
pub fn normalize(roles: &Roles) -> Roles {
    let classic = classic_roles();
    let mut out = Roles::new();
    let mut used = std::collections::BTreeSet::new();
    for button in ROLE_KEYS {
        let mut role = roles.get(&button).copied().unwrap_or(classic[&button]);
        if !role_allowed(role, button) {
            role = Role::None;
        }
        // The button's own role acts held or at once: no long press on it.
        if let Some(owner) = hold_owner(button)
            && out
                .get(&owner)
                .is_some_and(|own| HOLD_BLOCK_ROLES.contains(own))
        {
            role = Role::None;
        }
        // One button per role, but the shortcut roles may repeat. `None` means "no role", so it may repeat too.
        if role != Role::None && !DUP_ROLES.contains(&role) && !used.insert(role) {
            role = Role::None;
        }
        out.insert(button, role);
    }
    out
}

/// The table with the armory on the held D-pad right (what the profiles do): the armory sits on no other button.
pub fn roles_with_armory(roles: &Roles) -> Roles {
    let mut t = normalize(roles);
    for role in t.values_mut() {
        if *role == Role::Armory {
            *role = Role::None;
        }
    }
    t.insert(Button::Dr, Role::Armory);
    normalize(&t)
}

/// A preset's table with the user's own long presses (`keep_holds`): a preset only sets the holds it names.
pub fn keep_holds(table: &Roles, current: &Roles) -> Roles {
    let mut out = table.clone();
    out.insert(
        Button::Dr,
        current.get(&Button::Dr).copied().unwrap_or(Role::None),
    );
    for key in HOLD_KEYS {
        let from_table = table.get(&key).copied().unwrap_or(Role::None);
        out.insert(
            key,
            if from_table != Role::None {
                from_table
            } else {
                current.get(&key).copied().unwrap_or(Role::None)
            },
        );
    }
    out
}

/// Bring a table in line with the options a profile sets (`installer.roles_from_flags`): `None` leaves that part as it
/// is. The target on R3 (or back on ZL), the shield on L, the sprint on L3.
pub fn roles_from_flags(
    roles: &Roles,
    manual_shield: Option<bool>,
    target_r3: Option<bool>,
    sprint_l3: Option<bool>,
) -> Roles {
    let mut r = normalize(roles);
    fn holder(r: &Roles, role: Role) -> Option<Button> {
        r.iter().find(|(_, v)| **v == role).map(|(b, _)| *b)
    }
    fn free_for(r: &Roles, role: Role, prefer: &[Button]) -> Option<Button> {
        prefer
            .iter()
            .copied()
            .find(|b| matches!(r[b], Role::None | Role::Camera) && role_allowed(role, *b))
    }
    if let Some(want) = target_r3 {
        let t = holder(&r, Role::Target);
        if want && t != Some(Button::R3) {
            let old = r[&Button::R3];
            r.insert(Button::R3, Role::Target);
            if let Some(t) = t {
                r.insert(
                    t,
                    if role_allowed(old, t) {
                        old
                    } else {
                        Role::None
                    },
                );
            }
        } else if !want && t == Some(Button::R3) {
            let z = r[&Button::Zl];
            r.insert(Button::Zl, Role::Target);
            r.insert(
                Button::R3,
                if role_allowed(z, Button::R3) {
                    z
                } else {
                    Role::Camera
                },
            );
        }
    }
    if let Some(want) = manual_shield {
        let sh = holder(&r, Role::Shield);
        if want && sh.is_none() {
            let displaced = r[&Button::L];
            r.insert(Button::L, Role::Shield);
            if ITEM_ROLES.contains(&displaced) || displaced == Role::Selector {
                let dst = if displaced == Role::Selector {
                    (role_allowed(Role::Selector, Button::R) && r[&Button::R] == Role::None)
                        .then_some(Button::R)
                } else {
                    free_for(&r, displaced, &[Button::Zl, Button::L3, Button::R3])
                };
                if let Some(dst) = dst {
                    r.insert(dst, displaced);
                }
            }
        } else if !want && let Some(sh) = sh {
            let item2_free = !r.values().any(|v| *v == Role::Item2);
            r.insert(sh, if item2_free { Role::Item2 } else { Role::None });
        }
    }
    if let Some(want) = sprint_l3 {
        let sp = holder(&r, Role::Sprint);
        if want && sp.is_none() && r[&Button::L3] == Role::None {
            r.insert(Button::L3, Role::Sprint);
        } else if !want && let Some(sp) = sp {
            r.insert(sp, Role::None);
        }
    }
    normalize(&r)
}

/// The basic roles no button has: the game cannot do those then.
pub fn roles_missing(roles: &Roles) -> Vec<Role> {
    let have = normalize(roles);
    BASE_ROLES
        .into_iter()
        .filter(|r| !have.values().any(|v| v == r))
        .collect()
}

/// The module options a table implies: the L3-style sprint and L as the item selector.
pub fn roles_flags(roles: &Roles) -> RoleFlags {
    let have = normalize(roles);
    let has = |r: Role| have.values().any(|v| *v == r);
    RoleFlags {
        sprint_l3: has(Role::Sprint),
        l_selector: has(Role::Selector),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RoleFlags {
    pub sprint_l3: bool,
    pub l_selector: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_table_is_valid_and_complete() {
        let t = normalize(&classic_roles());
        assert_eq!(t.len(), ROLE_KEYS.len());
        assert!(
            roles_missing(&t).is_empty(),
            "classic has jump, sword, action and target"
        );
        assert_eq!(t[&Button::B], Role::Item1);
        assert_eq!(t[&Button::Zl], Role::Target);
    }

    #[test]
    fn one_role_per_button_and_only_where_allowed() {
        // Two buttons asking for the same item: the first one in the module's order keeps it (L before B).
        let mut t = classic_roles();
        t.insert(Button::L, Role::Item1);
        let t = normalize(&t);
        assert_eq!(t[&Button::L], Role::Item1);
        assert_eq!(
            t[&Button::B],
            Role::None,
            "B loses item1 to L, which comes first"
        );

        // Target cannot sit on a face button.
        let mut t = classic_roles();
        t.insert(Button::Y, Role::Target);
        assert_eq!(normalize(&t)[&Button::Y], Role::None);

        // Items cannot go on the sticks.
        let mut t = classic_roles();
        t.insert(Button::R3, Role::Item1);
        assert_eq!(normalize(&t)[&Button::R3], Role::None);
    }

    #[test]
    fn presets_are_normalized_tables() {
        for key in REMAP_PRESET_KEYS {
            let t = remap_preset(key).unwrap_or_else(|| panic!("missing preset {key}"));
            assert_eq!(normalize(&t), t, "{key} is not normalized");
            assert_eq!(t[&Button::X], Role::Jump);
            assert_eq!(t[&Button::Y], Role::Sword);
            // Every table but `action_down` keeps the game's action on A (the Pokémon-style one moves it to B).
            let action = if key == "action_down" {
                Role::Item1
            } else {
                Role::Action
            };
            assert_eq!(t[&Button::A], action, "{key}");
        }
        assert_eq!(
            remap_preset("triggers").unwrap()[&Button::B],
            Role::Selector
        );
        assert_eq!(remap_preset("souls").unwrap()[&Button::R3], Role::Target);
        assert!(remap_preset("nope").is_none());
    }

    #[test]
    fn armory_moves_to_the_dpad() {
        let mut t = classic_roles();
        t.insert(Button::R, Role::Armory);
        let t = roles_with_armory(&t);
        assert_eq!(t[&Button::Dr], Role::Armory);
        assert_eq!(t[&Button::R], Role::None);
    }

    #[test]
    fn holds_keep_the_users_choice() {
        let mut user = classic_roles();
        user.insert(Button::YHold, Role::Camera);
        let preset = remap_preset("action_down").unwrap();
        let t = keep_holds(&preset, &user);
        assert_eq!(t[&Button::XHold], Role::Back, "the preset sets this hold");
        assert_eq!(t[&Button::YHold], Role::Camera, "the user's hold survives");
    }

    #[test]
    fn a_button_that_acts_held_has_no_hold_and_shortcut_roles_repeat() {
        let mut t = classic_roles();
        // B is item 1 in the classic table: an item acts held, so its long press goes.
        t.insert(Button::BHold, Role::Back);
        t.insert(Button::AHold, Role::Sword);
        let n = normalize(&t);
        assert_eq!(n[&Button::BHold], Role::None);
        assert_eq!(
            n[&Button::AHold],
            Role::Sword,
            "A is the action: it may be held"
        );
        // The roles a hold can do are the short list, not shield, target or the selector.
        assert!(!role_allowed(Role::Shield, Button::LHold));
        assert!(role_allowed(Role::Back, Button::RHold));
        // Jump on X and on R is a shortcut; two buttons for the target are not.
        t.insert(Button::R, Role::Jump);
        t.insert(Button::R3, Role::Target);
        let n = normalize(&t);
        assert_eq!((n[&Button::X], n[&Button::R]), (Role::Jump, Role::Jump));
        t.insert(Button::Zr, Role::Target);
        let n = normalize(&t);
        assert_eq!(
            (n[&Button::Zr], n[&Button::R3]),
            (Role::Target, Role::None),
            "the target stays on one button"
        );
    }

    #[test]
    fn the_wait_byte_has_a_bit_per_held_button() {
        // Only the arrow waits by default.
        assert_eq!(hold_wait_byte(&default_hold_wait()), 1);
        let mut w = default_hold_wait();
        w.insert(Button::R, true);
        assert_eq!(hold_wait_byte(&w), 1 | (1 << 6));
        // D-pad right is not a choice: its short press is the game's Navi call, so it always waits.
        w.insert(Button::Dr, false);
        assert_eq!(hold_wait_byte(&w), 1 | (1 << 6));
        // A missing button takes its default.
        assert_eq!(hold_wait_byte(&BTreeMap::new()), 1);
    }

    #[test]
    fn flags_follow_the_table() {
        assert_eq!(roles_flags(&classic_roles()), RoleFlags::default());
        let souls = remap_preset("souls").unwrap();
        assert!(roles_flags(&souls).sprint_l3);
        let triggers = remap_preset("triggers").unwrap();
        assert!(roles_flags(&triggers).l_selector);
    }
}
