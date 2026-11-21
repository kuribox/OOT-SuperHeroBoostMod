# Menú rápido de espada y escudo

Estado: **diseño**. Rama `feature/sword-shield-menu`. Todavía no hay código de la función: este documento fija el
comportamiento, los datos y el plan para implementarla sin reutilizar el mod de referencia.

Referencia analizada: *Hyrule Armory Sync v1.1* (README + módulo `subsdk5` + IPS). Solo se toma su comportamiento
descrito; **no se copia código, tablas ni assets**, y el mod no se incluye ni se enlaza.

## Comportamiento a replicar

- Mantener el botón asignado abre el *drawer* de equipo (en el mod original es L / L1).
- Stick **arriba / abajo**: cambia entre la fila de espadas y la de escudos.
- Stick **izquierda / derecha**: recorre las celdas de la fila.
- Al **soltar** el botón se confirma lo seleccionado; mientras se mantiene, nada se equipa.
- El drawer recuerda la última fila usada durante la sesión.
- La **celda vacía** de la izquierda desequipa esa categoría; el objeto sigue en la bolsa.
- Solo se listan y equipan espadas y escudos que el jugador **ya tiene** (como la página de Items del menú de pausa).
- R / R1 sigue abriendo el selector nativo de objetos: esta función no lo toca.

Decisiones nuestras (a confirmar al probar): cancelar con B o abriendo un menú/cutscene, y no abrir el drawer si el
inventario no tiene ninguna espada ni escudo.

## Datos que ya usa el proyecto

Todo lo necesario está ya identificado; no hace falta RE nuevo salvo los `kind` del equip request.

| Dato | Dónde está |
|---|---|
| `PlayerStatus.EquipWeapon` = String64 índice **23** | `core/hud.c` (`GD_S64_EQUIP_WEAPON`), `equip_mirror()` |
| `PlayerStatus.EquipShield` = String64 índice **22** | `core/hud.c` (`GD_S64_EQUIP_SHIELD`) |
| Lectura/escritura del String64 (live `+0x74`/`+0x68`, copia menú/save `+0xC4`/`+0xB8`) | `features/quick_select/quick_select.c`: `gd_get_string()` / `gd_set_string()` |
| Aplicar el equipo con la ruta del menú (`equip_request` + `equip_refresh`) | `features/quick_select/quick_select.c`: `apply_equip()` |
| Propiedad del objeto (`IsGet.<actor>`) | `core/hud.c`: `hud_owns_name()` |
| Entrada del mando y botones virtuales | `core/hid.c`, `core/remap.c`, sitio de input 3 del dispatcher |
| HUD: dibujar texto/paneles | `core/hud.c`: `put_text()`, `place()`, `tint_rec()`, `build_menu()` |

Nombres candidatos (verificar contra el dump; el HUD ya usa `Weapon_Sword_*` como nombre de textura):

- Espadas: `Weapon_Sword_Kokiri`, `_Master`, `_Giant`, `_Biggoron`, `_BrokenGiant`. `_BrokenGiant` (cuchillo roto)
  no debería poder equiparse como espada.
- Escudos: `Armor_Shield_Deku`, `_Hylian`, `_Mirror`.

Restricciones de edad a comprobar: Kokiri solo de niño; espada de Biggoron y escudo espejo solo de adulto.

## Entrada

Dos opciones, en orden de preferencia:

1. **Rol nuevo (`ROLE_DRAWER`)** dentro del remap (`core/remap.h`, `core/remap.c`). El jugador asigna el rol a
   cualquier botón desde la app, igual que el escudo manual o el sprint; el botón se consume y el juego no lo ve
   mientras abre el drawer. Es la opción coherente con 0.9.0.
2. Campo propio `ssm_button` (índice `PB_*`). Más simple, pero duplica la UI de reasignación y puede chocar con L
   (slot de objeto) o con el escudo manual.

Por defecto: **L**. Si L ya es un slot de objeto, la app debe avisar del conflicto y proponer otro botón.

## Interfaz

- **Fase A (iteración rápida, sin assets nuevos):** reutilizar los paneles de texto que ya clona `hud.c`
  (`T_ShbTxt_*`, `put_text`) para pintar una lista de hasta 6 filas + título. Permite probar navegación y equipado
  en el juego antes de invertir en el marco.
