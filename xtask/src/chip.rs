use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Chip {
    pub name: String,
    pub memory: Vec<MemoryRegion>,
    pub cores: Vec<Core>,
    #[serde(default)]
    pub memory_ram_code_config: Option<RamCodeConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryRegion {
    pub name: String,
    pub kind: String,
    pub address: u64,
    pub size: u64,
    #[serde(default)]
    pub modes: Vec<Mode>,
    #[serde(default)]
    pub access: Option<Access>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mode {
    Fast { page_size: u32, load_size: u32 },
    Standard { erase_size: u32, write_size: u32 },
}

impl Mode {
    pub fn fast(&self) -> Option<(u32, u32)> {
        if let Mode::Fast {
            page_size,
            load_size,
        } = self
        {
            Some((*page_size, *load_size))
        } else {
            None
        }
    }
    pub fn standard(&self) -> Option<(u32, u32)> {
        if let Mode::Standard {
            erase_size,
            write_size,
        } = self
        {
            Some((*erase_size, *write_size))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Access {
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub write: bool,
    #[serde(default)]
    pub execute: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Core {
    #[serde(default)]
    pub peripherals: Vec<Peripheral>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Peripheral {
    pub name: String,
    #[serde(default)]
    pub registers: Option<PeripheralRegisters>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeripheralRegisters {
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RamCodeConfig {
    pub default: String,
    pub configs: Vec<RamCodeOption>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RamCodeOption {
    pub name: String,
    pub code: u64,
    pub ram: u64,
}

impl Chip {
    pub fn flash_version(&self) -> Option<&str> {
        for c in &self.cores {
            for p in &c.peripherals {
                if p.name == "FLASH"
                    && let Some(r) = &p.registers
                {
                    return Some(r.version.as_str());
                }
            }
        }
        None
    }

    pub fn variants(&self) -> Vec<Variant> {
        let Some(cfg) = &self.memory_ram_code_config else {
            return vec![Variant {
                option: None,
                memory: self.memory.clone(),
            }];
        };
        let usr1_default = self.memory.iter().find(|r| r.name == "USR_1");
        let usr2_default = self.memory.iter().find(|r| r.name == "USR_2");
        let total_flash = match (usr1_default, usr2_default) {
            (Some(u1), Some(u2)) => u1.size + u2.size,
            _ => return Vec::new(),
        };
        let usr1_addr = usr1_default.unwrap().address;

        cfg.configs
            .iter()
            .map(|opt| {
                let usr2_addr = usr1_addr + opt.code;
                let usr2_size = total_flash - opt.code;
                let mut memory = self.memory.clone();
                for r in &mut memory {
                    match r.name.as_str() {
                        "USR_1" => r.size = opt.code,
                        "USR_2" => {
                            r.address = usr2_addr;
                            r.size = usr2_size;
                        }
                        "RAM" => r.size = opt.ram,
                        _ => {}
                    }
                }
                Variant {
                    option: Some(opt.name.clone()),
                    memory,
                }
            })
            .collect()
    }
}

pub struct Variant {
    /// `None` when chip has no `memory_ram_code_config`; else the option name.
    pub option: Option<String>,
    pub memory: Vec<MemoryRegion>,
}

pub fn load_all(dir: &Path) -> Result<Vec<Chip>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let chip: Chip =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        out.push(chip);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}
