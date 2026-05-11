# ch32-rs flash-algorithms

[probe-rs](https://probe.rs) flash algorithms and target YAMLs for WCH's CH32
RISC-V MCU families, built on top of [ch32-metapac](https://github.com/ch32-rs/ch32-metapac).

## Coverage

One algorithm crate per flash-peripheral version, covering every chip currently
modelled in ch32-metapac:

| Crate | Flash peripheral | Core | Target triple | Chip families |
| --- | --- | --- | --- | --- |
| `algos/v0`   | `flash_v0`   | qingke-v2a  | `riscv32ec-unknown-none-elf`    | CH32V003, CH641 |
| `algos/v00x` | `flash_v00x` | qingke-v2c  | `riscv32ec-unknown-none-elf`    | CH32V002/4/5/6/7, CH32M007 |
| `algos/v1`   | `flash_v1`   | qingke-v3   | `riscv32imac-unknown-none-elf`  | CH32V103 |
| `algos/v2`   | `flash_v3`   | qingke-v4b/c| `riscv32imac-unknown-none-elf`  | CH32V2 series |
| `algos/v3`   | `flash_v3`   | qingke-v4f  | `riscv32imacf-unknown-none-elf` | CH32V3 series |
| `algos/x0`   | `flash_x0`   | qingke-v4c  | `riscv32imac-unknown-none-elf`  | CH32X0, CH643 |
| `algos/l1`   | `flash_l1`   | qingke-v4c  | `riscv32imac-unknown-none-elf`  | CH32L103 |

Each crate produces three flash-algorithm binaries:

- `usr` — user flash region
- `sys` — boot/system flash region
- `opt` — option bytes region (writable as free storage)

A fourth read-only region per chip — `vnd` — covers ESIG (UID + flash
capacity register) and any factory-locked vendor/manufacturer configuration
word. Exposed in the generated YAML as read-only memory so probe-rs can read
it but won't try to program or erase.

## Generating target YAMLs

The `xtask` host tool builds every algorithm binary, then walks the chip list
from ch32-metapac (merged with the per-family overrides in `chips.yaml`) and
emits one probe-rs target YAML per chip variant into `generated/`.

```
cargo run -p xtask
```

The output directory is gitignored — regenerate locally or pull from a release
build. PRs to [probe-rs](https://github.com/probe-rs/probe-rs) replace the
existing `CH32V0_Series.yaml` / `CH32V2_Series.yaml` / `CH32V3_Series.yaml` /
`CH32F1_Series.yaml` files with the per-variant outputs.

## License

Dual-licensed under MIT or Apache-2.0 at your option.
