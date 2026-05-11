#![no_std]

pub use flash_algo_common::*;

include!(concat!(env!("OUT_DIR"), "/constants.rs"));

pub fn wait_busy() {
    while FLASH.statr().read().bsy() {}
    FLASH.statr().write(|w| w.set_eop(true));
}

pub fn page_erase(addr: u32) {
    FLASH.ctlr().write(|w| w.set_page_er(true));
    FLASH.addr().write(|w| w.set_addr(addr));
    FLASH.ctlr().write(|w| {
        w.set_page_er(true);
        w.set_strt(true);
    });
    wait_busy();
    FLASH.ctlr().write(|_| {});
}

/// `addr` 4-byte aligned, `data.len()` a multiple of 4, must not cross a page.
pub fn page_program(addr: u32, data: &[u8]) {
    let page_base = addr & !(PAGE_SIZE - 1);

    FLASH.ctlr().write(|w| w.set_page_pg(true));
    FLASH.ctlr().write(|w| {
        w.set_page_pg(true);
        w.set_bufrst(true);
    });
    wait_busy();

    let mut buf_addr = addr;
    let mut src = data.as_ptr() as *const u32;
    let words = data.len() as u32 / WORD;
    for _ in 0..words {
        let v = unsafe { src.read_unaligned() };
        unsafe { core::ptr::write_volatile(buf_addr as *mut u32, v) };
        FLASH.ctlr().write(|w| {
            w.set_page_pg(true);
            w.set_bufload(true);
        });
        wait_busy();
        buf_addr += WORD;
        src = unsafe { src.add(1) };
    }

    FLASH.addr().write(|w| w.set_addr(page_base));
    FLASH.ctlr().write(|w| {
        w.set_page_pg(true);
        w.set_strt(true);
    });
    wait_busy();
    FLASH.ctlr().write(|_| {});
}
