/* Linker script for our own subsdk9 module.
 * Placed right after subsdk0 in the stock module order.
 * All MOD0 offsets are computed here from real section addresses. */
ENTRY(_start)

__base = 0x4F80000;

SECTIONS
{
    . = __base;
    .text : ALIGN(0x1000) { *(.module) *(.text*) }
    .rodata : ALIGN(0x1000) { *(.rodata*) }
    .data : ALIGN(0x1000) {
        *(.data*)
        /* The dynamic tables go after the existing data: their entries would otherwise move every .data
         * symbol, and the generated C embeds those addresses. Fits in the page's padding. */
        __dynamic_start = .;
        KEEP(*(.dynamic))
        /* Minimal ELF dynamic symbol table, string table and SysV hash: Rekarina's loader refuses a module
         * without DT_SYMTAB, DT_STRTAB and a symbol hash (runtime/loader.c parse_dynamic). */
        __shb_dynstr_start = .;
        KEEP(*(.shb_dynstr))
        __shb_dynstr_end = .;
        . = ALIGN(8);
        __shb_dynsym_start = .;
        KEEP(*(.shb_dynsym))
        __shb_dynsym_end = .;
        . = ALIGN(4);
        __shb_hash_start = .;
        KEEP(*(.shb_hash))
        __shb_hash_end = .;
    }
    .bss : ALIGN(0x1000) {
        __bss_start = .;
        *(.bss*) *(COMMON)
        . = ALIGN(16);
        __module_object = .;
        . = . + 0x200;
        __bss_end = .;
    }

    /* main-relative addresses of the lazy-PLT0 resolver and the input stubs'
     * GOT slots (used by core/hooks.S; adrp needs a symbol, not an .equ). */
    STOCK0 = 0x3C427F0;
    STOCK1 = 0x3C42810;
    STOCK2 = 0x3C42820;
    STOCK3 = 0x3C42800;
    STOCK3B = 0x3C42804;
    STOCK4 = 0x3C42830;
    SLOTFN = 0x1211C00;
    equip_request = 0xA7ACD0;  /* (pouch, name_ref, kind, arg3): the game's equipment request, as the pause menu makes it */
    equip_refresh = 0x33AC230; /* (pouch, name_ref, 1): apply/refresh after the request */
    RES_FRAME = 0x2B30DE4;
    RES_FRAME4 = 0x2B30DE8;
    RES_MP = 0x33BDCE4;
    RES_UI = 0x43A6FC;
    RES_SETFRAME = 0x2D2D94;
    RES_INST = 0x152F8F4;
    RES_MOVE = 0x2C215E4;
    RES_NUM = 0x33C3B14;
    show_ui = 0x2CDEF0;
    set_text = 0x439520;
    WORLD_MGR = 0x4E7EAB0;
    GAMEDATA_MGR = 0x4E842A8;
    ATTENTION_SYS = 0x4E24B28;  /* AttentionSystem *: actor link of the lock-on target at +0xB68 (entry +0x70, id +0x78) */
    os_mutex_lock = 0x3C42460;  /* import stubs (GOT 0x4CBF3F8 / 0x4CBF400) */
    os_mutex_unlock = 0x3C42470;
    GOT_MUTEX_LOCK = 0x4CBF3F8;
    GOT_MUTEX_UNLOCK = 0x4CBF400;
    read_enum = 0x17BF90;   /* gmd::readEnum(manager, out, descriptor, field_hash) */
    read_bool = 0x179F70;   /* gmd::readBool(manager, out, descriptor, field_hash) */
    RES_GUARD0 = 0x27F7D04;
    RES_GUARD1 = 0x27F7F94;
    RES_GUARD2 = 0x27FB0C4;
    RES_GUARD3 = 0x27FA204;
    RES_SPEED = 0x9352F4;
    RES_SPEED4 = 0x9352F8;
    RES_OCARINA_ENTER = 0x2894344;
    RES_OCARINA_LEAVE = 0x28947F4;
    RES_OCARINA_UI = 0x3292074;         /* ocarina UI update, after pacibsp */
    RES_OCARINA_IS_OPEN = 0x3294990;
    RES_OCARINA_UPDATE = 0x3296E20;    /* (ctx) -> 1 while the ocarina UI is up */
    RES_OCARINA_UPDATE = 0x3296E20;     /* (ctx) ocarina state machine entry */
    /* Songbook store hook: continue right after `str w20,[x19,#0x22c]`. Must be a
     * linker symbol: a numeric literal here assembles as a PC-relative offset. */
    RES_SONGBOOK_RESUME = 0x3294BD0;
    RES_ICONCTRL = 0xA37324;
    RES_HUDINIT = 0x33F5D24;
    RES_SETTEXT = 0x439524;
    RES_MENUSEL = 0x2CBD24;
    RES_NVNLOAD = 0x129E294;
    RES_MENUPAGE = 0xF78BAC;
    RES_FSOPEN = 0x3C42594;
    GOT_FSOPEN = 0x4CBF490;    /* nn::fs::OpenFile's GOT slot */
    fs_open_file = 0x3C42590;  /* nn::fs import stubs */
    fs_close_file = 0x3C425B0;
    fs_create_file = 0x3C425D0;
    fs_write_file = 0x3C425E0;
    fs_set_file_size = 0x3C42940;
    fs_read_file = 0x3C43050;  /* ReadFile(size_t *out, handle, offset, buf, size) */
    /* manual shield: guard/AS functions in main and their resume points */
    resolve_player = 0x973030;
    get_guard_component = 0x974EB0;
    native_draw_equipment = 0x6B6820;
    as_int = 0x37BB2C0;
    as_bool = 0x37BB4C0;
    GUARD_SETTER_BODY = 0x37BA104;
    GUARD_SETTER_EARLY = 0x37BA138;
    GUARD_FLAGS_RESUME = 0x972E1C;
    GUARD_UPDATE_RESUME = 0x973210;
    icon_widget_init = 0x33B7DA0;  /* (widget, heap): widget+0x20 = part holding Pa_ReplaceIcon_00; builds its icon controller (initial icon Item_AgonyStone) */
    /* armory drawer (features/quick_select/armory_drawer.c, BL_SITES in tools/hook_sites.py) */
    AD_SORT_RESTORE = 0x33ABF80; /* (pouch, flag): the drawer's sort option from GameData, at open */
    AD_CONT_ITEMS = 0xA6F730;    /* (pouch data): the items' container (category 0) */
    AD_CONT_EQUIP = 0xA6F790;    /* (pouch data): the equipment container (category 1) */
    AD_GD_BOOL = 0x20FD90;       /* GameData bool read (list build: sorted list) */
    AD_RECORD = 0xA6EF20;        /* (pouch, category, index, &a, &b): an entry's record */
    AD_FILTER = 0x596AF0;        /* (pouch, name_ref, a, record, child): keep an entry in the drawer list */
    AD_CATEGORY = 0xA6F690;      /* (pouch, category): a category's container */
    AD_CELL_ICON = 0xA75EF0;     /* (cell, name_ref, count, ...): a drawer cell's icon and count */
    AD_OWNED = 0x5963E0;         /* (pouch, name_ref, 1, 0): how many Link has (>= 1: owned) */
    AD_PANE_SCALE = 0xB29B20;    /* (pane, sx, sy) */
    AD_PANE_MOVE = 0x20A3E0;     /* (pane, x, y, z) */
    AD_PANE_SHOW = 0x20AEA0;     /* (pane, visible) */
    AD_RET_MOVE = 0x33C95C4;     /* return address of the drawer's cursor-move call */
    AD_SORT_SHOW = 0x208350;     /* (part, flag): called where the drawer's layout holds the sort window */
    AD_UPDATE_RESUME = 0x33C9274; /* the drawer's per-frame update, after its pacibsp */
    AD_CELL_AT = 0x1505A40;      /* (cell list, index): a cell */
    AD_CURSOR_MOVE = 0x33C9800;  /* (drawer, from, to): the cursor's move animation */
    AD_CURSOR_SET = 0x33C8F50;   /* (drawer, index): select a cell (name text, ...) */
    AD_FIND_PANE = 0x43E1E0;     /* (layout, name): a pane by name */
    AD_STICK_SLOT = 0x4E982E0;   /* -> controller state: +0xC0 -> sticks (floats +0x120 left x/y, +0x128 right) */
    AD_EQUIP_OBJ = 0x4C7E180;    /* -> pointer to the GameData accessor */
    AD_GD_READY = 0x9B3EB0;      /* (gd, buf16, 0x060890B0): bit 0 = readable */
    AD_GD_STRING = 0xA7A860;     /* (gd, &name, buf16, hash): a String64 by hash (EquipWeapon / EquipShield) */
    tex_replace = 0xA73920;     /* (replacer, const char **name, flag): load UI/Tex/Icon/<name> into a replaceable pane texture */
    item_actor_name = 0x592B20; /* (manager, const char **out, item index): actor name of an item (Item_Boomerang ...) */
    ITEM_MGR_SLOT = 0x4C7E0D0;  /* -> pointer to the item manager pointer */
    ITEM_NAME_DEFAULT = 0x4C7DAD0; /* -> default (empty) name pointer */
    ITEM_SET_SLOT = 0x4C7E2A8;  /* -> pointer to an object whose byte +0x5C0 is the item set passed to the equip call */
    MAIN_SCHED_VT = 0x4B05318;
    find_part = 0x43C3E0;
    ui_find_sub = 0x43C4A0;     /* (name, part, &sub part): a nested part by name */
    ui_owner_layout = 0x43C040; /* (pane+0x40): the layout that owns a pane */
    find_anim = 0x43C7A0;
    set_frame = 0x2D2D90;
    ui_btn_anim = 0x3772000;   /* (menu cell button, index): its animation (4 = Decide) */
    UI_CTRL_VT = 0x4C216E8;   /* UI control (button) vtable: the item box / menu cells (core/hud.c part_ctrl) */
    ui_btn_decide = 0x3772100; /* (menu cell button): the game's own Decide play, with its sound (A on an Items cell) */
    PLT0 = 0x3C41C50;
    GOT0 = 0x4CBF5C0;
    GOT1 = 0x4CBF5D0;
    GOT2 = 0x4CBF5D8;
    GOT3 = 0x4CBF5C8;
    GOT4 = 0x4CBF5E0;

    /* Module-relative offsets consumed by MOD0 in core/module.S */
    __dynamic_off    = __dynamic_start - __base - 0x20;   /* MOD0 fields are offsets from MOD0 (at base+0x20) */
    __bss_start_off  = __bss_start - __base;
    __bss_end_off    = __bss_end - __base;
    __module_object_off = __module_object - __base;
    /* Dynamic table pointers in the .dynamic section (module-relative, as MOD0's fields) */
    __shb_dynstr_off = __shb_dynstr_start - __base;
    __shb_dynstr_size = __shb_dynstr_end - __shb_dynstr_start;
    __shb_dynsym_off = __shb_dynsym_start - __base;
    __shb_hash_off   = __shb_hash_start - __base;
}
