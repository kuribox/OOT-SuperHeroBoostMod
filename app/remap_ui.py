"""Button remap editor: what each of the ten buttons does, with a live preview.

The module side is core/remap.h: every physical button gets a role (jump, sword, action, target, camera, sprint,
shield, item selector, item 1..3, the game's own item, or nothing). The old separate switches (manual shield,
target on R3, L3 sprint, L as the item selector) are now roles; the target and sprint options sit on the row of
the button that has that role. This page edits the table and draws two previews from it:
  * the controller, with the chosen button style's glyphs, and each button's role;
  * the in-game HUD cluster, the way core/hud.c draws it.

Its texts live here and are merged into the app's tables on import, so the page is self-contained.
"""
from __future__ import annotations

import tkinter as tk

import installer as I
import ui_kit as UI
from i18n import T

# ---------------------------------------------------------------- texts
_TXT = {
    "en": dict(
        rm_title="Controller buttons",
        rm_help="Choose what each button does; the HUD buttons follow. The shield turns on the manual shield (L "
                "guard while targeting), sprint runs without holding A. Only while you play: menus and the ocarina "
                "keep the real buttons.",
        rm_preset="Preset", rm_custom="Custom",
        preset_classic="Classic", preset_action_down="Action at the bottom",
        preset_shield="Manual shield", preset_souls="Souls (target on R3)",
        rm_items="Item buttons: {n} / 4",
        rm_missing="No button for: {a}",
        rm_no_shield="Target also raises the shield (no shield button).",
        rm_menu="Pause menu: {a}",
        rm_pad="Your controller", rm_hud="In game",
        rm_hud_help="Target, camera, sprint, shield, the selector and the sticks (L3 / R3) have no HUD button.",
        opt_switch="Tap: next enemy",
        opt_switch_help="While locked on, tap target to move to the enemy marked in red; hold it to let go.",
        opt_auto="Next one when it dies",
        opt_auto_help="When the locked enemy dies, press target again to lock on to the next one.",
        opt_toggle="Toggle",
        opt_toggle_help="One press starts sprinting, the next one stops (also when stamina runs out or with A).",
        role_none="—", role_jump="Jump", role_sword="Sword", role_action="Action", role_target="Target",
        role_camera="Center camera", role_sprint="Sprint", role_shield="Shield", role_selector="Item selector",
        role_item1="Item 1", role_item2="Item 2", role_item3="Item 3", role_item_main="Main item",
        role_action_help="Talk, open, check, roll.",
        role_item_main_help="The item you pick with A in the pause menu or with the item selector.",
    ),
    "es": dict(
        rm_title="Botones del mando",
        rm_help="Elige qué hace cada botón; los botones del HUD lo siguen. Escudo activa el escudo manual (guardia "
                "mientras fijas), sprint corre sin mantener A. Solo mientras juegas: los menús y la ocarina usan los "
                "botones reales.",
        rm_preset="Preset", rm_custom="Personalizado",
        preset_classic="Clásico", preset_action_down="Acción abajo",
        preset_shield="Escudo manual", preset_souls="Souls (fijar en R3)",
        rm_items="Botones de objeto: {n} / 4",
        rm_missing="Sin botón para: {a}",
        rm_no_shield="Fijar también levanta el escudo (no hay botón de escudo).",
        rm_menu="Menú de pausa: {a}",
        rm_pad="Tu mando", rm_hud="En el juego",
        rm_hud_help="Fijar, cámara, sprint, escudo, el selector y lo que esté en L3 / R3 no tienen botón en el HUD.",
        opt_switch="Toque: siguiente enemigo",
        opt_switch_help="Con un enemigo fijado, un toque pasa al enemigo marcado en rojo; mantenerlo suelta el fijado.",
        opt_auto="Siguiente al morir",
        opt_auto_help="Cuando muere el enemigo fijado, se fija el siguiente automáticamente.",
        opt_toggle="Alternar",
        opt_toggle_help="Un toque empieza a correr y el siguiente para (también al quedarte sin estamina o con A).",
        role_none="—", role_jump="Saltar", role_sword="Espada", role_action="Acción", role_target="Fijar",
        role_camera="Centrar cámara", role_sprint="Sprint", role_shield="Escudo", role_selector="Selector de objetos",
        role_item1="Objeto 1", role_item2="Objeto 2", role_item3="Objeto 3", role_item_main="Objeto principal",
        role_action_help="Hablar, abrir, revisar, rodar.",
        role_item_main_help="El objeto que eliges con A en el menú de pausa o con el selector de objetos.",
    ),
    "pt": dict(
        rm_title="Botões do controle",
        rm_help="Escolha o que cada botão faz; os botões do HUD acompanham. Escudo liga o escudo manual (defesa ao "
                "mirar), corrida corre sem segurar A. Só enquanto joga: menus e a ocarina usam os botões reais.",
        rm_preset="Predefinição", rm_custom="Personalizado",
        preset_classic="Clássico", preset_action_down="Ação embaixo",
        preset_shield="Escudo manual", preset_souls="Souls (mirar no R3)",
        rm_items="Botões de item: {n} / 4",
        rm_missing="Sem botão para: {a}",
        rm_no_shield="Mirar também levanta o escudo (sem botão de escudo).",
        rm_menu="Menu de pausa: {a}",
        rm_pad="Seu controle", rm_hud="No jogo",
        rm_hud_help="Mirar, câmera, corrida, escudo, o seletor e o que estiver em L3 / R3 não têm botão no HUD.",
        opt_switch="Toque: próximo inimigo",
        opt_switch_help="Com um inimigo na mira, um toque passa ao inimigo marcado em vermelho; segurar solta a mira.",
        opt_auto="Próximo ao morrer",
        opt_auto_help="Quando o inimigo na mira morre, mira o próximo automaticamente.",
        opt_toggle="Alternar",
        opt_toggle_help="Um toque começa a correr e o seguinte para (também quando a estamina acaba ou com A).",
        role_none="—", role_jump="Pular", role_sword="Espada", role_action="Ação", role_target="Mirar",
        role_camera="Centralizar câmera", role_sprint="Corrida", role_shield="Escudo", role_selector="Seletor de itens",
        role_item1="Item 1", role_item2="Item 2", role_item3="Item 3", role_item_main="Item principal",
        role_action_help="Falar, abrir, examinar, rolar.",
        role_item_main_help="O item escolhido com A no menu de pausa ou com o seletor de itens.",
    ),
    "fr": dict(
        rm_title="Boutons de la manette",
        rm_help="Choisissez l'action de chaque bouton ; les boutons du HUD suivent. Bouclier active le bouclier "
                "manuel (garde en ciblant), sprint court sans tenir A. Seulement en jeu : les menus et l'ocarina "
                "gardent les vrais boutons.",
        rm_preset="Préréglage", rm_custom="Personnalisé",
        preset_classic="Classique", preset_action_down="Action en bas",
        preset_shield="Bouclier manuel", preset_souls="Souls (cibler sur R3)",
        rm_items="Boutons d'objet : {n} / 4",
        rm_missing="Aucun bouton pour : {a}",
        rm_no_shield="Cibler lève aussi le bouclier (pas de bouton bouclier).",
        rm_menu="Menu pause : {a}",
        rm_pad="Votre manette", rm_hud="En jeu",
        rm_hud_help="Cibler, caméra, sprint, bouclier, le sélecteur et ce qui est sur L3 / R3 n'ont pas de bouton "
                    "dans le HUD.",
        opt_switch="Appui : ennemi suivant",
        opt_switch_help="En ciblant, un appui passe à l'ennemi marqué en rouge ; le maintenir relâche la cible.",
        opt_auto="Suivant à sa mort",
        opt_auto_help="Quand l'ennemi ciblé meurt, le suivant est ciblé automatiquement.",
        opt_toggle="Bascule",
        opt_toggle_help="Un appui lance le sprint, le suivant l'arrête (aussi sans endurance ou avec A).",
        role_none="—", role_jump="Sauter", role_sword="Épée", role_action="Action", role_target="Cibler",
        role_camera="Recentrer la caméra", role_sprint="Sprint", role_shield="Bouclier",
        role_selector="Sélecteur d'objets",
        role_item1="Objet 1", role_item2="Objet 2", role_item3="Objet 3", role_item_main="Objet principal",
        role_action_help="Parler, ouvrir, examiner, rouler.",
        role_item_main_help="L'objet choisi avec A dans le menu pause ou avec le sélecteur d'objets.",
    ),
    "de": dict(
        rm_title="Controller-Tasten",
        rm_help="Lege fest, was jede Taste tut; die HUD-Tasten folgen. Schild schaltet den manuellen Schild ein "
                "(Abwehr beim Anvisieren), Sprint rennt ohne A zu halten. Nur beim Spielen: Menüs und die Okarina "
                "nutzen die echten Tasten.",
        rm_preset="Vorlage", rm_custom="Eigene",
        preset_classic="Klassisch", preset_action_down="Aktion unten",
        preset_shield="Manueller Schild", preset_souls="Souls (Anvisieren auf R3)",
        rm_items="Item-Tasten: {n} / 4",
        rm_missing="Keine Taste für: {a}",
        rm_no_shield="Anvisieren hebt auch den Schild (keine Schild-Taste).",
        rm_menu="Pausemenü: {a}",
        rm_pad="Dein Controller", rm_hud="Im Spiel",
        rm_hud_help="Anvisieren, Kamera, Sprint, Schild, die Auswahl und alles auf L3 / R3 haben keine HUD-Taste.",
        opt_switch="Tippen: nächster Gegner",
        opt_switch_help="Beim Anvisieren wechselt ein Tippen zum rot markierten Gegner; Halten löst das Ziel.",
        opt_auto="Nächster, wenn er stirbt",
        opt_auto_help="Stirbt der anvisierte Gegner, wird der nächste automatisch anvisiert.",
        opt_toggle="Umschalten",
        opt_toggle_help="Ein Druck startet den Sprint, der nächste beendet ihn (auch ohne Ausdauer oder mit A).",
        role_none="—", role_jump="Springen", role_sword="Schwert", role_action="Aktion", role_target="Anvisieren",
        role_camera="Kamera zentrieren", role_sprint="Sprint", role_shield="Schild", role_selector="Item-Auswahl",
        role_item1="Item 1", role_item2="Item 2", role_item3="Item 3", role_item_main="Haupt-Item",
        role_action_help="Reden, öffnen, untersuchen, rollen.",
        role_item_main_help="Das Item, das du mit A im Pausemenü oder mit der Item-Auswahl wählst.",
    ),
    "it": dict(
        rm_title="Tasti del controller",
        rm_help="Scegli cosa fa ogni tasto; i tasti dell'HUD lo seguono. Scudo attiva lo scudo manuale (guardia "
                "mentre miri), sprint corre senza tenere A. Solo mentre giochi: menu e ocarina usano i tasti reali.",
        rm_preset="Preset", rm_custom="Personalizzato",
        preset_classic="Classico", preset_action_down="Azione in basso",
        preset_shield="Scudo manuale", preset_souls="Souls (mira su R3)",
        rm_items="Tasti oggetto: {n} / 4",
        rm_missing="Nessun tasto per: {a}",
        rm_no_shield="Mirare alza anche lo scudo (nessun tasto scudo).",
        rm_menu="Menu di pausa: {a}",
        rm_pad="Il tuo controller", rm_hud="In gioco",
        rm_hud_help="Mira, telecamera, sprint, scudo, il selettore e ciò che è su L3 / R3 non hanno un tasto "
                    "nell'HUD.",
        opt_switch="Tocco: nemico successivo",
        opt_switch_help="Mentre miri, un tocco passa al nemico segnato in rosso; tenerlo premuto rilascia la mira.",
        opt_auto="Successivo alla morte",
        opt_auto_help="Quando il nemico mirato muore, si mira il successivo automaticamente.",
        opt_toggle="Alterna",
        opt_toggle_help="Un tocco avvia lo sprint, il successivo lo ferma (anche senza stamina o con A).",
        role_none="—", role_jump="Salto", role_sword="Spada", role_action="Azione", role_target="Mira",
        role_camera="Centra telecamera", role_sprint="Sprint", role_shield="Scudo", role_selector="Selettore oggetti",
        role_item1="Oggetto 1", role_item2="Oggetto 2", role_item3="Oggetto 3", role_item_main="Oggetto principale",
        role_action_help="Parlare, aprire, esaminare, rotolare.",
        role_item_main_help="L'oggetto scelto con A nel menu di pausa o con il selettore oggetti.",
    ),
}
for _lang, _t in _TXT.items():
    if _lang in T:
        T[_lang].update(_t)


