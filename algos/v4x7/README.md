# CH32V407 / CH32V467 flash algorithms

Uses the existing `flash-algorithm` template, compiled against the
`flash_v4x7` PAC with CH32V467WEU6 as the representative chip. The crate is
named after the flash IP, consistent with `v3`, `h4`, and the other algorithms.

- USR: 256-byte fast programming (`FTPG`, `WR_BSY`, `PGSTRT`), 4 KiB PER erase.
- USR bulk erase: walk the 992 KiB user range with PER, never SYS or OPT.
- SYS: 256-byte fast programming and 4 KiB PER; no bulk erase entry point.
- OPT: halfword programming and option-byte erase; bulk erase is rejected.
- VND/ESIG: read-only, no algorithm.

V4x7 has no V3 `PAGE_ER`, `BTPG`, or `BTER` bits. The programming sequence
follows WCH's [ch32v4x7_flash.c](https://github.com/openwch/ch32v407_ch32v467/blob/main/EVT/EXAM/SRC/Peripheral/src/ch32v4x7_flash.c),
particularly `FLASH_ErasePage` and `FLASH_ProgramPage_Fast`.

Bootstrap the PAC from the updated ch32-data submodule first, as described
in the root README. Build from this directory with
`cargo +nightly build --release`. Use the
existing xtask to extract the three binaries and generate the family YAML.
The generator currently emits `CH32V4_Series.yaml`; for probe-rs, rename it to
`CH32V4X7_Series.yaml` and manually add the WCH-Link detection information:

- Family byte `0xA6` belongs to the WCH-Link transport, not the chip ID table.
- IDs: V407 VET/WEU/RET = `0x46700000` / `0x46710001` / `0x46720002`;
  V467 VET/WEU/RET = `0x46730000` / `0x46740001` / `0x46750002`.
  Mask `0xffffff0f` ignores revision bits [7:4], not the low package nibble.
  Hardware WEU6 readback was `0x46740011`.
- USER option byte `0x1ffff802`, bit 6: 0 = 200 KiB RAM; 1 = 136 KiB RAM.
- Total USR stays 992 KiB; its zero-wait boundary moves from 512 to 576 KiB.
- Remove OPT's generated `pc_erase_all`: the template exports an unsupported
  stub, not a usable bulk erase operation. SYS must omit that entry too.

Do not regenerate over hand-maintained detection fields without preserving
them. Building and validating the YAML does not validate flash operations on
hardware. In particular, SYS and OPT programming must be tested separately;
do not use them for an ordinary application firmware download.
