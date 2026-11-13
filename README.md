# ch32-rs flash-algorithms

[probe-rs](https://probe.rs) flash algorithms and target YAMLs for WCH's CH32
MCU families, built on top of [ch32-metapac](https://github.com/ch32-rs/ch32-metapac).

## Coverage

One algo crate per (flash IP version, CPU arch) pair. `f1` is a Cortex-M3
build of the same `flash_v1` logic used by `v1` on Qingke RISC-V — Cargo can't
unify two ch32-metapac chip features, so the lib is duplicated.

| Crate        | Flash peripheral | Target triple                      | Chip families              |
| ------------ | ---------------- | ---------------------------------- | -------------------------- |
| `algos/v0`   | `flash_v0`       | `riscv32ec-unknown-none-elf`       | CH32V003, CH641            |
| `algos/v00x` | `flash_v00x`     | `riscv32ec_zmmul-unknown-none-elf` | CH32V002/4/5/6/7, CH32M007 |
| `algos/v1`   | `flash_v1`       | `riscv32imac-unknown-none-elf`     | CH32V103                   |
| `algos/v3`   | `flash_v3`       | `riscv32imac-unknown-none-elf`     | CH32V2xx, CH32V3xx         |
| `algos/v4x7` | `flash_v4x7`     | `riscv32imac-unknown-none-elf`     | CH32V407, CH32V467         |
| `algos/x0`   | `flash_x0`       | `riscv32imac-unknown-none-elf`     | CH32X035/X033, CH643       |
| `algos/l1`   | `flash_l1`       | `riscv32imac-unknown-none-elf`     | CH32L103                   |
| `algos/h4`   | `flash_h4`       | `riscv32imac-unknown-none-elf`     | CH32H415/H416/H417         |
| `algos/f1`          | `flash_v1`       | `thumbv7m-none-eabi`               | CH32F103 (Cortex-M3)       |
| `algos-ch5xx/ch5xx` | `FLASH_ROM`      | `riscv32imac-unknown-none-elf`     | CH570/571/572/573, CH581/582/583/584/585, CH591/592 (PIC CodeFlash + DataFlash) |
| `algos-ch5xx/ch569` | `FLASH_ROM`      | `riscv32imac-unknown-none-elf`     | CH565/CH569 (CodeFlash + DataFlash) |
| `algos-ch5xx/ch564` | `FLASH_ROM`      | `riscv32imac-unknown-none-elf`     | CH564 (192K/448K configurations, CodeFlash + DataFlash) |
| `algos-ch5xx/ch595` | `FLASH_ROM`      | `riscv32imac-unknown-none-elf`     | CH595, CH596 (page-latch CodeFlash) |

Each `algos/*` crate builds three binaries — `usr`, `sys`, `ob` — one per writable
region (CH32 naming). VND (vendor / ESIG) is read-only and not programmed.

`algos-ch5xx/ch5xx` is a standalone workspace crate. Its binaries and the
probe-rs regions are named after the datasheet's CODE-area map:
`CodeFlash` / `DataFlash` / `InfoFlash` — not the CH32
`USR`/`SYS`/`OPT` names. `InfoFlash` is a read-only region; `BootLoader` is not
mapped.

| Binary      | Region      | Frame address   | Chips                            |
| ----------- | ----------- | --------------- | -------------------------------- |
| `code`      | `CodeFlash` | absolute        | CH570/571/572/573, CH581/582/583/584/585, CH591/592 |
| `code-584x` | `CodeFlash` | `addr + 0x80000` | CH584X                           |
| `data`      | `DataFlash` | `0x70000 + off` | CH571/CH573, CH581/CH584/CH585   |
| `data-bank` | `DataFlash` | `0xF0000 + off` | CH582/CH583, CH591/CH592         |
| `data-584x` | `DataFlash` | 0-based engine  | CH584X                           |

`code` is position-independent and shared across the family; it enforces the
per-chip CodeFlash ceiling dynamically via `R8_CHIP_ID` (`0x40001041`).
`code-584x` is separate because CH584X shares `R8_CHIP_ID` (`0x92`) with CH584
but has 480 KiB of CodeFlash at a different engine frame address
(`addr + 0x80000`); it verifies `(*(uint32_t *)0x7F024 & 0x0F) == 0x08` and
uses ISP585's conservative 448 KiB `RB_BOOT_LOADER` limit when
`R8_GLOB_CFG_INFO` bit 5 is set.
The DataFlash protocol is not a CodeFlash alias (256-byte `0x81` erase,
byte-wise `0x02` program, `0x0B` engine reads).

`data` and `data-bank` share `src/data_flash.rs` and differ only in the
`0x80000` bank bit. `data-584x` uses its own `src/data_flash_584x.rs`: CH584X
addresses its 512 KiB DataFlash 0-based in the engine and erases in 4 KiB
blocks, while probe-rs exposes it at the program-view base `0x80000`.

The binaries are selected statically in the probe-rs target YAML because
`R8_CHIP_ID` collides between the generations: `0x92` covers CH581 and CH584
(no bank bit) as well as CH592 (bank bit). CH584X also shares `0x92` and the
WCH-Link device id with CH584, so `--chip CH584X` is required; both 584X
algorithms verify `(*(uint32_t *)0x7F024 & 0x0F) == 0x08`. CH570/CH572 have
no DataFlash.

CH595/CH596 use a newer hardware pre-program/erase/page-latch Flash controller
at `0x40001800..0x40001820` (`CH59x_flash.c`) rather than the SPI-like
`0x40001800..0x40001806` interface, so they live in their own crate. CH586/CH587
have up to 960 KiB CodeFlash, and CH577/CH578/CH579 are Cortex-M0 parts; none
of them are covered by `algos-ch5xx/ch5xx`.

`algos-ch5xx/ch569` covers CH565/CH569 CodeFlash (448 KiB) and DataFlash
(32 KiB at `0x70000`), both in 4 KiB sectors. Its `usr` and `data` binaries
share the SYS-block flash window (`0x40001014`/`18`/`1a`), start commands with
`CTRL=7` rather than the CH57x/CH58x/CH59x `CTRL=5`, and are linked into RAMX
at `0x20020008`.

`algos-ch5xx/ch564` covers CH564's 192 KiB and 448 KiB CodeFlash layouts plus
the 32 KiB DataFlash at `0x30000`/`0x70000`. CH564's controller is at
`0x40400018`/`0x4040001a`: erase/program frames use `CTRL=3` with a `6` prefix,
reads use `CTRL=7`, and both modes add an 18-NOP delay after each access. The
algorithms read InfoFlash `0x3007` at init to pick the memory configuration, so
the same binaries serve both `CH564` and `CH564-512K`.

`algos-ch5xx/ch595` covers CH595/CH596: 240 KiB CodeFlash at `0`, 2 KiB
sectors, 256-byte page latch, and no user DataFlash. Programming is a hardware
state machine (`CH59x_flash.c`): pre-program, erase, clear the page latch, load
256 bytes through `R32_FLASH_DATA`, then start page program, polling
`R32_FLASH_STATUS` bit 31/bit 19 for busy/ready. The safe-access signature opens
`R32_GLOBAL_CONFIG = 0xE1` for the whole operation. The two parts have different
`R8_CHIP_ID` values (`0x97`/`0x96`) but share the WCH-Link chip id `0xcb`, so
probe-rs auto-selects `CH595`; use `--chip CH596` for CH596.

## Repo layout

`ch32-metapac` isn't pinned via crates.io or git rev — it's built from a
[`ch32-data`](https://github.com/ch32-rs/ch32-data) submodule and consumed
through a `path` dep. The submodule sha is the single source of truth for
both the chip JSON xtask reads and the PAC the algos link against.

```
flash-algorithms/
├── ch32-data/                  # submodule — pinned sha
│   └── build/                  # generated by `xtask --bootstrap-only` (gitignored)
│       ├── ch32-metapac/       # ← algo path dep target
│       └── data/chips/*.json   # ← xtask reads these
├── algos/{common,v0,v00x,v1,v3,v4x7,l1,x0,h4,f1}/
├── algos-ch5xx/{ch5xx,ch564,ch569,ch595}/  # standalone CH5xx flash algorithms
└── xtask/                      # standalone crate, NOT in the algos workspace
```

`xtask` lives outside the algos workspace because Cargo validates every
workspace member's manifest on any workspace command — including the
ch32-metapac path dep that doesn't exist until xtask has run its bootstrap.

## Clone

```
git clone --recursive https://github.com/ch32-rs/flash-algorithms
```

If you forgot `--recursive`:

```
git submodule update --init
```

## Bootstrap

Generates `ch32-data/build/data/chips/*.json` and `ch32-data/build/ch32-metapac/`.
Required once after a fresh clone or after bumping the submodule sha; safe to
re-run (no-op if outputs are already present):

```
cargo run --manifest-path xtask/Cargo.toml -- --bootstrap-only
```

## Building algos

Each algo crate's `.cargo/config.toml` pins its target triple, so cargo only
picks it up when run from inside the crate:

```
cd algos/v0
cargo build --release
```

## Generating target YAMLs

Runs the bootstrap (if needed), builds every algo, walks
`ch32-data/build/data/chips/`, and emits one probe-rs YAML per
(chip × memory_option) into `generated/` (gitignored):

```
cargo run --manifest-path xtask/Cargo.toml
```

## Bumping ch32-data

```
cd ch32-data && git fetch && git checkout <new-sha> && cd ..
rm -rf ch32-data/build
cargo run --manifest-path xtask/Cargo.toml
git add ch32-data && git commit -m "bump ch32-data to <new-sha>"
```

## Formatting

`cargo fmt --all` from the repo root reaches into the generated
`ch32-data/build/ch32-metapac/` path dep and complains about code we don't
own. Mirror CI instead:

```
find algos algos-ch5xx xtask -name '*.rs' -not -path '*/target/*' -print0 \
  | xargs -0 rustfmt --edition 2024
```

## License

Dual-licensed under MIT or Apache-2.0 at your option.
