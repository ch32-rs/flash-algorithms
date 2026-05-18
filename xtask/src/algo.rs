use anyhow::{Context, Result, anyhow, bail};
use probe_rs_target::{FlashProperties, RawFlashAlgorithm, SectorDescription};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub struct AlgoBlob {
    /// Flash IP version (matches `FLASH.registers.version`).
    pub silicon: String,
    pub arch: String,
    /// "usr" | "sys" | "ob".
    pub region_kind: String,
    pub template: RawFlashAlgorithm,
}

pub fn build_all(workspace_root: &Path) -> Result<Vec<AlgoBlob>> {
    let algos_dir = workspace_root.join("algos");
    let mut out = Vec::new();

    for entry in std::fs::read_dir(&algos_dir)? {
        let entry = entry?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "common" {
            continue;
        }
        out.extend(build_crate(&dir, &name)?);
    }

    if out.is_empty() {
        bail!("no algo crates found under algos/");
    }
    Ok(out)
}

fn build_crate(crate_dir: &Path, crate_name: &str) -> Result<Vec<AlgoBlob>> {
    eprintln!("    cargo build --release ({})", crate_dir.display());
    let status = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--release")
        .current_dir(crate_dir)
        .status()
        .with_context(|| format!("failed to spawn cargo for {}", crate_dir.display()))?;
    if !status.success() {
        bail!("cargo build failed in {}", crate_dir.display());
    }

    let triple = read_target_triple(crate_dir)?;
    let arch = arch_from_triple(&triple)
        .ok_or_else(|| anyhow!("unknown CPU arch in triple `{}`", triple))?;
    let silicon = silicon_for_crate(crate_name);
    let bin_dir = crate_dir.join("../../target").join(&triple).join("release");

    let mut out = Vec::new();
    let bin_src = crate_dir.join("src/bin");
    for entry in
        std::fs::read_dir(&bin_src).with_context(|| format!("reading {}", bin_src.display()))?
    {
        let entry = entry?;
        let stem = entry
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("bad bin filename"))?;
        let elf_path = bin_dir.join(&stem);
        let elf = std::fs::read(&elf_path)
            .with_context(|| format!("reading ELF {}", elf_path.display()))?;
        let template =
            extract_algo(&elf).with_context(|| format!("parsing ELF {}", elf_path.display()))?;
        out.push(AlgoBlob {
            silicon: silicon.to_string(),
            arch: arch.to_string(),
            region_kind: stem,
            template,
        });
    }
    Ok(out)
}

fn silicon_for_crate(name: &str) -> &str {
    match name {
        "f1" => "v1",
        n => n,
    }
}

fn arch_from_triple(triple: &str) -> Option<&'static str> {
    if triple.starts_with("riscv") {
        Some("riscv")
    } else if triple.starts_with("thumb") || triple.starts_with("arm") {
        Some("arm")
    } else {
        None
    }
}

fn read_target_triple(crate_dir: &Path) -> Result<String> {
    let cfg = crate_dir.join(".cargo/config.toml");
    let text =
        std::fs::read_to_string(&cfg).with_context(|| format!("reading {}", cfg.display()))?;
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("target") {
            let rest = rest.trim_start_matches([' ', '=']).trim();
            let raw = rest.trim_matches('"');
            if raw.is_empty() {
                continue;
            }
            // Custom JSON target specs build into a dir named after the
            // file stem, not the path literal.
            let triple = if raw.ends_with(".json") {
                Path::new(raw)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow!("bad target json path {raw} in {}", cfg.display()))?
                    .to_string()
            } else {
                raw.to_string()
            };
            return Ok(triple);
        }
    }
    bail!("no `target = \"…\"` in {}", cfg.display());
}

// ELF extraction ported from probe-rs target-gen (binary-only dependency).

const CODE_SECTION_KEY: (&str, u32) = ("PrgCode", goblin::elf64::section_header::SHT_PROGBITS);
const DATA_SECTION_KEY: (&str, u32) = ("PrgData", goblin::elf64::section_header::SHT_PROGBITS);
const BSS_SECTION_KEY: (&str, u32) = ("PrgData", goblin::elf64::section_header::SHT_NOBITS);

struct Section {
    start: u32,
    length: u32,
    data: Vec<u8>,
    load_address: u32,
}

struct AlgoElf {
    code: Section,
    data: Section,
    bss: Section,
}

impl AlgoElf {
    fn parse(elf: &goblin::elf::Elf<'_>, buf: &[u8]) -> Result<Self> {
        let mut code = None;
        let mut data = None;
        let mut bss = None;
        for ph in &elf.program_headers {
            if ph.p_type != goblin::elf::program_header::PT_LOAD || ph.p_memsz == 0 {
                continue;
            }
            let seg = ph.p_offset..ph.p_offset + ph.p_memsz;
            for sh in &elf.section_headers {
                let r = sh.sh_offset..sh.sh_offset + sh.sh_size;
                if seg.start <= r.start && r.end <= seg.end {
                    let bytes = if sh.sh_type == goblin::elf64::section_header::SHT_NOBITS {
                        Vec::new()
                    } else {
                        buf[sh.sh_offset as usize..][..sh.sh_size as usize].to_vec()
                    };
                    let s = Section {
                        start: sh.sh_addr as u32,
                        length: sh.sh_size as u32,
                        data: bytes,
                        load_address: (ph.p_vaddr + sh.sh_offset - ph.p_offset) as u32,
                    };
                    match (&elf.shdr_strtab[sh.sh_name], sh.sh_type) {
                        CODE_SECTION_KEY => code = Some(s),
                        DATA_SECTION_KEY => data = Some(s),
                        BSS_SECTION_KEY => bss = Some(s),
                        _ => {}
                    }
                }
            }
        }
        let code = code.ok_or_else(|| anyhow!("PrgCode section missing"))?;
        let data = data.unwrap_or_else(|| Section {
            start: code.start + code.length,
            length: 0,
            data: Vec::new(),
            load_address: code.load_address + code.length,
        });
        let bss = bss.unwrap_or_else(|| Section {
            start: data.start + data.length,
            length: 0,
            data: Vec::new(),
            load_address: data.load_address + data.length,
        });
        Ok(Self { code, data, bss })
    }

