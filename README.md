# ch32-rs flash-algorithms

[probe-rs](https://probe.rs) flash algorithms and target YAMLs for WCH's CH32
RISC-V MCU families, built on top of [ch32-metapac](https://github.com/ch32-rs/ch32-metapac).

## Coverage

One algo crate per flash IP version.

| Crate        | Flash peripheral | Target triple                      | Chip families              |
| ------------ | ---------------- | ---------------------------------- | -------------------------- |
| `algos/v0`   | `flash_v0`       | `riscv32ec-unknown-none-elf`       | CH32V003, CH641            |
| `algos/v00x` | `flash_v00x`     | `riscv32ec_zmmul-unknown-none-elf` | CH32V002/4/5/6/7, CH32M007 |
| `algos/v1`   | `flash_v1`       | `riscv32imac-unknown-none-elf`     | CH32V103                   |
| `algos/v3`   | `flash_v3`       | `riscv32imac-unknown-none-elf`     | CH32V2xx, CH32V3xx         |
| `algos/x0`   | `flash_x0`       | `riscv32imac-unknown-none-elf`     | CH32X035/X033, CH643       |
| `algos/l1`   | `flash_l1`       | `riscv32imac-unknown-none-elf`     | CH32L103                   |

Each crate builds three binaries — `usr`, `sys`, `ob` — one per writable
region. VND (vendor / ESIG) is read-only and not programmed.

## Building

Each algo crate's `.cargo/config.toml` pins its target triple, so cargo only
picks it up when run from inside the crate:

```
cd algos/v0
cargo build --release
```

## Generating target YAMLs

```
cargo run -p xtask
```

Walks `../ch32-data/build/data/chips/`, builds every algo crate, emits one
probe-rs YAML per (chip × memory_option) into `generated/` (gitignored).

## License

Dual-licensed under MIT or Apache-2.0 at your option.
