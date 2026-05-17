use crate::algo::AlgoBlob;
use crate::chip::{Access, Chip, MemoryRegion, Variant};
use anyhow::{Context, Result, anyhow, bail};
use probe_rs_target::{
    ArmCoreAccessOptions, Chip as PrChip, ChipFamily, Core as PrCore, CoreAccessOptions, CoreType,
    MemoryAccess, MemoryRegion as PrMemoryRegion, NvmRegion, RamRegion, RiscvCoreAccessOptions,
    SectorDescription, TargetDescriptionSource,
    chip_detection::{ChipDetectionMethod, ObRefinement, WchLinkDetection},
};
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::Path;

#[derive(Default)]
pub struct EmitStats {
    pub families_written: usize,
    pub variants_written: usize,
    pub chips_skipped: usize,
}

/// Group chips by (silicon, arch) and emit one YAML per group. Splitting by
/// arch separates `flash_v1` consumers into V1 (RISC-V) and F1 (Cortex-M3)
/// families, each with the right CPU-built algo blob.
pub fn emit_all(chips: &[Chip], algos: &[AlgoBlob], out_dir: &Path) -> Result<EmitStats> {
    let mut groups: BTreeMap<(String, String), Vec<&Chip>> = BTreeMap::new();
    let mut stats = EmitStats::default();

    for chip in chips {
        match (chip.flash_version(), chip.arch()) {
            (Some(silicon), Some(arch)) => {
                groups
                    .entry((silicon.to_string(), arch.to_string()))
                    .or_default()
                    .push(chip);
            }
            (None, _) => {
                eprintln!("    skip {} (no FLASH peripheral version)", chip.name);
                stats.chips_skipped += 1;
            }
            (_, None) => {
                eprintln!("    skip {} (no `arch` on first core)", chip.name);
                stats.chips_skipped += 1;
            }
        }
    }

    for ((silicon, arch), silicon_chips) in groups {
        if !algos.iter().any(|a| a.silicon == silicon && a.arch == arch) {
            eprintln!(
                "    skip silicon {}/{} ({} chips: no algo blob built)",
                silicon,
                arch,
                silicon_chips.len()
            );
            stats.chips_skipped += silicon_chips.len();
            continue;
        }

        // Roll up marketing-series prefixes (e.g. v0 → "CH32V003_CH641").
        // A `series:` field in ch32-data would replace `series_for`.
        let mut prefixes: Vec<String> = silicon_chips.iter().map(|c| series_for(&c.name)).collect();
        prefixes.sort();
        prefixes.dedup();
        let joined = prefixes.join("_");
        let display_name = format!("{} Series", joined.replace('_', " / "));
        let file_stem = format!("{}_Series", joined);

        let (chip_family, variant_count) =
            build_family(&display_name, &silicon_chips, &silicon, &arch, algos)?;
        let yaml = serialize_yaml(&chip_family)?;
        let path = out_dir.join(format!("{}.yaml", file_stem));
        std::fs::write(&path, yaml).with_context(|| format!("writing {}", path.display()))?;
        eprintln!(
            "    wrote {} ({} variants, silicon {}/{})",
            path.file_name().unwrap().to_string_lossy(),
            variant_count,
            silicon,
            arch,
        );
        stats.families_written += 1;
        stats.variants_written += variant_count;
    }

    Ok(stats)
}

/// Heuristic — ch32-data has no explicit `series` field yet.
fn series_for(chip_name: &str) -> String {
    // V003 is silicon v0 alone; all other V0xx + M0xx are silicon v00x.
    if chip_name.starts_with("CH32V003") {
        return "CH32V003".to_string();
    }
    if chip_name.starts_with("CH32V0") || chip_name.starts_with("CH32M0") {
        return "CH32V00X".to_string();
    }
    if let Some(rest) = chip_name.strip_prefix("CH32") {
        let prefix: String = rest.chars().take(2).collect();
        if prefix.len() == 2 {
            return format!("CH32{}", prefix);
        }
    }
    if chip_name.starts_with("CH6") {
        let prefix: String = chip_name.chars().take(5).collect();
        if prefix.len() == 5 {
            return prefix;
        }
    }
    chip_name.to_string()
}

