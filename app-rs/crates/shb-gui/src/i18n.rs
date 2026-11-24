//! The window's texts: the Python app's table (`app/i18n.py`, generated into `i18n_table.rs`) plus the few keys
//! this port adds.
//!
//! `tr()` never fails: a key that is in neither table is returned as it is, so a missing translation shows up as a
//! visible key instead of an empty label.

use shb_core::settings::Lang;

include!("i18n_table.rs");

/// Keys this port has that the Python app does not (its own extra labels).
static EXTRA: &[(&str, [&str; 6])] = &[
    (
        "pick_folder",
        [
            "Choose a folder",
            "Elige una carpeta",
            "Escolha uma pasta",
            "Choisir un dossier",
            "Ordner wählen",
            "Scegli una cartella",
        ],
    ),
    (
        "rust_note",
        [
            "This window is the Rust version of the configurator: it installs the same module and writes the same bytes.",
            "Esta ventana es la versión en Rust del configurador: instala el mismo módulo y escribe los mismos bytes.",
            "Esta janela é a versão em Rust do configurador: instala o mesmo módulo e escreve os mesmos bytes.",
            "Cette fenêtre est la version Rust du configurateur : elle installe le même module et écrit les mêmes octets.",
            "Dieses Fenster ist die Rust-Version des Konfigurators: es installiert dasselbe Modul und schreibt dieselben Bytes.",
            "Questa finestra è la versione Rust del configuratore: installa lo stesso modulo e scrive gli stessi byte.",
        ],
    ),
    (
        "status",
        ["Status", "Estado", "Estado", "État", "Status", "Stato"],
    ),
    (
        "installed",
        [
            "installed",
            "instalado",
            "instalado",
            "installé",
            "installiert",
            "installato",
        ],
    ),
    (
        "save_settings",
        [
            "Save settings",
            "Guardar ajustes",
            "Guardar definições",
            "Enregistrer les réglages",
            "Einstellungen speichern",
            "Salva impostazioni",
        ],
    ),
    (
        "enemy_alert",
        [
            "Alert radius",
            "Radio de alerta",
            "Raio de alerta",
            "Rayon d'alerte",
            "Alarmradius",
            "Raggio di allarme",
        ],
    ),
    (
        "enemy_aggro",
        [
            "Aggression",
            "Agresividad",
            "Agressividade",
            "Agressivité",
            "Aggressivität",
            "Aggressività",
        ],
    ),
    (
        "aggro_normal",
        ["Normal", "Normal", "Normal", "Normale", "Normal", "Normale"],
    ),
    (
        "aggro_aggressive",
        [
            "Aggressive",
            "Agresivos",
            "Agressivos",
            "Agressifs",
            "Aggressiv",
            "Aggressivi",
        ],
    ),
    (
        "aggro_max",
        [
            "Maximum", "Máxima", "Máxima", "Maximale", "Maximal", "Massima",
        ],
    ),
];

fn index(lang: Lang) -> usize {
    match lang {
        Lang::En => 0,
        Lang::Es => 1,
        Lang::Pt => 2,
        Lang::Fr => 3,
        Lang::De => 4,
        Lang::It => 5,
    }
}

/// The text of `key` in `lang`, or the key itself when there is no such text.
pub fn tr(lang: Lang, key: &'static str) -> &'static str {
    let i = index(lang);
    if let Ok(found) = TEXTS.binary_search_by(|(k, _)| k.cmp(&key)) {
        return TEXTS[found].1[i];
    }
    if let Some((_, texts)) = EXTRA.iter().find(|(k, _)| *k == key) {
        return texts[i];
    }
    key
}

/// The name of a language as the app's own list spells it (`Español`, `Deutsch`, …).
pub fn lang_name(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "English",
        Lang::Es => "Español",
        Lang::Pt => "Português",
        Lang::Fr => "Français",
        Lang::De => "Deutsch",
        Lang::It => "Italiano",
    }
}

/// Replace the `{a}`-style placeholders of a text (the Python app uses `str.format`).
pub fn fill(text: &str, args: &[(&str, &str)]) -> String {
    let mut out = text.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_the_python_keys() {
        // The generator sorts the table: the binary search depends on it.
        assert!(
            TEXTS.windows(2).all(|w| w[0].0 < w[1].0),
            "TEXTS is not sorted"
        );
        assert!(TEXTS.len() > 300);
        assert_eq!(tr(Lang::Es, "install"), "Instalar");
        assert_eq!(tr(Lang::En, "install"), "Install");
        assert_eq!(tr(Lang::De, "install"), "Installieren");
    }

    #[test]
    fn placeholders_are_filled() {
        let text = fill(tr(Lang::En, "profile_current"), &[("a", "Recommended")]);
        assert_eq!(text, "Active profile: Recommended");
    }

    #[test]
    fn an_unknown_key_shows_itself() {
        assert_eq!(tr(Lang::Es, "no_such_key"), "no_such_key");
    }
}