# manual shield as an option of the target row (it enables the Shield role)
_TXT2 = {
    'en': dict(rm_help='Choose what each button does; the HUD buttons follow. Sprint runs without holding A. Only while you play: menus and the ocarina keep the real buttons.',
             rm_no_shield='Manual shield is on but no button has Shield: you cannot guard.',
             opt_shield='Manual shield', opt_shield_help='Target no longer raises the shield: it goes up with its own Shield button.'),
    'es': dict(rm_help='Elige qué hace cada botón; los botones del HUD lo siguen. Sprint corre sin mantener A. Solo mientras juegas: los menús y la ocarina usan los botones reales.',
             rm_no_shield='El escudo manual está activo pero ningún botón tiene Escudo: no podrás cubrirte.',
             opt_shield='Escudo manual', opt_shield_help='Fijar ya no levanta el escudo: se levanta con su propio botón de Escudo.'),
    'pt': dict(rm_help='Escolha o que cada botão faz; os botões do HUD acompanham. Corrida corre sem segurar A. Só enquanto joga: menus e a ocarina usam os botões reais.',
             rm_no_shield='O escudo manual está ligado mas nenhum botão tem Escudo: não poderá se defender.',
             opt_shield='Escudo manual', opt_shield_help='Mirar não levanta mais o escudo: ele sobe com o próprio botão de Escudo.'),
    'fr': dict(rm_help="Choisissez l'action de chaque bouton ; les boutons du HUD suivent. Sprint court sans tenir A. Seulement en jeu : les menus et l'ocarina gardent les vrais boutons.",
             rm_no_shield="Le bouclier manuel est actif mais aucun bouton n'a Bouclier : vous ne pourrez pas parer.",
             opt_shield='Bouclier manuel', opt_shield_help='Cibler ne lève plus le bouclier : il se lève avec son propre bouton Bouclier.'),
    'de': dict(rm_help='Lege fest, was jede Taste tut; die HUD-Tasten folgen. Sprint rennt ohne A zu halten. Nur beim Spielen: Menüs und die Okarina nutzen die echten Tasten.',
             rm_no_shield='Der manuelle Schild ist an, aber keine Taste hat Schild: du kannst nicht abwehren.',
             opt_shield='Manueller Schild', opt_shield_help='Anvisieren hebt den Schild nicht mehr: er kommt mit seiner eigenen Schild-Taste.'),
    'it': dict(rm_help="Scegli cosa fa ogni tasto; i tasti dell'HUD lo seguono. Sprint corre senza tenere A. Solo mentre giochi: menu e ocarina usano i tasti reali.",
             rm_no_shield='Lo scudo manuale è attivo ma nessun tasto ha Scudo: non potrai pararti.',
             opt_shield='Scudo manuale', opt_shield_help='Mirare non alza più lo scudo: si alza con il suo tasto Scudo.'),
}
for _lang, _t in _TXT2.items():
    if _lang in T:
        T[_lang].update(_t)


