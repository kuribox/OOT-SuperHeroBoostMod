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

# D-pad right: its short press is the game's Navi call and cannot be changed; the role is the long press
for _lang, _t in {
    'en': "D-pad right is the game's Navi call: a short tap always calls her, and that cannot be changed. This row is "
          "the long press: hold the arrow about a third of a second and its role happens instead. The tap waits that "
          "long so the two can be told apart.",
    'es': 'La cruceta derecha es la llamada a Navi del juego: un toque corto siempre la llama y eso no se puede '
          'cambiar. Esta fila es la pulsación larga: mantén la flecha un tercio de segundo y se hace su acción. El '
          'toque espera ese tiempo para poder distinguirse.',
    'pt': 'O direcional direito é a chamada da Navi do jogo: um toque curto sempre a chama e isso não pode ser '
          'mudado. Esta linha é o toque longo: segure a seta por cerca de um terço de segundo e a ação dela acontece. '
          'O toque espera esse tempo para se distinguir.',
    'fr': "La croix droite est l'appel de Navi du jeu : un appui court l'appelle toujours, et cela ne se change pas. "
          "Cette ligne est l'appui long : maintenez la flèche environ un tiers de seconde et son action se fait. "
          "L'appui court attend ce délai pour qu'on puisse les distinguer.",
    'de': 'Steuerkreuz rechts ist der Navi-Ruf des Spiels: ein kurzer Druck ruft sie immer, und das lässt sich nicht '
          'ändern. Diese Zeile ist der lange Druck: halte den Pfeil etwa eine Drittelsekunde, dann passiert seine '
          'Aktion. Der kurze Druck wartet so lange, damit beide zu unterscheiden sind.',
    'it': 'La croce destra è la chiamata di Navi del gioco: un tocco breve la chiama sempre e questo non si può '
          'cambiare. Questa riga è la pressione lunga: tieni la freccia circa un terzo di secondo e succede la sua '
          'azione. Il tocco aspetta quel tempo per poterle distinguere.',
}.items():
    if _lang in T:
        T[_lang]['dr_note'] = _t

# the author's own table: items on the triggers, the item selector on B
for _lang, (_n, _d) in {
    'en': ('Triggers', 'Items on the triggers (R, ZL, ZR), the item selector on B, the shield on L, target on R3. Hold X: Back (the B of the game).'),
    'es': ('Gatillos', 'Objetos en los gatillos (R, ZL, ZR), selector de objetos en B, escudo en L, fijar en R3. Mantener X: la B del juego (Atrás).'),
    'pt': ('Gatilhos', 'Itens nos gatilhos (R, ZL, ZR), seletor de itens no B, escudo no L, mirar no R3. Segurar X: o B do jogo (Voltar).'),
    'fr': ('Gâchettes', "Objets sur les gâchettes (R, ZL, ZR), sélecteur d'objets sur B, bouclier sur L, cibler sur R3. Maintenir X : le B du jeu (Retour)."),
    'de': ('Trigger', 'Items auf den Triggern (R, ZL, ZR), Item-Auswahl auf B, Schild auf L, Anvisieren auf R3. X halten: das B des Spiels (Zurück).'),
    'it': ('Grilletti', 'Oggetti sui grilletti (R, ZL, ZR), selettore oggetti su B, scudo su L, mira su R3. Tieni X: il B del gioco (Indietro).'),
}.items():
    if _lang in T:
        T[_lang]['preset_triggers'] = _n
        T[_lang]['ctrl_preset_triggers_d'] = _d