fn build_family(
    display_name: &str,
    chips: &[&Chip],
    silicon: &str,
    arch: &str,
    algos: &[AlgoBlob],
) -> Result<(ChipFamily, usize)> {
    let core_name = "main".to_string();
    let (core_type, core_access_options) = match arch {
        "riscv" => (
            CoreType::Riscv,
            CoreAccessOptions::Riscv(RiscvCoreAccessOptions {
                hart_id: Some(0),
                jtag_tap: None,
                mem_ap: None,
            }),
        ),
        "arm" => (
            CoreType::Armv7m,
            CoreAccessOptions::Arm(ArmCoreAccessOptions::default()),
        ),
        other => bail!("unsupported arch `{}`", other),
    };
    let core = PrCore {
        name: core_name.clone(),
        core_type,
        core_access_options,
    };

    let mut variants: Vec<PrChip> = Vec::new();
    let mut variant_algo_uses: BTreeMap<String, Vec<Range<u64>>> = BTreeMap::new();
    let mut algo_kind: BTreeMap<String, String> = BTreeMap::new();
    let mut detection_entries: BTreeMap<u32, DetectionGroup> = BTreeMap::new();

    for chip in chips {
        let default_opt = chip
            .memory_ram_code_config
            .as_ref()
            .map(|c| c.default.clone());

        let refinement = build_ob_refinement(chip);

        for variant in chip.variants() {
            let is_default_variant = match (&variant.option, &default_opt) {
                (Some(opt), Some(def)) => opt == def,
                _ => true,
            };
            let suffix = if is_default_variant {
                String::new()
            } else {
                format!("_{}", variant.option.as_deref().unwrap_or(""))
            };
            let target_name = format!("{}{}", chip.name, suffix);

            let (memory_map, variant_algo_names) = build_variant(
                &variant,
                &core_name,
                silicon,
                arch,
                algos,
                &mut variant_algo_uses,
                &mut algo_kind,
            );

            // Non-default splits share the same chip_id; OB refinement maps
            // OB.USER bits 5-7 to the right variant post-attach.
            if is_default_variant && let Some(id) = chip.device_id {
                let mask = mask_for(id);
                let group = detection_entries.entry(mask).or_default();
                group.variants.insert(id & mask, target_name.clone());
                if let Some(r) = &refinement {
                    group.ob_refinement.insert(id & mask, r.clone());
                }
            }

            variants.push(PrChip {
                name: target_name,
                part: None,
                svd: None,
                documentation: Default::default(),
                package_variants: vec![],
                cores: vec![core.clone()],
                memory_map,
                flash_algorithms: variant_algo_names,
                rtt_scan_ranges: None,
                jtag: None,
                default_binary_format: None,
            });
        }
    }

    let chip_detection: Vec<ChipDetectionMethod> = detection_entries
        .into_iter()
        .map(|(mask, group)| {
            ChipDetectionMethod::WchLink(WchLinkDetection {
                mask,
                variants: group.variants,
                ob_refinement: group.ob_refinement,
            })
        })
        .collect();

    let mut flash_algorithms = Vec::new();
    for (algo_name, ranges) in variant_algo_uses {
        let kind = algo_kind
            .get(&algo_name)
            .ok_or_else(|| anyhow!("missing kind for algo {}", algo_name))?;
        let start = ranges.iter().map(|r| r.start).min().unwrap();
        let end = ranges.iter().map(|r| r.end).max().unwrap();
        let blob_kind = if kind == "usr-legacy" { "usr" } else { kind };
        // Prefer Fast over Standard (v0/v1 OB only has Standard).
        let template_region = find_template_region(chips, blob_kind)?;
        let (page_size, _) = template_region
            .modes
            .iter()
            .find_map(|m| m.fast().or_else(|| m.standard()))
            .ok_or_else(|| {
                anyhow!(
                    "no programming mode on region {} (silicon {})",
                    template_region.name,
                    silicon
                )
            })?;

        let blob = algos
            .iter()
            .find(|a| a.silicon == silicon && a.arch == arch && a.region_kind == blob_kind)
            .ok_or_else(|| {
                anyhow!(
                    "no algo blob for silicon {} arch {} kind {}",
                    silicon,
                    arch,
                    kind
                )
            })?;
        let mut algo = blob.template.clone();
        algo.name = algo_name.clone();
        algo.description = algo_name.clone();
        algo.default = true;
        algo.flash_properties.address_range = start..end;
        algo.flash_properties.page_size = page_size;
        algo.flash_properties.sectors = vec![SectorDescription {
            size: page_size as u64,
            address: 0,
        }];
        // Drop EraseChip for SYS — defends against accidental bulk-erase
        // routing. USR's MER and OB's page-erase+defaults are fine.
        if kind == "sys" {
            algo.pc_erase_all = None;
        }
        flash_algorithms.push(algo);
    }

    let variant_count = variants.len();
    Ok((
        ChipFamily {
            name: display_name.to_string(),
            manufacturer: None,
            chip_detection,
            generated_from_pack: false,
            pack_file_release: None,
            variants,
            flash_algorithms,
            source: TargetDescriptionSource::External,
        },
        variant_count,
    ))
}

