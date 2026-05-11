# ch32-rs flash-algorithms

[probe-rs](https://probe.rs) flash algorithms and target YAMLs for WCH's CH32
RISC-V MCU families, built on top of [ch32-metapac](https://github.com/ch32-rs/ch32-metapac).

## Status

`algos/v0` is the first implementation, covering CH32V003 / CH641. The other
families listed below are planned — `chips.yaml` already carries their region
maps, but the algo crates haven't been written. The `xtask` host tool that
turns the algo ELFs into per-chip probe-rs YAMLs is also still TODO.

## Coverage

One algorithm crate per flash-peripheral version. The target-triple column
matches the JSON spec under `targets/` that each crate's `.cargo/config.toml`
points at — these are the standard upstream `riscv32*` triples adjusted for
the WCH cores (ILP32E ABI for the qingke-v2 chips, plus a `+zmmul` variant
for v00x).

| Crate        | Status      | Flash peripheral | Core         | Target triple                        | Chip families              |
| ------------ | ----------- | ---------------- | ------------ | ------------------------------------ | -------------------------- |
| `algos/v0`   | implemented | `flash_v0`       | qingke-v2a   | `riscv32ec-unknown-none-elf`         | CH32V003, CH641            |
| `algos/v00x` | planned     | `flash_v00x`     | qingke-v2c   | `riscv32ec_zmmul-unknown-none-elf`   | CH32V002/4/5/6/7, CH32M007 |
| `algos/v1`   | planned     | `flash_v1`       | qingke-v3    | `riscv32imac-unknown-none-elf`       | CH32V103                   |
| `algos/v2`   | planned     | `flash_v3`       | qingke-v4b/c | `riscv32imac-unknown-none-elf`       | CH32V2 series              |
| `algos/v3`   | planned     | `flash_v3`       | qingke-v4f   | `riscv32imacf-unknown-none-elf`      | CH32V3 series              |
| `algos/x0`   | planned     | `flash_x0`       | qingke-v4c   | `riscv32imac-unknown-none-elf`       | CH32X0, CH643              |
| `algos/l1`   | planned     | `flash_l1`       | qingke-v4c   | `riscv32imac-unknown-none-elf`       | CH32L103                   |

Each crate produces three flash-algorithm binaries:

- `usr` — user flash region
- `sys` — boot/system flash region
- `opt` — option bytes region (writable as free storage)

A fourth read-only region per chip — `vnd` — covers ESIG (UID + flash
capacity register) and any factory-locked vendor/manufacturer configuration
word. The xtask will emit it as read-only memory in the generated YAML so
probe-rs can read it but won't try to program or erase.

## Building an algo crate

Each algo crate carries its own `.cargo/config.toml` that pins the target
triple and the algorithm-placement address. Cargo only picks that up when run
from inside the crate, so build from the crate directory:

```
cd algos/v0
cargo build --release
```

This produces `target/<triple>/release/{usr,sys,opt}` — three flash-algorithm
ELFs ready to drop into a probe-rs target description.

## Generating target YAMLs (planned)

The `xtask` host tool will build every algorithm binary, then walk the chip
list from ch32-metapac (merged with the per-family overrides in `chips.yaml`)
and emit one probe-rs target YAML per chip variant into `generated/`.

```
cargo run -p xtask
```

Not yet implemented — the `xtask/` directory is currently empty. Until it
lands, you can hand-author a single-chip YAML referencing the three ELFs
above to smoke-test on hardware.

The output directory is gitignored — regenerate locally or pull from a
release build. The intent is to publish the per-variant YAMLs to
[probe-rs](https://github.com/probe-rs/probe-rs) in place of the current
`CH32V0_Series.yaml` / `CH32V2_Series.yaml` / `CH32V3_Series.yaml` /
`CH32F1_Series.yaml` aggregates.

## License

Dual-licensed under MIT or Apache-2.0 at your option.