# the back role and the held face buttons
for _lang, (_n, _h, _hh) in {
    'en': ('Back (B action)', "The game's own B: let go of a ledge, cancel, get off ... It is not an item use, so it works on any button even when item 1 is somewhere else or the B slot has an item. It acts while Link is busy with something else (hanging, climbing, swimming ...) or when the B slot is empty.",
           "Hold the button for about a third of a second to do this as well. The short press keeps its role above and acts at once; when the hold fires, the short role lets go (so a held button cannot charge the sword or sprint with A)."),
    'es': ('Atrás (acción de B)', 'La B propia del juego: soltarse de un borde, cancelar, bajarse ... No usa objeto, así que va en cualquier botón aunque el objeto 1 esté en otro o la ranura B tenga un objeto. Actúa mientras Link está ocupado en otra cosa (colgado, trepando, nadando ...) o con la ranura B vacía.',
           'Mantén el botón un tercio de segundo para hacer esto también. La pulsación corta conserva su acción de arriba y actúa al instante; cuando salta la larga, la corta se suelta (así un botón mantenido no puede cargar la espada ni correr con A).'),
    'pt': ('Voltar (ação do B)', 'O B do próprio jogo: soltar-se de uma borda, cancelar, descer ... Não usa item, então vale em qualquer botão mesmo com o item 1 em outro ou o espaço B com item. Age enquanto Link está ocupado com outra coisa (pendurado, escalando, nadando ...) ou com o espaço B vazio.',
           'Segure o botão por cerca de um terço de segundo para fazer isto também. O toque curto mantém a ação de cima e age na hora; quando a longa dispara, a curta é solta (assim um botão segurado não carrega a espada nem corre com A).'),
    'fr': ('Retour (action de B)', "Le B propre au jeu : se lâcher d'un rebord, annuler, descendre ... Il n'utilise pas d'objet, donc il va sur n'importe quel bouton même si l'objet 1 est ailleurs ou si l'emplacement B a un objet. Il agit quand Link est occupé à autre chose (suspendu, grimpant, nageant ...) ou si l'emplacement B est vide.",
           "Maintenez le bouton environ un tiers de seconde pour faire ceci aussi. L'appui court garde son action du dessus et agit tout de suite ; quand le long part, le court est relâché (un bouton maintenu ne peut donc pas charger l'épée ni sprinter avec A)."),
    'de': ('Zurück (B-Aktion)', 'Das eigene B des Spiels: von einer Kante loslassen, abbrechen, absteigen ... Es benutzt kein Item, also geht es auf jeder Taste, auch wenn Item 1 woanders liegt oder der B-Platz ein Item hat. Es wirkt, solange Link etwas anderes tut (hängen, klettern, schwimmen ...) oder der B-Platz leer ist.',
           'Halte die Taste etwa eine Drittelsekunde, um dies auch zu tun. Der kurze Druck behält seine Aktion oben und wirkt sofort; wenn der lange auslöst, wird der kurze losgelassen (eine gehaltene Taste kann also kein Schwert laden und nicht mit A sprinten).'),
    'it': ('Indietro (azione di B)', "Il B vero del gioco: lasciarsi da un bordo, annullare, scendere ... Non usa oggetti, quindi va su qualsiasi tasto anche se l'oggetto 1 è altrove o lo slot B ha un oggetto. Agisce mentre Link è occupato in altro (appeso, in arrampicata, a nuoto ...) o con lo slot B vuoto.",
           "Tieni premuto il tasto circa un terzo di secondo per fare anche questo. La pressione breve mantiene l'azione sopra e agisce subito; quando scatta la lunga, la breve viene rilasciata (un tasto tenuto non può quindi caricare la spada né correre con A)."),
}.items():
    if _lang in T:
        T[_lang]['role_back'] = _n
        T[_lang]['role_back_help'] = _h
        T[_lang]['hold_help'] = _hh