fn build_variant(
    variant: &Variant,
    core_name: &str,
    silicon: &str,
    arch: &str,
    algos: &[AlgoBlob],
    algo_uses: &mut BTreeMap<String, Vec<Range<u64>>>,
    algo_kind: &mut BTreeMap<String, String>,
) -> (Vec<PrMemoryRegion>, Vec<String>) {
    let mut memory_map = Vec::new();
    let mut variant_algos = Vec::new();
    let regions = merge_regions(&variant.memory);
    for region in &regions {
        let access = MemoryAccess {
            read: region.access.as_ref().is_none_or(|a| a.read),
            write: region.access.as_ref().is_none_or(|a| a.write),
            execute: region.access.as_ref().is_none_or(|a| a.execute),
            boot: false,
        };
        let range = region.address..(region.address + region.size);
        match region.kind.as_str() {
            "ram" => {
                memory_map.push(PrMemoryRegion::Ram(RamRegion {
                    name: Some(region.name.clone()),
                    range,
                    cores: vec![core_name.to_string()],
                    is_alias: false,
                    access: Some(access),
                }));
            }
            "flash" => {
                // `is_alias` = "skip in `probe-rs erase` bulk path". Only erase USR_*
                let is_alias = !region.name.starts_with("USR");
                memory_map.push(PrMemoryRegion::Nvm(NvmRegion {
                    name: Some(region.name.clone()),
                    range: range.clone(),
                    cores: vec![core_name.to_string()],
                    is_alias,
                    access: Some(access),
                }));
                if access.write
                    && let Some(kind) = region_kind(&region.name)
                    && algos
                        .iter()
                        .any(|a| a.silicon == silicon && a.arch == arch && a.region_kind == kind)
                {
                    // Name by kind so split-SYS chips (v1: SYS_1 +
                    // SYS_2) share one entry instead of two copies.
                    let suffix = match kind {
                        "ob" => "opt",
                        other => other,
                    };
                    let algo_name = format!("ch32-{}-{}", silicon, suffix);
                    if !variant_algos.contains(&algo_name) {
                        variant_algos.push(algo_name.clone());
                    }
                    algo_uses
                        .entry(algo_name.clone())
                        .or_default()
                        .push(range.clone());
                    algo_kind.insert(algo_name, kind.to_string());

                    if kind == "usr" {
                        let alias_range = 0u64..(range.end - range.start);
                        memory_map.push(PrMemoryRegion::Nvm(NvmRegion {
                            name: Some("USR_LEGACY".to_string()),
                            range: alias_range.clone(),
                            cores: vec![core_name.to_string()],
                            is_alias: true,
                            access: Some(access),
                        }));
                        let legacy_name = format!("ch32-{}-usr-legacy", silicon);
                        if !variant_algos.contains(&legacy_name) {
                            variant_algos.push(legacy_name.clone());
                        }
                        algo_uses
                            .entry(legacy_name.clone())
                            .or_default()
                            .push(alias_range);
                        algo_kind.insert(legacy_name, "usr-legacy".to_string());
                    }
                }
            }
            _ => {}
        }
    }
    (memory_map, variant_algos)
}

fn region_kind(name: &str) -> Option<&'static str> {
    match name {
        n if n.starts_with("USR") => Some("usr"),
        n if n.starts_with("SYS") => Some("sys"),
        "OPT" => Some("ob"),
        _ => None,
    }
}

fn find_template_region(chips: &[&Chip], kind: &str) -> Result<MemoryRegion> {
    for chip in chips {
        for variant in chip.variants() {
            for region in &variant.memory {
                if region.kind == "flash" && region_kind(&region.name) == Some(kind) {
                    return Ok(region.clone());
                }
            }
        }
    }
    Err(anyhow!("no region of kind {} in family", kind))
}

/// CH643 (0x643xxxxx) needs exact match per wlink's `chips.rs`; everyone
/// else clears the package nibble.
fn mask_for(chip_id: u32) -> u32 {
    match chip_id & 0xFFF0_0000 {
        0x6430_0000 => 0xFFFF_FFFF,
        _ => 0xFFFF_FF0F,
    }
}