# the shield has a HUD button now (parts 12 / 13)
for _lang, _t in {'en': 'Target, camera, sprint, the selector and the sticks (L3 / R3) have no HUD button.', 'es': 'Fijar, cámara, sprint, el selector y los sticks (L3 / R3) no tienen botón en el HUD.', 'pt': 'Mirar, câmera, corrida, o seletor e os sticks (L3 / R3) não têm botão no HUD.', 'fr': "Cibler, caméra, sprint, le sélecteur et les sticks (L3 / R3) n'ont pas de bouton dans le HUD.", 'de': 'Anvisieren, Kamera, Sprint, die Auswahl und die Sticks (L3 / R3) haben keine HUD-Taste.', 'it': "Mira, telecamera, sprint, il selettore e gli stick (L3 / R3) non hanno un tasto nell'HUD."}.items():
    if _lang in T:
        T[_lang]['rm_hud_help'] = _t


# the armory role (features/quick_select): change sword and shield while held
for _lang, (_n, _h) in {'en': ('Armory', 'Hold it to open the item drawer with the swords and shields you own and can use at your age: swords above, shields below; the stick picks, let go to equip.'), 'es': ('Armería', 'Mantenlo para abrir el cajón de objetos con las espadas y escudos que tienes y puedes usar a tu edad: espadas arriba, escudos abajo; el stick elige, suelta para equipar.'), 'pt': ('Arsenal', 'Segure para abrir a gaveta de itens com as espadas e escudos que você tem e pode usar na sua idade: espadas em cima, escudos embaixo; o analógico escolhe, solte para equipar.'), 'fr': ('Armurerie', "Maintenez-le pour ouvrir le tiroir d'objets avec les épées et boucliers que vous avez et pouvez utiliser à votre âge : épées en haut, boucliers en bas ; le stick choisit, relâchez pour équiper."), 'de': ('Waffenkammer', 'Gedrückt halten öffnet die Item-Leiste mit den Schwertern und Schilden, die du hast und in deinem Alter nutzen kannst: Schwerter oben, Schilde unten; der Stick wählt, loslassen rüstet aus.'), 'it': ('Armeria', 'Tienilo premuto per aprire il cassetto degli oggetti con le spade e gli scudi che hai e puoi usare alla tua età: spade sopra, scudi sotto; lo stick sceglie, rilascia per equipaggiare.')}.items():
    if _lang in T:
        T[_lang]['role_armory'] = _n
        T[_lang]['role_armory_help'] = _h

