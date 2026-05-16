mod algo;
mod chip;
mod render;

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn ch32_data_chip_dir() -> PathBuf {
    workspace_root().join("../ch32-data/build/data/chips")
}

fn output_dir() -> PathBuf {
    workspace_root().join("generated")
}

fn main() -> Result<()> {
    let root = workspace_root();
    let out = output_dir();
    std::fs::create_dir_all(&out)?;

    eprintln!("==> building algo crates");
    let algos = algo::build_all(&root)?;
    eprintln!("    {} algo blobs built", algos.len());

    let chip_dir = ch32_data_chip_dir();
    if !chip_dir.exists() {
        bail!(
            "ch32-data chip JSON dir not found at {} — run `cd ../ch32-data && ./d gen` first",
            chip_dir.display(),
        );
    }

    eprintln!("==> reading chip JSONs from {}", chip_dir.display());
    let chips = chip::load_all(&chip_dir).context("loading chip JSONs")?;
    eprintln!("    {} chips parsed", chips.len());

    eprintln!("==> emitting target YAMLs to {}", out.display());
    let stats = render::emit_all(&chips, &algos, &out)?;
    eprintln!(
        "==> done: {} family YAMLs written ({} variants total), {} chips skipped",
        stats.families_written, stats.variants_written, stats.chips_skipped,
    );
    Ok(())
}
