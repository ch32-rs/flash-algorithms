ENTRY(init)
SECTIONS {
  . = 0x20000008;
  .text : {
    KEEP(*(.text.init))
    KEEP(*(.text.uninit))
    KEEP(*(.text.erase_sector))
    KEEP(*(.text.erase_all))
    KEEP(*(.text.program_page))
    *(.text .text.*)
  }
  .rodata : { *(.rodata .rodata.*) }
  .data : { *(.data .data.*) }
  .bss : { *(.bss .bss.*) *(COMMON) }
}
