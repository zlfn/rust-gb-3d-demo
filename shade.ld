/* The shading tables, at the fixed WRAM pages `src/tables.rs` names. */
SECTIONS {
    .shade 0xC000 (NOLOAD) : {
        KEEP(*(.shade .shade.*))
    } > WRAM
}
INSERT BEFORE .data;

ASSERT(ADDR(.shade) == 0xC000 && SIZEOF(.shade) == 0x900, "shade tables must be 0x900 bytes at 0xC000");
