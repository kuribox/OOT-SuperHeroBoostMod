# Menú rápido de espada y escudo

Estado: **primera implementación** en la rama `feature/sword-shield-menu`. Rama de referencia: *Hyrule Armory
Sync v1.1* (solo se replica el comportamiento descrito en su README; no se usa su código, sus tablas ni sus assets).

## Cómo se usa

- El botón es el rol **«Menú de espada y escudo»** de la tabla de botones (app > Controles > Botones del mando).
  Se puede poner en cualquiera de los diez botones físicos.
- **Mantener** el botón ~0.2 s abre el drawer.
- **Cruceta arriba / abajo**: cambia entre la fila de espadas y la de escudos.
- **Cruceta izquierda / derecha**: recorre la fila.
- **Soltar** el botón equipa la celda marcada y cierra.
- La **primera celda** de cada fila está vacía: desequipa esa categoría; el objeto sigue en la bolsa.
- El drawer recuerda la última fila usada durante la sesión (`ssm_flags` bit 0 lo desactiva).

Nota de diseño: el mod de referencia navega con los sticks; aquí se usa la **cruceta** para que el jugador no se
mueva ni gire la cámara mientras elige. Si se prefiere el stick, hay que identificar y neutralizar sus campos en
las muestras HID.

## Qué se ha implementado

| Pieza | Dónde |
|---|---|
| Estado, navegación, equipado | `features/sword_shield_menu/sword_shield_menu.c` |
| Entrada (antes del remap) | `tools/build_core.py`: `FEATURE_INPUT_EARLY["sword_shield_menu"]` |
| Rol del botón | `core/remap.h` / `core/remap.c`: `ROLE_DRAWER`; el juego no ve el botón |
| Configuración | `core/config.h`: `ssm_enabled`, `ssm_flags` (append-only) |
| Drawer en pantalla | `core/hud.c`: `drawer_update()`, panes `T_ShbTxt_14..27` y placa `P_ShbPic_275` |
| App | `app/installer.py`, `app/main.py`, `app/remap_ui.py` (textos en 6 idiomas) |
| Build | `tools/make_release.py` (las dos variantes incluyen la feature) |

Datos del juego (ya usados por el proyecto): String64 `PlayerStatus.EquipWeapon` = índice **23** y
`PlayerStatus.EquipShield` = **22**; la escritura usa `equip_request` + `equip_refresh` (la ruta del menú de pausa).
La propiedad se comprueba con `hud_owns_known()` (`IsGet.<actor>`); si un nombre no existe en la tabla del build,
se lista igualmente para no ocultar todo el equipo.

Edad: Kokiri solo de niño; Master, Giant y Biggoron solo de adulto; escudos Deku e Hylian para ambos y Espejo solo
de adulto. El cuchillo roto de Biggoron no se lista como espada.

## Aspecto

El drawer se dibuja con los paneles de texto que el HUD ya clona y una placa oscura hecha con la textura de placa
del propio juego. La fila activa muestra la celda elegida en amarillo, el equipo puesto en verde y el resto en gris.
Las celdas usan nombres cortos (`Kokiri`, `Deku`...).

Pendiente para la siguiente iteración: iconos de cada espada/escudo. El HUD ya resuelve nombres como
`Weapon_Sword_Master` a través del controlador de iconos del juego (es lo que pinta el botón Y), pero montar ese
controlador para las celdas del drawer necesita probarse en el juego.

## Abrir y cerrar

- Solo en partida (`player_active`, `!player_dead`, `!hud_game_covered`). En pausa, diálogos, cutscenes o muerte se
  cierra sin aplicar.
- Mientras está abierto se consumen el botón y la cruceta; el resto de botones sigue su curso.
- Si no hay ninguna espada ni escudo que elegir, no se abre.

## Configuración

```c
unsigned char ssm_enabled;   /* 1 = activo */
unsigned char ssm_flags;     /* bit0: no recordar la última fila */
unsigned char ssm_unused[2];
```

La app escribe `f+74` y `f+75` cuando el módulo tiene al menos `0x5E` bytes de configuración. Los presets no lo
activan: es una preferencia del jugador, como el remap.

## Pendiente de probar en el juego

1. Que el módulo arranque con los 14 paneles de texto y la placa extra (contadores de las tablas del HUD).
2. Posición y tamaño del marco y de las celdas (valores de layout a ojo: `drawer_update()`).
3. Que la cruceta no mueva al jugador ni dispare quick select mientras el drawer está abierto.
4. Que el equipado se aplique al soltar y persista al guardar/cambiar de área.
5. Nombres de `IsGet` de cada espada/escudo (si falta alguno, se lista igualmente).
6. El `kind` del `equip_request` (se usa 3, el mismo de botas/máscara); si espada y escudo usan otro, ajustarlo.
7. Abrir/cerrar en pausa, diálogo, cutscene, muerte y con cada botón físico.

## Riesgos conocidos

- **Panes**: añadir paneles cambia los contadores del HUD; un nombre o hash mal puesto deja el HUD sin construir
  (`g_hud_dbg[9]`). El build compila y enlaza, pero hay que verlo arrancar.
- **Desequipar**: se escribe el String64 vacío y no se llama al equip request (evita pasarle un nombre vacío);
  puede que el modelo tarde en desaparecer hasta recargar el área o abrir el menú.
- **Conflicto de botón**: si el rol cae en L y L es un slot de objeto, el drawer tiene prioridad mientras el menú
  está activo; la app permite moverlo a otro botón.