    fn blob(&self) -> Vec<u8> {
        let mut b =
            Vec::with_capacity((self.code.length + self.data.length + self.bss.length) as usize);
        b.extend(&self.code.data);
        b.extend(&self.data.data);
        b.extend(std::iter::repeat_n(0u8, self.bss.length as usize));
        b
    }
}

fn extract_algo(buf: &[u8]) -> Result<RawFlashAlgorithm> {
    let elf = goblin::elf::Elf::parse(buf)?;
    let parsed = AlgoElf::parse(&elf, buf)?;
    let dev = read_flash_device(&elf, buf)?;

    let mut algo = RawFlashAlgorithm::default();
    let code_off = parsed.code.start as u64;
    let mut syms: HashMap<&str, u64> = HashMap::new();
    for s in elf.syms.iter() {
        let name = &elf.strtab[s.st_name];
        if !name.is_empty() {
            syms.insert(name, s.st_value);
        }
    }
    let need = |k: &str| -> Result<u64> {
        syms.get(k)
            .copied()
            .map(|v| v - code_off)
            .ok_or_else(|| anyhow!("ELF missing required symbol {k}"))
    };
    algo.pc_init = Some(need("Init")?);
    algo.pc_uninit = syms.get("UnInit").copied().map(|v| v - code_off);
    algo.pc_program_page = need("ProgramPage")?;
    algo.pc_erase_sector = need("EraseSector")?;
    algo.pc_erase_all = syms.get("EraseChip").copied().map(|v| v - code_off);

    algo.instructions = parsed.blob();
    algo.load_address = Some(parsed.code.load_address as u64);
    algo.data_section_offset = (parsed.data.start - parsed.code.load_address) as u64;
    algo.big_endian = !elf.little_endian;

    algo.flash_properties = FlashProperties {
        address_range: dev.start..(dev.start + dev.size),
        page_size: dev.page_size,
        erased_byte_value: dev.erased,
        program_page_timeout: dev.program_to,
        erase_sector_timeout: dev.erase_to,
        sectors: dev
            .sectors
            .iter()
            .map(|(size, address)| SectorDescription {
                size: *size as u64,
                address: *address as u64,
            })
            .collect(),
    };
    // probe-rs schema default (Rust default for u64 is 0).
    algo.rtt_poll_interval = 20;
    Ok(algo)
}

struct FlashDevice {
    start: u64,
    size: u64,
    page_size: u32,
    erased: u8,
    program_to: u32,
    erase_to: u32,
    sectors: Vec<(u32, u32)>,
}

fn read_flash_device(elf: &goblin::elf::Elf<'_>, buf: &[u8]) -> Result<FlashDevice> {
    use scroll::Pread;
    let mut addr = None;
    let mut sym_size = 0u32;
    for s in elf.syms.iter() {
        if &elf.strtab[s.st_name] == "FlashDevice" {
            addr = Some(s.st_value as u32);
            sym_size = s.st_size as u32;
            break;
        }
    }
    let addr = addr.ok_or_else(|| anyhow!("ELF missing FlashDevice symbol"))?;

    let want = sym_size.max(160);
    let bytes = read_at(elf, buf, addr, want)
        .ok_or_else(|| anyhow!("FlashDevice not in any LOAD segment"))?;
    let mut sectors = Vec::new();
    let mut off = 160;
    while off + 8 <= bytes.len() {
        let size: u32 = bytes.pread(off).unwrap();
        let address: u32 = bytes.pread(off + 4).unwrap();
        if size == u32::MAX && address == u32::MAX {
            break;
        }
        sectors.push((size, address));
        off += 8;
    }
    Ok(FlashDevice {
        start: bytes.pread::<u32>(132).unwrap() as u64,
        size: bytes.pread::<u32>(136).unwrap() as u64,
        page_size: bytes.pread(140).unwrap(),
        erased: bytes.pread(148).unwrap(),
        program_to: bytes.pread(152).unwrap(),
        erase_to: bytes.pread(156).unwrap(),
        sectors,
    })
}

fn read_at<'a>(
    elf: &goblin::elf::Elf<'_>,
    buf: &'a [u8],
    addr: u32,
    size: u32,
) -> Option<&'a [u8]> {
    let want = addr as u64..(addr as u64 + size as u64);
    for ph in &elf.program_headers {
        let seg = ph.p_paddr..(ph.p_paddr + ph.p_memsz.min(ph.p_filesz));
        if seg.start <= want.start && want.end <= seg.end {
            let off = (ph.p_offset + addr as u64 - seg.start) as usize;
            return Some(&buf[off..][..size as usize]);
        }
    }
    None
}