# D-pad right: its short press is the game's Navi call, the role is the long press
for _lang, _t in {
    'en': 'D-pad right: a short press still calls Navi; this is its long press.',
    'es': 'Cruceta derecha: pulsación corta sigue llamando a Navi; esto es la pulsación larga.',
    'pt': 'Direcional direito: um toque curto continua chamando a Navi; isto é o toque longo.',
    'fr': "Croix droite : un appui court appelle toujours Navi ; ceci est l'appui long.",
    'de': 'Steuerkreuz rechts: kurzer Druck ruft weiter Navi; das hier ist der lange Druck.',
    'it': 'Croce destra: la pressione breve chiama ancora Navi; questa è la pressione lunga.',
}.items():
    if _lang in T:
        T[_lang]['dr_note'] = _t

# ---------------------------------------------------------------- model
# The order the editor lists the buttons in (face buttons, shoulders, sticks). The module's order is I.REMAP_BUTTONS.
ROW_ORDER = ("X", "Y", "A", "B", "L", "R", "ZL", "ZR", "L3", "R3", "DR")
FACE = ("A", "B", "X", "Y")
STICKS = ("L3", "R3")
# glyph drawn for each game button per button style (tools/make_buttons.py GLYPHS)
GLYPHS = {
    "switch": {b: ("▶" if b == "DR" else b) for b in I.REMAP_BUTTONS},
    "xbox": {"A": "B", "B": "A", "X": "Y", "Y": "X", "L": "LB", "R": "RB", "ZL": "LT", "ZR": "RT",
             "L3": "LS", "R3": "RS", "DR": "▶"},
    "playstation": {"A": "circle", "B": "cross", "X": "triangle", "Y": "square",
                    "L": "L1", "R": "R1", "ZL": "L2", "ZR": "R2", "L3": "L3", "R3": "R3", "DR": "▶"},
}
PS_COLORS = {"circle": "#e0605a", "cross": "#7aa6e8", "triangle": "#5fc7a0", "square": "#d98cc8"}
PS_TEXT = {"circle": "○", "cross": "✕", "triangle": "△", "square": "□"}
ITEM_MARK = {"item1": "1", "item2": "2", "item3": "3", "item_main": "★"}
# core/hud.c: where each button sits (layout units, y up); ZL mirrors ZR. Items sit up-left of their button.
HUD_POS = {"L": (1020.4, 872.3), "X": (1292.6, 843.0), "R": (1545.2, 872.3), "Y": (1129.6, 680.0),
           "A": (1455.6, 680.0), "ZR": (1677.2, 680.0), "B": (1292.6, 517.0), "ZL": (908.0, 680.0)}
