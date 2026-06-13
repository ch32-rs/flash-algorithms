mod algo;
mod chip;
mod render;

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn ch32_data_dir() -> PathBuf {
    workspace_root().join("ch32-data")
}

fn ch32_data_chip_dir() -> PathBuf {
    ch32_data_dir().join("build/data/chips")
}

fn output_dir() -> PathBuf {
    workspace_root().join("generated")
}

fn run_cargo(dir: &Path, args: &[&str]) -> Result<()> {
    eprintln!("    cargo {} (in {})", args.join(" "), dir.display());
    let status = Command::new("cargo")
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("failed to spawn cargo {:?}", args))?;
    if !status.success() {
        bail!("cargo {:?} failed in {}", args, dir.display());
    }
    Ok(())
}

/// Populate `ch32-data/build/{data,ch32-metapac}/` if missing — the algo crates' path
/// dep on `ch32-data/build/ch32-metapac` won't resolve until this runs.
fn ensure_ch32_data_build(root: &Path) -> Result<()> {
    let submodule = root.join("ch32-data");
    if !submodule.join("Cargo.toml").exists() {
        bail!(
            "ch32-data submodule not initialized at {} — run `git submodule update --init`",
            submodule.display(),
        );
    }
    let json_marker = submodule.join("build/data/chips");
    let metapac_marker = submodule.join("build/ch32-metapac/Cargo.toml");
    if json_marker.exists() && metapac_marker.exists() {
        return Ok(());
    }
    eprintln!("==> bootstrapping ch32-data/build");
    if !json_marker.exists() {
        run_cargo(&submodule, &["run", "-p", "ch32-data-gen"])?;
    }
    if !metapac_marker.exists() {
        // Same chip pattern as ch32-data's `./d gen` target.
        run_cargo(
            &submodule,
            &[
                "run",
                "-p",
                "ch32-metapac-gen",
                "--",
                "CH32X03*",
                "CH32V*",
                "CH32L*",
                "CH32M*",
                "CH32F*",
                "CH32H*",
                "CH641",
                "CH643",
            ],
        )?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let root = workspace_root();
    let bootstrap_only = std::env::args().any(|a| a == "--bootstrap-only");

    ensure_ch32_data_build(&root)?;
    if bootstrap_only {
        return Ok(());
    }

    let out = output_dir();
    std::fs::create_dir_all(&out)?;

    eprintln!("==> building algo crates");
    let algos = algo::build_all(&root)?;
    eprintln!("    {} algo blobs built", algos.len());

    let chip_dir = ch32_data_chip_dir();
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