# held buttons: L and R too, how the short press behaves while the button is held, and the buttons that have no hold
_HOLD_TXT = {
    'en': ('Hold the button for about a third of a second to do this. Not available when the button above is the shield, target, sprint, the item selector, the armory or an item: those act while held or at once. The option below chooses what the short press does meanwhile.', 'Skip the tap action when held (adds a short delay)', 'ON: holding the button only does the hold action. To know which press it is, the tap action waits about a third of a second before it runs, so every tap on this button is a little late. OFF: the tap action runs at once, with no delay; if you keep holding, the hold action is done as well, after it.', 'No hold here: {a} acts while the button is held or at once.'),
    'es': ('Mantén el botón un tercio de segundo para hacer esto. No disponible si el botón de arriba es escudo, fijar, sprint, selector de objetos, armería o un objeto: esas acciones actúan al mantener o al instante. La opción de abajo elige qué hace mientras tanto la pulsación corta.', 'Evitar la acción normal al mantener (añade un pequeño retraso)', 'ACTIVADO: al mantener el botón solo se hace la acción de mantener. Para saber qué pulsación es, la acción normal espera un tercio de segundo antes de ejecutarse, así que cada toque en este botón llega un poco tarde. DESACTIVADO: la acción normal se ejecuta al instante, sin retraso; si sigues manteniendo, se hace también la de mantener, después.', 'Sin mantener aquí: {a} actúa mientras se mantiene el botón o al instante.'),
    'pt': ('Segure o botão por cerca de um terço de segundo para fazer isto. Indisponível quando o botão de cima é escudo, mira, corrida, seletor de itens, arsenal ou um item: essas ações agem enquanto seguradas ou na hora. A opção abaixo escolhe o que o toque curto faz nesse meio tempo.', 'Evitar a ação normal ao segurar (adiciona um pequeno atraso)', 'LIGADO: ao segurar o botão só a ação de segurar acontece. Para saber qual toque é, a ação normal espera cerca de um terço de segundo antes de rodar, então cada toque neste botão chega um pouco atrasado. DESLIGADO: a ação normal roda na hora, sem atraso; se continuar segurando, a de segurar também acontece, depois.', 'Sem segurar aqui: {a} age enquanto o botão é segurado ou na hora.'),
    'fr': ("Maintenez le bouton environ un tiers de seconde pour faire ceci. Indisponible quand le bouton du dessus est le bouclier, cibler, le sprint, le sélecteur d'objets, l'armurerie ou un objet : ces actions agissent pendant le maintien ou tout de suite. L'option ci-dessous choisit ce que fait l'appui court entre-temps.", "Éviter l'action normale au maintien (ajoute un petit délai)", "ACTIVÉ : maintenir le bouton ne fait que l'action de maintien. Pour savoir de quel appui il s'agit, l'action normale attend environ un tiers de seconde avant de s'exécuter : chaque appui bref sur ce bouton arrive donc un peu en retard. DÉSACTIVÉ : l'action normale s'exécute tout de suite, sans délai ; si vous continuez à maintenir, l'action de maintien est faite aussi, après.", 'Pas de maintien ici : {a} agit pendant le maintien du bouton ou tout de suite.'),
    'de': ('Halte die Taste etwa eine Drittelsekunde, um dies zu tun. Nicht verfügbar, wenn die Taste darüber Schild, Anvisieren, Sprint, Item-Auswahl, Waffenkammer oder ein Item ist: Diese Aktionen wirken beim Halten oder sofort. Die Option darunter legt fest, was der kurze Druck derweil macht.', 'Normale Aktion beim Halten überspringen (kleine Verzögerung)', 'AN: Beim Halten der Taste passiert nur die Halten-Aktion. Um den Druck zu unterscheiden, wartet die normale Aktion etwa eine Drittelsekunde, bevor sie läuft; jeder kurze Druck auf diese Taste kommt also etwas später. AUS: Die normale Aktion läuft sofort, ohne Verzögerung; hältst du weiter, folgt danach auch die Halten-Aktion.', 'Kein Halten hier: {a} wirkt, solange die Taste gehalten wird, oder sofort.'),
    'it': ("Tieni premuto il tasto circa un terzo di secondo per fare questo. Non disponibile quando il tasto sopra è scudo, mira, sprint, selettore oggetti, armeria o un oggetto: quelle azioni agiscono durante la pressione o subito. L'opzione sotto sceglie cosa fa nel frattempo la pressione breve.", "Evita l'azione normale quando è tenuto (aggiunge un piccolo ritardo)", "ATTIVO: tenendo il tasto si fa solo l'azione di pressione lunga. Per capire quale pressione è, l'azione normale aspetta circa un terzo di secondo prima di partire, quindi ogni tocco su questo tasto arriva un po' in ritardo. DISATTIVO: l'azione normale parte subito, senza ritardo; se continui a tenere premuto si fa anche quella lunga, dopo.", 'Nessuna pressione lunga qui: {a} agisce mentre il tasto è premuto o subito.'),
}
for _lang, (_hh, _w, _wh, _bl) in _HOLD_TXT.items():
    if _lang in T:
        T[_lang]['hold_help'] = _hh
        T[_lang]['opt_hwait'] = _w
        T[_lang]['opt_hwait_help'] = _wh
        T[_lang]['hold_blocked'] = _bl

# ---------------------------------------------------------------- model
# The order the editor lists the buttons in (face buttons, shoulders, sticks). The module's order is I.REMAP_BUTTONS.
ROW_ORDER = ("X", "X_hold", "Y", "Y_hold", "A", "A_hold", "B", "B_hold", "L", "L_hold", "R", "R_hold", "ZL", "ZR", "L3", "R3", "DR")
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
MOVE_ROLES = ("jump", "sword", "action", "target", "camera", "sprint", "shield", "armory", "back")
ROLE_HELP = {"action": "role_action_help", "item_main": "role_item_main_help", "armory": "role_armory_help",
             "back": "role_back_help"}


def role_vars(saved, master=None) -> dict:
    roles = I.normalize_roles(saved if isinstance(saved, dict) else I.CLASSIC_ROLES)
    return {b: tk.StringVar(master=master, value=roles[b]) for b in I.ROLE_KEYS}