# Switch GameCube face-button centers from core/hud.c g_sb_gamecube.
HUD_GAMECUBE = {**HUD_POS, "A": (1292.6, 680.0), "B": (1144.6, 612.0),
                "X": (1434.6, 710.0), "Y": (1254.6, 813.0)}
HUD_SIMPLE = {"item2": (1120.4, 872.3), "item3": (1445.2, 872.3), "item_main": (1677.2, 680.0),
              "item1": (1292.6, 680.0)}
ITEM_OFF = (-25.3, 25.3)
MOVE_ROLES = ("jump", "sword", "action", "target", "camera", "sprint", "shield", "armory")
ROLE_HELP = {"action": "role_action_help", "item_main": "role_item_main_help", "armory": "role_armory_help"}


def role_vars(saved, master=None) -> dict:
    roles = I.normalize_roles(saved if isinstance(saved, dict) else I.CLASSIC_ROLES)
    return {b: tk.StringVar(master=master, value=roles[b]) for b in I.REMAP_BUTTONS}


def roles_value(rvars: dict) -> dict:
    return I.normalize_roles({b: v.get() for b, v in rvars.items()})


def set_roles(rvars: dict, roles: dict) -> None:
    roles = I.normalize_roles(roles)
    for b, var in rvars.items():
        if var.get() != roles[b]:
            var.set(roles[b])


def preset_of(roles: dict) -> str:
    """The preset a table is (the D-pad right long press is no part of a preset: it keeps its own role)."""
    roles = dict(I.normalize_roles(roles), DR="none")
    for k in I.REMAP_PRESET_KEYS:
        if dict(I.normalize_roles(I.REMAP_PRESETS[k]), DR="none") == roles:
            return k
    return "custom"


def menu_buttons(roles: dict) -> dict:
    """core/remap.c remap_menu_buttons(): the pause-menu button that assigns each of items 1..3."""
    phys = {r: b for b, r in roles.items() if r != "none"}
    out, taken = {}, set()
    for r in ("item1", "item2", "item3"):
        b = phys.get(r)
        if b in ("X", "Y", "L", "R") and b not in taken:
            out[r] = b
            taken.add(b)
    classic = {"item1": "X", "item2": "L", "item3": "R"}
    for r in ("item1", "item2", "item3"):
        if r in out or r not in phys:
            continue
        pick = next((c for c in (classic[r], "X", "L", "R", "Y") if c not in taken), classic[r])
        out[r] = pick
        taken.add(pick)
    return out


# ---------------------------------------------------------------- widgets
def help_icon(parent, text_fn, colors, fam, bg=None):
    """The quiet "?" that opens the full explanation (the visual kit's help window)."""
    return UI.HelpIcon(parent, text_fn)


