//! CodeFlash algorithm for CH564.
//!
//! Supports both the 192 KiB and 448 KiB memory configurations; the config
//! byte at InfoFlash `0x3007` is read at init time.

#![no_std]
#![no_main]

#[path = "../flash_rom.rs"]
mod flash_rom;

#[path = "../usr_flash.rs"]
mod usr_flash;