def roles_value(rvars: dict) -> dict:
    return I.normalize_roles({b: v.get() for b, v in rvars.items()})


def set_roles(rvars: dict, roles: dict) -> None:
    roles = I.normalize_roles(roles)
    for b, var in rvars.items():
        if var.get() != roles[b]:
            var.set(roles[b])


def preset_of(roles: dict) -> str:
    """The preset a table is (the D-pad right long press is no part of a preset: it keeps its own role)."""
    free = {k: "none" for k in ("DR", *I.HOLD_KEYS)}
    roles = dict(I.normalize_roles(roles), **free)
    for k in I.REMAP_PRESET_KEYS:
        if dict(I.normalize_roles(I.REMAP_PRESETS[k]), **free) == roles:
            return k
    return "custom"


def menu_buttons(roles: dict) -> dict:
    """core/remap.c remap_menu_buttons(): the pause-menu button that assigns each of items 1..3."""
    phys = {r: b for b, r in roles.items() if r != "none" and b in I.REMAP_BUTTONS}
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
        self._traces.append((v["items"], v["items"].trace_add("write", lambda *_: self.redraw())))
        for var in v["roles"].values():
            self._traces.append((var, var.trace_add("write", lambda *_: self.redraw())))
        self.frame.bind("<Destroy>", self._drop_traces)
        self._shown_opts: dict[str, tuple] = {}
        self._enabled = True
        self._lock: set = set()   # the lists of the held rows that have no hold now (their button's own role acts held)
        self.redraw()

    # ---- helpers ----
    def _drop_traces(self, _e=None):
        for var, tid in self._traces:
            try:
                var.trace_remove("write", tid)
            except (tk.TclError, ValueError):
                pass
        self._traces = []

    def set_enabled(self, on: bool) -> None:
        """Button mapping off: the table stays on screen but frozen (the app keeps it for when it comes back)."""
        self._enabled = bool(on)
        for cb in self.combos.values():
            try:
                cb.config(state="readonly" if on else "disabled")
            except tk.TclError:
                pass
        self._freeze_all()

    def _freeze_all(self) -> None:
        for w in self.frame.winfo_children():
            self._freeze(w)

    def _freeze(self, w) -> None:
        """Every list and box of the table follows the switch (the role options are rebuilt on each redraw)."""
        if isinstance(w, (UI.Select, UI.Check)):
            try:
                live = self._enabled and w not in self._lock
                w.config(state=("readonly" if isinstance(w, UI.Select) else "normal") if live else "disabled")
            except tk.TclError:
                pass
        for c in w.winfo_children():
            self._freeze(c)

    def role_name(self, role: str) -> str:
        return self.L("role_" + role)

    def _style(self) -> str:
        s = self.v["bstyle"].get()
        return s if s in GLYPHS else "switch"

    def _items_on(self) -> bool:
        """The item side: our own item buttons and the item roles (both switches on)."""
        return bool(self.v["bmap"].get() and self.v["items"].get())

    def _roles(self) -> dict:
        roles = roles_value(self.v["roles"])
        # (with the custom item HUD off the module lets an item role's press through untouched: the game's own
        #  item buttons, so the table keeps them and everything else is remapped as usual)
        return roles

    # ---- editing ----
    def _pick(self, btn: str):
        new = self.choices[btn][self.combos[btn].current()]
        rv = self.v["roles"]
        old = rv[btn].get()
        if new == old:
            return
        table = {b: var.get() for b, var in rv.items()}
        if new != "none" and new not in I.DUP_ROLES:
            # each role on one button (the shortcut roles excepted: those may repeat): the button that had it
            # takes this one's old role (if it may), else nothing.
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

    def _options_for(self, btn: str, role: str, blocked_by: str | None = None):
        """The options of the role on this row: target (switch, next one) and sprint (toggle); on a held row, how the
        short press behaves, or why the row has no hold (blocked_by: the role of its button)."""
        if self._shown_opts.get(btn) == (role, blocked_by):
            return
        self._shown_opts[btn] = (role, blocked_by)
        help_slot, f = self.opts[btn]
        for w in list(f.winfo_children()) + list(help_slot.winfo_children()):
            w.destroy()
        items = []
        if role == "target":
            items = [("opt_shield", self.v["mshield"]), ("opt_switch", self.v["switch_target"]),
                     ("opt_auto", self.v["auto_target"])]
        elif role == "sprint":
            items = [("opt_toggle", self.v["sprint_toggle"])]
        # A hold on a face button or L / R: how its short press behaves. D-pad right has no such choice: its short
        # press is the game's Navi call and always waits for the hold (the note below says so).
        wait_key = btn[:-5] if btn in I.HOLD_KEYS else None
        if wait_key and not blocked_by and role != "none":   # a hold is set: how its short press behaves
            items.append(("opt_hwait", self.v["hwait"][wait_key]))
        bg = self.c["PANEL"]
        for key, var in items:
            line = UI.Frame(f, bg=bg)
            line.pack(anchor="w", pady=1)
            UI.Check(line, text=self.L(key), variable=var, bg=bg, fg=self.c["TEXT"],
                     selectcolor=self.c["PANEL2"], font=(self.fam, 9)).pack(side="left")
            help_icon(line, self.L(key + "_help"), self.c, self.fam).pack(side="left", padx=(4, 0))
        if role in ROLE_HELP:
            help_icon(help_slot, self.L(ROLE_HELP[role]), self.c, self.fam).pack(side="left")
        if btn in I.HOLD_KEYS:   # what a long press means for this button
            help_icon(help_slot, self.L("hold_help"), self.c, self.fam).pack(side="left", padx=(4, 0))
        if blocked_by:   # the button's own role acts held or at once: nothing to hold for
            line = UI.Frame(f, bg=bg)
            line.pack(anchor="w", pady=1)
            UI.Label(line, text=self.L("hold_blocked", a=self.role_name(blocked_by)), bg=bg, fg=self.c["DIM"],
                     font=(self.fam, 9), justify="left", anchor="w", wraplength=330).pack(side="left")
            items = items or [None]
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
        items = self._items_on()
        self._lock = set()
        for b, cb in self.combos.items():
            # the item roles are only offered with the custom item HUD; without it the table's own stays visible
            ch = [r for r in self.choices[b] if ((on or r not in ("item1", "item2", "item3", "selector"))
                                                and (items or r not in I.ITEM_ROLES or r == roles[b]))
                  and (r != "shield" or self.v["mshield"].get() or roles[b] == "shield")]
            blocked = None
            if b in I.HOLD_KEYS and roles[b[:-5]] in I.HOLD_BLOCK_ROLES:   # no hold on a shield / target / item ... button
                blocked, ch = roles[b[:-5]], ["none"]
                self._lock.add(cb)
            cb.config(values=[self.role_name(r) for r in ch])
            self.choices[b] = ch + [r for r in self.choices[b] if r not in ch]   # (hidden ones stay pickable by index)
            cb.current(ch.index(roles[b]) if roles[b] in ch else 0)
            if b in I.HOLD_KEYS:   # the long press of the face button above: a small chip and a down arrow
                self.chips[b].delete("all")
                self._chip(self.chips[b], 14, 14, b[0], 9, clear=False)
                self.chips[b].create_text(31, 14, text="↓", fill=self.c["DIM"], font=(self.fam, 11, "bold"))
            else:
                self._chip(self.chips[b], 20, 14, b, 12)
            self._options_for(b, roles[b], blocked)
        n_items = sum(1 for r in roles.values() if r in I.ITEM_ROLES)
        self.count.config(text=self.L("rm_items", n=n_items) if items else "")
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
        self.info.config(text=self.L("rm_menu", a="   ".join(pairs)) if pairs and items else "")
        self._draw_pad(roles)
        self._draw_hud(roles)
        self._freeze_all()

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
            text = self.role_name(role)
            hold = roles.get(btn + "_hold", "none")
            if hold != "none":   # a long press role too: "Jump  ↓ Back"
                text = f"{text}  ↓ {self.role_name(hold)}" if role != "none" else f"↓ {self.role_name(hold)}"
            cv.create_text(lx, ly, text=text, fill=self._role_color(role), anchor=anchor,
                           font=(self.fam, 9, "bold"))

    def _draw_hud(self, roles: dict):
        """core/hud.c sb_xy() / slots_draw(): the HUD cluster, centred in the preview."""
        cv = self.hud
        cv.delete("all")
        simple = self.v["bsimple"].get()
        positions = HUD_GAMECUBE if self.v["bplace"].get() == "gamecube" else HUD_POS
        phys = {r: b for b, r in roles.items() if r != "none" and b in I.REMAP_BUTTONS}
        sc = 0.37
        ox, oy = 200 - 1292.6 * sc, 118 + 712.0 * sc

        def to_px(x, y):
            return ox + x * sc, oy - y * sc

        hud_roles = ["item2", "jump", "item3", "sword", "action", "item_main", "item1"]
        if not self._items_on():
            hud_roles = [r for r in hud_roles if r not in I.ITEM_ROLES]   # the game's own item HUD
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