# ---------------------------------------------------------------- editor
class RemapEditor:
    """One row per button (its glyph, a role list and that role's options) and the two previews."""

    def __init__(self, parent, L, fam, colors, v):
        """v: the app's variables: bmap, roles (dict), bstyle, bsimple, switch_target, auto_target, sprint_toggle."""
        self.L, self.fam, self.c, self.v = L, fam, colors, v
        self.combos: dict[str, UI.Select] = {}
        self.chips: dict[str, tk.Canvas] = {}
        self.opts: dict[str, UI.Frame] = {}
        self.choices: dict[str, list] = {}
        self.rolevars: dict[str, tk.StringVar] = {}
        self._traces = []
        bg = colors["PANEL"]
        self.frame = UI.Frame(parent, bg=bg)
        self.frame.pack(fill="x")

        main = UI.Frame(self.frame, bg=bg)
        main.pack(fill="x", pady=(4, 0))
        left = UI.Frame(main, bg=bg)
        left.grid(row=0, column=0, sticky="nw")
        right = UI.Frame(main, bg=bg)
        right.grid(row=0, column=1, sticky="nw", padx=(20, 0))

        self.count = UI.Label(left, text="", bg=bg, fg=colors["DIM"], font=(fam, 9))
        self.count.pack(anchor="w", pady=(0, 4))

        for b in ROW_ORDER:
            box = UI.Frame(left, bg=bg)
            box.pack(anchor="w", fill="x", pady=(1, 3))
            row = UI.Frame(box, bg=bg)
            row.pack(anchor="w", fill="x")
            chip = tk.Canvas(row, width=40, height=28, bg=bg, highlightthickness=0)
            chip.pack(side="left")
            self.chips[b] = chip
            self.choices[b] = [r for r in I.ROLES if I.role_allowed(r, b)]
            self.rolevars[b] = tk.StringVar(value=L("role_none"))
            cb = UI.Select(row, [L("role_none")], self.rolevars[b], width=19)
            cb.pack(side="left", padx=(6, 0))
            cb.bind("<<ComboboxSelected>>", lambda e, btn=b: self._pick(btn))
            self.combos[b] = cb
            help_slot = UI.Frame(row, bg=bg, width=24)   # the role's own "?" beside its list
            help_slot.pack(side="left", padx=(4, 0))
            opt = UI.Frame(box, bg=bg, width=1)   # the role's options, on the line below
            opt.pack(anchor="w", padx=(56, 0), pady=(2, 0))
            self.opts[b] = (help_slot, opt)

        UI.Label(right, text=L("rm_pad"), bg=bg, fg=colors["DIM"], font=(fam, 9, "bold")).pack(anchor="w")
        self.pad = tk.Canvas(right, width=400, height=228, bg=colors["PANEL2"], highlightthickness=1,
                             highlightbackground=colors["BORDER"])
        self.pad.pack(anchor="w")
        hrow = UI.Frame(right, bg=bg)
        hrow.pack(anchor="w", pady=(8, 0))
        UI.Label(hrow, text=L("rm_hud"), bg=bg, fg=colors["DIM"], font=(fam, 9, "bold")).pack(side="left")
        help_icon(hrow, L("rm_hud_help"), colors, fam).pack(side="left", padx=(4, 0))
        self.hud = tk.Canvas(right, width=400, height=228, bg="#101010", highlightthickness=1,
                             highlightbackground=colors["BORDER"])
        self.hud.pack(anchor="w")

        self.notes = UI.Label(self.frame, text="", bg=bg, fg=colors["AMBER"], font=(fam, 9), justify="left",
                              anchor="w", wraplength=980)
        self.notes.pack(anchor="w", pady=(8, 0))
        self.info = UI.Label(self.frame, text="", bg=bg, fg=colors["DIM"], font=(fam, 9), justify="left",
                             anchor="w", wraplength=980)
        self.info.pack(anchor="w", pady=(2, 0))

        for key in ("bmap", "bstyle", "bplace", "bsimple"):
            self._traces.append((v[key], v[key].trace_add("write", lambda *_: self.redraw())))
        self._traces.append((v["mshield"], v["mshield"].trace_add("write", lambda *_: self._shield_changed())))
        for var in v["roles"].values():
            self._traces.append((var, var.trace_add("write", lambda *_: self.redraw())))
        self.frame.bind("<Destroy>", self._drop_traces)
        self._shown_opts: dict[str, str] = {}
        self.redraw()

    # ---- helpers ----
    def _drop_traces(self, _e=None):
        for var, tid in self._traces:
            try:
                var.trace_remove("write", tid)
            except (tk.TclError, ValueError):
                pass
        self._traces = []

    def role_name(self, role: str) -> str:
        return self.L("role_" + role)

    def _style(self) -> str:
        s = self.v["bstyle"].get()
        return s if s in GLYPHS else "switch"

    def _roles(self) -> dict:
        roles = roles_value(self.v["roles"])
        if not self.v["bmap"].get():   # no item buttons: the slots and the selector do nothing
            roles = {b: ("none" if r in ("item1", "item2", "item3", "selector") else r) for b, r in roles.items()}
        return roles

    # ---- editing ----
    def _pick(self, btn: str):
        new = self.choices[btn][self.combos[btn].current()]
        rv = self.v["roles"]
        old = rv[btn].get()
        if new == old:
            return
        table = {b: var.get() for b, var in rv.items()}
        if new != "none":
            # each role on one button: the button that had it takes this one's old role (if it may), else nothing.
            for other, r in table.items():
                if other == btn:
                    continue
                if r == new:
                    table[other] = old if I.role_allowed(old, other) else "none"
        table[btn] = new
        set_roles(rv, table)

    def _shield_changed(self):
        """Manual shield (an option of the target row): off, no button keeps the Shield role."""
        if not self.v["mshield"].get():
            rv = self.v["roles"]
            set_roles(rv, {b: ("none" if var.get() == "shield" else var.get()) for b, var in rv.items()})
        self.redraw()

    def _options_for(self, btn: str, role: str):
        """The options of the role on this row: target (switch, next one) and sprint (toggle)."""
        if self._shown_opts.get(btn) == role:
            return
        self._shown_opts[btn] = role
        help_slot, f = self.opts[btn]
        for w in list(f.winfo_children()) + list(help_slot.winfo_children()):
            w.destroy()
        items = []
        if role == "target":
            items = [("opt_shield", self.v["mshield"]), ("opt_switch", self.v["switch_target"]),
                     ("opt_auto", self.v["auto_target"])]
        elif role == "sprint":
            items = [("opt_toggle", self.v["sprint_toggle"])]
        bg = self.c["PANEL"]
        for key, var in items:
            line = UI.Frame(f, bg=bg)
            line.pack(anchor="w", pady=1)
            UI.Check(line, text=self.L(key), variable=var, bg=bg, fg=self.c["TEXT"],
                     selectcolor=self.c["PANEL2"], font=(self.fam, 9)).pack(side="left")
            help_icon(line, self.L(key + "_help"), self.c, self.fam).pack(side="left", padx=(4, 0))
        if role in ROLE_HELP:
            help_icon(help_slot, self.L(ROLE_HELP[role]), self.c, self.fam).pack(side="left")
        if btn == "DR":   # the arrow is Navi's call: only the long press is the role
            line = UI.Frame(f, bg=bg)
            line.pack(anchor="w", pady=1)
            UI.Label(line, text=self.L("dr_note"), bg=bg, fg=self.c["DIM"], font=(self.fam, 9), justify="left",
                     anchor="w", wraplength=330).pack(side="left")
            items = items or [None]
        if items:   # options appear on their own line under the select
            if not f.winfo_manager():
                f.pack(anchor="w", padx=(56, 0), pady=(2, 0))
        else:
            f.pack_forget()

    # ---- drawing ----
    def redraw(self):
        try:
            if not self.frame.winfo_exists():
                return
        except tk.TclError:
            return
        roles = self._roles()
        on = self.v["bmap"].get()
        for b, cb in self.combos.items():
            ch = [r for r in self.choices[b] if (on or r not in ("item1", "item2", "item3", "selector"))
                  and (r != "shield" or self.v["mshield"].get() or roles[b] == "shield")]
            cb.config(values=[self.role_name(r) for r in ch])
            self.choices[b] = ch + [r for r in self.choices[b] if r not in ch]   # (hidden ones stay pickable by index)
            cb.current(ch.index(roles[b]) if roles[b] in ch else 0)
            self._chip(self.chips[b], 20, 14, b, 12)
            self._options_for(b, roles[b])
        n_items = sum(1 for r in roles.values() if r in I.ITEM_ROLES)
        self.count.config(text=self.L("rm_items", n=n_items))
        notes = []
        missing = [self.role_name(r) for r in I.roles_missing(roles)]
        if missing:
            notes.append(self.L("rm_missing", a=", ".join(missing)))
        if self.v["mshield"].get() and "shield" not in roles.values():
            notes.append(self.L("rm_no_shield"))
        self.notes.config(text="\n".join(notes), fg=self.c["AMBER"])
        mb = menu_buttons(roles)
        glyph = GLYPHS[self._style()]
        pairs = [f"{self._gt(glyph[mb[r]])} → {self.role_name(r)}" for r in ("item1", "item2", "item3") if r in mb]
        if "item_main" in roles.values():
            pairs.append(f"{self._gt(glyph['A'])} → {self.role_name('item_main')}")
        self.info.config(text=self.L("rm_menu", a="   ".join(pairs)) if pairs else "")
        self._draw_pad(roles)
        self._draw_hud(roles)

    @staticmethod
    def _gt(g: str) -> str:
        return PS_TEXT.get(g, g)

    def _chip(self, cv: tk.Canvas, x: float, y: float, btn: str, r: float, clear=True):
        """One button in the chosen style: a round face button or stick, or a shoulder tab, with its glyph."""
        if clear:
            cv.delete("all")
        g = GLYPHS[self._style()][btn]
        if btn in FACE:
            cv.create_oval(x - r, y - r, x + r, y + r, fill="#101012", outline="#5b5b60", width=1)
        elif btn == "DR":   # the D-pad's right arrow: a small rounded key
            cv.create_rectangle(x - r * 0.85, y - r * 0.85, x + r * 0.85, y + r * 0.85, fill="#101012",
                                outline="#5b5b60", width=1)
        elif btn in STICKS:
            cv.create_oval(x - r * 1.1, y - r * 1.1, x + r * 1.1, y + r * 1.1, fill="#0f1813", outline="#7a8a80",
                           width=2)
        else:
            w = r * 1.45
            cv.create_rectangle(x - w, y - r * 0.8, x + w, y + r * 0.8, fill="#101012", outline="#5b5b60", width=1)
        col = "#c8c8cc"
        if g in PS_COLORS:
            col, s = PS_COLORS[g], r * 0.45
            if g == "circle":
                cv.create_oval(x - s, y - s, x + s, y + s, outline=col, width=2)
            elif g == "cross":
                cv.create_line(x - s, y - s, x + s, y + s, fill=col, width=2)
                cv.create_line(x - s, y + s, x + s, y - s, fill=col, width=2)
            elif g == "triangle":
                cv.create_polygon(x, y - s, x - s, y + s * 0.8, x + s, y + s * 0.8, outline=col, fill="", width=2)
            else:
                cv.create_rectangle(x - s, y - s, x + s, y + s, outline=col, width=2)
        else:
            size = max(7, int(r * (0.85 if len(g) == 1 else 0.62)))
            cv.create_text(x, y, text=g, fill=col, font=(self.fam, size, "bold"))

    def _role_color(self, role: str) -> str:
        if role in I.ITEM_ROLES or role == "selector":
            return self.c["ACCENT"]
        return "#7fc8a9" if role in MOVE_ROLES else self.c["DIM"]

    def _draw_pad(self, roles: dict):
        cv = self.pad
        cv.delete("all")
        cv.create_polygon(66, 64, 334, 64, 376, 130, 376, 196, 334, 218, 296, 186, 104, 186, 66, 218, 24, 196,
                          24, 130, fill="#1b2a22", outline=self.c["BORDER"], smooth=True, width=2)
        cx, cy, d = 296, 112, 25
        spot = {"Y": (cx - d, cy), "A": (cx + d, cy), "X": (cx, cy - d), "B": (cx, cy + d),
                "ZL": (56, 18), "L": (56, 44), "R": (344, 44), "ZR": (344, 18), "L3": (92, 110), "R3": (236, 160), "DR": (56, 168)}
        label_at = {"X": (cx, cy - d - 14, "s"), "B": (cx, cy + d + 14, "n"), "Y": (cx - d - 15, cy, "e"),
                    "A": (cx + d + 15, cy, "w"), "ZL": (80, 18, "w"), "L": (80, 44, "w"),
                    "R": (320, 44, "e"), "ZR": (320, 18, "e"), "L3": (92, 136, "n"), "R3": (210, 160, "e"),
                    "DR": (76, 168, "w")}
        for btn in I.REMAP_BUTTONS:
            x, y = spot[btn]
            self._chip(cv, x, y, btn, 12, clear=False)
            role = roles[btn]
            lx, ly, anchor = label_at[btn]
            cv.create_text(lx, ly, text=self.role_name(role), fill=self._role_color(role), anchor=anchor,
                           font=(self.fam, 9, "bold"))

    def _draw_hud(self, roles: dict):
        """core/hud.c sb_xy() / slots_draw(): the HUD cluster, centred in the preview."""
        cv = self.hud
        cv.delete("all")
        simple = self.v["bsimple"].get()
        positions = HUD_GAMECUBE if self.v["bplace"].get() == "gamecube" else HUD_POS
        phys = {r: b for b, r in roles.items() if r != "none"}
        sc = 0.37
        ox, oy = 200 - 1292.6 * sc, 118 + 712.0 * sc

        def to_px(x, y):
            return ox + x * sc, oy - y * sc

        hud_roles = ["item2", "jump", "item3", "sword", "action", "item_main", "item1"]
        if self.v["mshield"].get():
            hud_roles.append("shield")   # core/hud.c parts 12 / 13: the shield button and the equipped shield
        for role in hud_roles:
            btn = phys.get(role)
            if btn is None or btn in STICKS or btn == "DR":
                continue                                    # no HUD art for the sticks
            if simple and role in ("jump", "sword", "action", "shield"):
                continue
            bx, by = HUD_SIMPLE[role] if simple else positions[btn]
            x, y = to_px(bx, by)
            r = 20
            if role == "jump":   # the unlettered button with the "Jump" label
                cv.create_oval(x - r, y - r, x + r, y + r, fill="#101012", outline="#5b5b60")
                cv.create_text(x, y, text=self.role_name("jump"), fill="#e8e8e8", font=(self.fam, 8, "bold"))
                continue
            self._chip(cv, x, y, btn, r, clear=False)
            ix, iy = to_px(bx + ITEM_OFF[0] * 1.9, by + ITEM_OFF[1] * 1.9)
            if role == "sword":
                cv.create_text(ix, iy, text="⚔", fill="#dfe6ee", font=(self.fam, 13, "bold"))
            elif role == "shield":   # a small heater shield
                w, h = 9, 11
                cv.create_polygon(ix - w, iy - h, ix + w, iy - h, ix + w, iy + 1, ix, iy + h, ix - w, iy + 1,
                                  fill="#3c5fa0", outline="#c9d6ee", width=2)
            elif role in ITEM_MARK:
                s = 10
                cv.create_rectangle(ix - s, iy - s, ix + s, iy + s, fill="#3a2f12", outline=self.c["ACCENT"], width=2)
                cv.create_text(ix, iy, text=ITEM_MARK[role], fill=self.c["ACCENT"], font=(self.fam, 11, "bold"))
