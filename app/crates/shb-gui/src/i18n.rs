//! The window's texts: the Python app's table (`app-legacy/i18n.py`, generated into `i18n_table.rs`) plus the few keys
//! this port adds.
//!
//! `tr()` never fails: a key that is in neither table is returned as it is, so a missing translation shows up as a
//! visible key instead of an empty label.

use shb_core::settings::Lang;

include!("i18n_table.rs");

/// Keys this port has that the Python app does not (its own extra labels).
static EXTRA: &[(&str, [&str; 6])] = &[
    (
        "hold_tag",
        [
            "Hold",
            "Mantener",
            "Segurar",
            "Maintenir",
            "Halten",
            "Tieni",
        ],
    ),
    (
        "hold_intro",
        [
            "A row marked \"Hold\" belongs to the button above it: it is what that button does when you keep it pressed for about a third of a second.",
            "Una fila marcada \"Mantener\" es del botón de arriba: lo que hace ese botón cuando lo mantienes pulsado cerca de un tercio de segundo.",
            "Uma linha marcada \"Segurar\" pertence ao botão de cima: o que esse botão faz quando você o mantém pressionado por cerca de um terço de segundo.",
            "Une ligne marquée \"Maintenir\" appartient au bouton du dessus : ce que fait ce bouton quand on le maintient environ un tiers de seconde.",
            "Eine Zeile mit \"Halten\" gehört zur Taste darüber: Das macht die Taste, wenn du sie etwa eine Drittelsekunde gedrückt hältst.",
            "Una riga con \"Tieni\" appartiene al tasto sopra: cosa fa quel tasto se lo tieni premuto circa un terzo di secondo.",
        ],
    ),
    (
        "result_ok_title",
        [
            "Done",
            "Listo",
            "Concluído",
            "Terminé",
            "Fertig",
            "Fatto",
        ],
    ),
    (
        "result_fail_title",
        [
            "It failed",
            "Falló",
            "Falhou",
            "Échec",
            "Fehlgeschlagen",
            "Non riuscito",
        ],
    ),
    (
        "result_close",
        [
            "Close",
            "Cerrar",
            "Fechar",
            "Fermer",
            "Schließen",
            "Chiudi",
        ],
    ),
    (
        "path_ok",
        [
            "This folder is right: it opens {a}.",
            "La carpeta es válida: abre {a}.",
            "A pasta está certa: abre {a}.",
            "Le dossier est bon : il ouvre {a}.",
            "Der Ordner stimmt: er öffnet {a}.",
            "La cartella va bene: apre {a}.",
        ],
    ),
    (
        "e_path_emulator",
        [
            "No emulator in that folder: pick the one with its .exe.",
            "No hay ningún emulador en esa carpeta: elige la que tiene su .exe.",
            "Não há nenhum emulador nessa pasta: escolhe a que tem o .exe.",
            "Aucun émulateur dans ce dossier : choisis celui qui contient son .exe.",
            "Kein Emulator in diesem Ordner: wähle den mit seiner .exe.",
            "Nessun emulatore in quella cartella: scegli quella con il suo .exe.",
        ],
    ),
    (
        "e_path_game",
        [
            "Not a recompiled game: no ExeFS (`.longshot_exefs`, `.relink_exefs`…) or no launcher in it.",
            "No es un juego recompilado: falta su ExeFS (`.longshot_exefs`, `.relink_exefs`…) o su lanzador.",
            "Não é um jogo recompilado: falta o ExeFS (`.longshot_exefs`, `.relink_exefs`…) ou o lançador.",
            "Ce n'est pas un jeu recompilé : il manque l'ExeFS (`.longshot_exefs`, `.relink_exefs`…) ou le lanceur.",
            "Kein rekompiliertes Spiel: es fehlt die ExeFS (`.longshot_exefs`, `.relink_exefs`…) oder der Starter.",
            "Non è un gioco ricompilato: manca l'ExeFS (`.longshot_exefs`, `.relink_exefs`…) o l'avvio.",
        ],
    ),
    (
        "e_path_builder",
        [
            "Not a builder: the folder needs `tools/nx2/make_game.py` inside it.",
            "No es un builder: la carpeta debe tener `tools/nx2/make_game.py` dentro.",
            "Não é um builder: a pasta tem de ter `tools/nx2/make_game.py` dentro.",
            "Ce n'est pas un builder : le dossier doit contenir `tools/nx2/make_game.py`.",
            "Kein Builder: der Ordner braucht `tools/nx2/make_game.py` darin.",
            "Non è un builder: la cartella deve contenere `tools/nx2/make_game.py`.",
        ],
    ),
    (
        "rekarina_setup_missing",
        [
            "Rekarina's setup folder (with its own compiler) was not found beside the game.",
            "No se encontró la carpeta setup de Rekarina (con su compilador) al lado del juego.",
            "A pasta setup do Rekarina não foi encontrada ao lado do jogo.",
            "Le dossier setup de Rekarina (avec son compilateur) est introuvable à côté du jeu.",
            "Rekarinas setup-Ordner (mit seinem Compiler) wurde neben dem Spiel nicht gefunden.",
            "La cartella setup di Rekarina (con il suo compilatore) non è stata trovata accanto al gioco.",
        ],
    ),
    (
        "recomp_settings",
        [
            "Update settings",
            "Actualizar ajustes",
            "Atualizar ajustes",
            "Mettre à jour les réglages",
            "Einstellungen aktualisieren",
            "Aggiorna impostazioni",
        ],
    ),
    (
        "builder_label",
        [
            "Builder",
            "Builder",
            "Builder",
            "Compilateur",
            "Builder",
            "Builder",
        ],
    ),
    (
        "state",
        [
            "State:", "Estado:", "Estado:", "État :", "Status:", "Stato:",
        ],
    ),
    (
        "state_installed",
        [
            "mod installed",
            "mod instalado",
            "mod instalado",
            "mod installé",
            "Mod installiert",
            "mod installato",
        ],
    ),
    (
        "state_not_installed",
        [
            "mod not installed",
            "mod no instalado",
            "mod não instalado",
            "mod non installé",
            "Mod nicht installiert",
            "mod non installato",
        ],
    ),
    (
        "install_ok",
        [
            "Mod installed.",
            "Mod instalado.",
            "Mod instalado.",
            "Mod installé.",
            "Mod installiert.",
            "Mod installato.",
        ],
    ),
    (
        "install_failed",
        [
            "The action failed.",
            "La acción falló.",
            "A ação falhou.",
            "L'action a échoué.",
            "Die Aktion ist fehlgeschlagen.",
            "L'azione è fallita.",
        ],
    ),
    (
        "recomp_ok",
        [
            "Recompiled.",
            "Recompilado.",
            "Recompilado.",
            "Recompilé.",
            "Neu kompiliert.",
            "Ricompilato.",
        ],
    ),
    (
        "settings_ok",
        [
            "Settings updated (nothing was recompiled).",
            "Ajustes actualizados (no se recompiló nada).",
            "Ajustes atualizados (nada foi recompilado).",
            "Réglages mis à jour (rien n'a été recompilé).",
            "Einstellungen aktualisiert (nichts neu kompiliert).",
            "Impostazioni aggiornate (niente è stato ricompilato).",
        ],
    ),
    (
        "reinstall_ok",
        [
            "Uninstalled, installed and recompiled.",
            "Desinstalado, instalado y recompilado.",
            "Desinstalado, instalado e recompilado.",
            "Désinstallé, installé et recompilé.",
            "Deinstalliert, installiert und neu kompiliert.",
            "Disinstallato, installato e ricompilato.",
        ],
    ),
    (
        "starting",
        [
            "starting…",
            "empezando…",
            "começando…",
            "démarrage…",
            "startet…",
            "avvio…",
        ],
    ),
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