#[derive(Default)]
struct DetectionGroup {
    variants: indexmap::IndexMap<u32, String>,
    ob_refinement: indexmap::IndexMap<u32, ObRefinement>,
}

const OB_USER_ADDRESS: u64 = 0x1FFFF802;
const RAM_CODE_MASK: u8 = 0xE0;

fn build_ob_refinement(chip: &Chip) -> Option<ObRefinement> {
    let cfg = chip.memory_ram_code_config.as_ref()?;
    let ob_version = chip.ob_version()?;
    let encoding = ram_code_encoding(ob_version)?;

    let default_opt = &cfg.default;
    let mut variants: indexmap::IndexMap<u8, String> = indexmap::IndexMap::new();
    for raw in 0u8..8 {
        let option_name = encoding(raw)?;
        let suffix = if option_name == default_opt {
            String::new()
        } else {
            format!("_{}", option_name)
        };
        let variant_name = format!("{}{}", chip.name, suffix);
        variants.insert(raw << 5, variant_name);
    }

    Some(ObRefinement {
        address: OB_USER_ADDRESS,
        mask: RAM_CODE_MASK,
        variants,
    })
}

/// Encoding follows `ch32-data/data/nv/ob_v{2,3}_ram_code.yaml`.
fn ram_code_encoding(ob_version: &str) -> Option<fn(u8) -> Option<&'static str>> {
    match ob_version {
        "v2_ram_code" => Some(|raw| match raw {
            0b000 | 0b001 => Some("c128_r64"),
            0b010 | 0b011 => Some("c144_r48"),
            0b100 | 0b101 => Some("c160_r32"),
            // 0b11x reserved on v2; fall back to factory default.
            0b110 | 0b111 => Some("c128_r64"),
            _ => None,
        }),
        "v3_ram_code" => Some(|raw| match raw {
            0b000 | 0b001 => Some("c192_r128"),
            0b010 | 0b011 => Some("c224_r96"),
            0b100 | 0b101 => Some("c256_r64"),
            0b110 => Some("c128_r192"),
            0b111 => Some("c288_r32"),
            _ => None,
        }),
        _ => None,
    }
}

/// Collapse contiguous `USR_1`+`USR_2` runs into a single `USR` — the V2/V3
/// split is an OB-configurable line, not a HW boundary.
fn merge_regions(memory: &[MemoryRegion]) -> Vec<MemoryRegion> {
    let mut sorted: Vec<MemoryRegion> = memory.to_vec();
    sorted.sort_by_key(|r| r.address);

    let mut merged: Vec<MemoryRegion> = Vec::with_capacity(sorted.len());
    for r in sorted {
        let mergeable = merged.last().is_some_and(|last| can_merge(last, &r));
        if mergeable {
            merged.last_mut().unwrap().size += r.size;
        } else {
            merged.push(r);
        }
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in &merged {
        let base = base_name(&r.name).to_string();
        *counts.entry(base).or_insert(0) += 1;
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    for r in &mut merged {
        let base = base_name(&r.name).to_string();
        if counts[&base] <= 1 {
            r.name = base;
        } else {
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            r.name = format!("{}_{}", base, n);
        }
    }
    merged
}

fn can_merge(a: &MemoryRegion, b: &MemoryRegion) -> bool {
    let (a_base, a_idx) = split_name(&a.name);
    let (b_base, b_idx) = split_name(&b.name);
    a_idx.is_some()
        && b_idx.is_some()
        && a_base == b_base
        && a.kind == b.kind
        && a.address + a.size == b.address
        && access_key(a.access.as_ref()) == access_key(b.access.as_ref())
}

fn split_name(name: &str) -> (&str, Option<u32>) {
    if let Some((base, idx)) = name.rsplit_once('_')
        && let Ok(n) = idx.parse::<u32>()
    {
        return (base, Some(n));
    }
    (name, None)
}

fn base_name(name: &str) -> &str {
    split_name(name).0
}

fn access_key(a: Option<&Access>) -> Option<(bool, bool, bool)> {
    a.map(|x| (x.read, x.write, x.execute))
}

/// `probe-rs-target` serializes hex as `'0x…'` but deserializes raw numbers
/// — strip the quotes to round-trip (same workaround as probe-rs target-gen).
fn serialize_yaml(family: &ChipFamily) -> Result<String> {
    let raw = serde_yaml::to_string(family)?;
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        let needs_unquote = (line.contains("'0x") || line.contains("'0X"))
            && (line.ends_with('\'') || line.contains("':"));
        if needs_unquote {
            out.push_str(&line.replace('\'', ""));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    Ok(out)
}