- **Fase B (aspecto final):** clonar `P_SubItemBg_00` para el marco y colocar una celda vacía + los iconos, usando
  los nombres de textura que el propio juego usa para el equipo (`Weapon_Sword_*`, `Armor_Shield_*`). El resaltado
  va con `tint_rec()` y la selección actual con un marco más claro; el objeto equipado, con el marco oscuro.

El drawer se dibuja en el frame del HUD, no en el menú de pausa, y solo con el juego en partida (`hud_game_covered()`
en falso). Nada de paneles propios del mod de referencia.

## Configuración (append-only)

Campos nuevos al final de `ShbConfig` (los anteriores no se mueven):

```c
unsigned char ssm_enabled;   /* 0 off (default), 1 activo */
unsigned char ssm_role;      /* ROLE_DRAWER o 0 = sin botón */
unsigned char ssm_remember;  /* 1 recordar la última fila (default), 0 empezar en espadas */
unsigned char ssm_flags;     /* reservado */
```

- `installer.py`: offsets nuevos y `Settings`.
- `main.py`: interruptor en **Controls** y selector de botón; presets lo dejan **off**.
- `i18n.py`: textos en los seis idiomas.
- Sin bump de `version` (crecimiento append-only; una app vieja deja los bytes a 0 = off).

## Máquina de estados

```
CLOSED --(botón mantenido >= HOLD_FRAMES)--> OPEN(fila = última)
OPEN --(stick arriba/abajo)--> cambia fila (repeat con retardo)
OPEN --(stick izq/der)--> mueve la celda (repeat con retardo)
OPEN --(soltar)--> APPLY --(latch: esperar release)--> CLOSED
OPEN --(pausa/menú/cutscene/dialogo)--> CLOSED sin aplicar
```

- El botón se consume durante todo el gesto; el resto de botones siguen su curso.
- Repetir la fila con stick no debe equipar hasta soltar.
- `APPLY` escribe el String64 (live **y** copia del menú/save) y llama a `equip_request`/`equip_refresh`.
- Verificar el `kind` del equip request para espada y escudo (quick_select usa `3` para botas/cabeza): si son
  categorías distintas, usar el valor de cada slot; no asumir que `3` sirve para los cuatro.

## Casos límite

- Save sin espada/escudo: el drawer no se abre (o solo ofrece la celda vacía).
- Objeto no poseído: nunca se lista ni se equipa.
- Cuchillo roto de Biggoron: se lista como objeto, pero no como espada equipable.
- Niño/adulto: filtrar Kokiri, Biggoron y Espejo según `hud_is_child()`.
- Guardar y recargar, cambiar de área, morir y continuar: el equipo debe persistir.
- Longshot/Hookshot/ReLink: mismo módulo; requiere rebuild una vez.
- La app avisa si el botón elegido ya tiene otro rol (L como objeto, L como escudo manual, ZL target...).

## Plan por fases

1. **Lógica sin UI**: tablas de candidatos, propiedad, aplicar equipo y selección. Probar con el debug del HUD.
2. **Input + estados**: rol/botón configurable, hold, repetición y cancelaciones.
3. **UI fase A**: lista de texto y resaltado con los paneles existentes.
4. **UI fase B**: marco e iconos con las texturas del juego.
5. **App**: opción, botón, presets, i18n y offsets del instalador.
6. **Pruebas**: emulador + Longshot, matriz de saves, documentación y nota de release.

## Pruebas mínimas antes de publicar

- Save 100 %, save recién empezado y save solo con Kokiri.
- Niño y adulto; cuchillo roto; escudo espejo.
- Abrir/cerrar en partida, pausa, diálogo, cutscene y muerte.
- R y el selector nativo siguen funcionando.
- Guardar, recargar y cambiar de área.
- Con el botón reasignado a cada uno de los diez botones físicos.

## Riesgos

- **Panes del HUD**: clonar paneles exige nombres y hashes (`g_thash`, `g_pihash`) y respetar los contadores de las
  tablas; un nombre de más rompe el boot del HUD. Validar en el arranque con `g_hud_dbg`.
- **Equip request**: hacerlo en el frame del input puede competir con la acción del botón; consumir el botón y
  aplicar al soltar evita la mayoría de casos.
- **Conflicto de botones**: el rol debe consumir el botón también cuando el drawer está cerrado pero el gesto
  empieza, o el juego verá un L suelto y abrirá su propio selector.
