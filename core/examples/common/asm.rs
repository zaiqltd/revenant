//! Shared SM83 mini-assembler for REVENANT's homebrew games.
//!
//! Included by each game example via `#[path = "common/asm.rs"] mod asm;`. It is
//! NOT an example target itself (it lives in a subdirectory and has no `main`).
//!
//! Two-pass: emit instructions referencing string labels, then `build_rom`
//! resolves them and lays the code at $0150 inside a valid 32 KiB ROM-only cart.
//! REVENANT skips the DMG boot ROM, so the boot-logo region is left zero (the
//! ROM is original — no copyrighted content).
#![allow(dead_code)]
use std::collections::HashMap;

pub const BASE: u16 = 0x0150;

// Register indices for ld r,r' / alu r / inc r / dec r  (B C D E H L (HL) A)
pub const B: u8 = 0; pub const C: u8 = 1; pub const D: u8 = 2; pub const E: u8 = 3;
pub const H: u8 = 4; pub const L: u8 = 5; pub const M: u8 = 6; pub const A: u8 = 7;

// jr / jp / call condition opcodes
pub const NZ_JR: u8 = 0x20; pub const Z_JR: u8 = 0x28; pub const NC_JR: u8 = 0x30; pub const C_JR: u8 = 0x38;
pub const NZ: u8 = 0xC2; pub const ZF: u8 = 0xCA; pub const NCF: u8 = 0xD2; pub const CF: u8 = 0xDA;
pub const CALL_NZ: u8 = 0xC4; pub const CALL_Z: u8 = 0xCC;

#[derive(Default)]
pub struct Asm {
    pub c: Vec<u8>,
    labels: HashMap<String, usize>,
    rel: Vec<(usize, String)>,
    abs: Vec<(usize, String)>,
    uid: usize,
}

impl Asm {
    pub fn new() -> Self { Asm::default() }
    pub fn uniq(&mut self, p: &str) -> String { self.uid += 1; format!("{p}_{}", self.uid) }

    // ---- raw + labels ----
    pub fn raw(&mut self, b: &[u8]) -> &mut Self { self.c.extend_from_slice(b); self }
    pub fn op(&mut self, b: u8) -> &mut Self { self.c.push(b); self }
    pub fn label(&mut self, n: &str) -> &mut Self { self.labels.insert(n.into(), self.c.len()); self }

    // ---- control flow (label targets) ----
    pub fn jr(&mut self, cc: u8, l: &str) -> &mut Self { self.op(cc); let p = self.c.len(); self.raw(&[0]); self.rel.push((p, l.into())); self }
    pub fn jra(&mut self, l: &str) -> &mut Self { self.jr(0x18, l) }
    pub fn jp(&mut self, cc: u8, l: &str) -> &mut Self { self.op(cc); let p = self.c.len(); self.raw(&[0, 0]); self.abs.push((p, l.into())); self }
    pub fn jpa(&mut self, l: &str) -> &mut Self { self.jp(0xC3, l) }
    pub fn jp_hl(&mut self) -> &mut Self { self.op(0xE9) }
    pub fn call(&mut self, l: &str) -> &mut Self { self.op(0xCD); let p = self.c.len(); self.raw(&[0, 0]); self.abs.push((p, l.into())); self }
    pub fn callc(&mut self, cc: u8, l: &str) -> &mut Self { self.op(cc); let p = self.c.len(); self.raw(&[0, 0]); self.abs.push((p, l.into())); self }
    pub fn ret(&mut self) -> &mut Self { self.op(0xC9) }
    pub fn reti(&mut self) -> &mut Self { self.op(0xD9) }

    // ---- loads (immediate) ----
    pub fn ld_r_n(&mut self, r: u8, n: u8) -> &mut Self { self.raw(&[0x06 | (r << 3), n]) }
    pub fn ld_a(&mut self, n: u8) -> &mut Self { self.ld_r_n(A, n) }
    pub fn ld_bc(&mut self, v: u16) -> &mut Self { self.raw(&[0x01, v as u8, (v >> 8) as u8]) }
    pub fn ld_de(&mut self, v: u16) -> &mut Self { self.raw(&[0x11, v as u8, (v >> 8) as u8]) }
    pub fn ld_hl(&mut self, v: u16) -> &mut Self { self.raw(&[0x21, v as u8, (v >> 8) as u8]) }
    pub fn ld_sp(&mut self, v: u16) -> &mut Self { self.raw(&[0x31, v as u8, (v >> 8) as u8]) }
    /// ld hl, <address of label>  (e.g. a tile-data table)
    pub fn ld_hl_lbl(&mut self, l: &str) -> &mut Self { self.op(0x21); let p = self.c.len(); self.raw(&[0, 0]); self.abs.push((p, l.into())); self }

    // ---- loads (register / memory) ----
    pub fn ld_r_r(&mut self, d: u8, s: u8) -> &mut Self { self.op(0x40 | (d << 3) | s) }
    pub fn ld_a_hl(&mut self) -> &mut Self { self.op(0x7E) }
    pub fn ld_hl_a(&mut self) -> &mut Self { self.op(0x77) }
    pub fn ld_hl_imm(&mut self, n: u8) -> &mut Self { self.raw(&[0x36, n]) }   // ld (hl),n
    pub fn ldi_hl_a(&mut self) -> &mut Self { self.op(0x22) }                  // ld (hl+),a
    pub fn ldi_a_hl(&mut self) -> &mut Self { self.op(0x2A) }                  // ld a,(hl+)
    pub fn ldd_hl_a(&mut self) -> &mut Self { self.op(0x32) }                  // ld (hl-),a
    pub fn ld_a_de(&mut self) -> &mut Self { self.op(0x1A) }
    pub fn ld_de_a(&mut self) -> &mut Self { self.op(0x12) }
    pub fn ldh_to(&mut self, n: u8) -> &mut Self { self.raw(&[0xE0, n]) }      // ldh ($FFn),a
    pub fn ldh_from(&mut self, n: u8) -> &mut Self { self.raw(&[0xF0, n]) }    // ldh a,($FFn)
    pub fn ld_nn_a(&mut self, a: u16) -> &mut Self { self.raw(&[0xEA, a as u8, (a >> 8) as u8]) }
    pub fn ld_a_nn(&mut self, a: u16) -> &mut Self { self.raw(&[0xFA, a as u8, (a >> 8) as u8]) }

    // 16-bit var helpers (operate on HL / DE through A)
    pub fn store16(&mut self, a: u16) -> &mut Self { self.ld_r_r(A, L); self.ld_nn_a(a); self.ld_r_r(A, H); self.ld_nn_a(a + 1) }
    pub fn load16(&mut self, a: u16) -> &mut Self { self.ld_a_nn(a); self.ld_r_r(L, A); self.ld_a_nn(a + 1); self.ld_r_r(H, A) }
    pub fn load_de(&mut self, a: u16) -> &mut Self { self.ld_a_nn(a); self.ld_r_r(E, A); self.ld_a_nn(a + 1); self.ld_r_r(D, A) }

    // ---- arithmetic / logic ----
    pub fn inc_r(&mut self, r: u8) -> &mut Self { self.op(0x04 | (r << 3)) }
    pub fn dec_r(&mut self, r: u8) -> &mut Self { self.op(0x05 | (r << 3)) }
    pub fn inc_bc(&mut self) -> &mut Self { self.op(0x03) }
    pub fn inc_de(&mut self) -> &mut Self { self.op(0x13) }
    pub fn inc_hl(&mut self) -> &mut Self { self.op(0x23) }
    pub fn dec_bc(&mut self) -> &mut Self { self.op(0x0B) }
    pub fn dec_de(&mut self) -> &mut Self { self.op(0x1B) }
    pub fn dec_hl(&mut self) -> &mut Self { self.op(0x2B) }
    pub fn add_hl_bc(&mut self) -> &mut Self { self.op(0x09) }
    pub fn add_hl_de(&mut self) -> &mut Self { self.op(0x19) }
    pub fn add_a(&mut self, n: u8) -> &mut Self { self.raw(&[0xC6, n]) }
    pub fn sub_a(&mut self, n: u8) -> &mut Self { self.raw(&[0xD6, n]) }
    pub fn and_a(&mut self, n: u8) -> &mut Self { self.raw(&[0xE6, n]) }
    pub fn or_a(&mut self, n: u8) -> &mut Self { self.raw(&[0xF6, n]) }
    pub fn xor_a(&mut self, n: u8) -> &mut Self { self.raw(&[0xEE, n]) }
    pub fn cp(&mut self, n: u8) -> &mut Self { self.raw(&[0xFE, n]) }
    pub fn add_r(&mut self, r: u8) -> &mut Self { self.op(0x80 | r) }
    pub fn sub_r(&mut self, r: u8) -> &mut Self { self.op(0x90 | r) }
    pub fn and_r(&mut self, r: u8) -> &mut Self { self.op(0xA0 | r) }
    pub fn or_r(&mut self, r: u8) -> &mut Self { self.op(0xB0 | r) }
    pub fn xor_r(&mut self, r: u8) -> &mut Self { self.op(0xA8 | r) }
    pub fn cp_r(&mut self, r: u8) -> &mut Self { self.op(0xB8 | r) }
    pub fn xor_aa(&mut self) -> &mut Self { self.op(0xAF) } // a=0
    pub fn cpl(&mut self) -> &mut Self { self.op(0x2F) }
    pub fn add_aa(&mut self) -> &mut Self { self.op(0x87) } // a<<1
    pub fn swap_a(&mut self) -> &mut Self { self.raw(&[0xCB, 0x37]) }

    // ---- bit ops (CB) ----
    pub fn bit(&mut self, n: u8, r: u8) -> &mut Self { self.raw(&[0xCB, 0x40 | (n << 3) | r]) }
    pub fn res(&mut self, n: u8, r: u8) -> &mut Self { self.raw(&[0xCB, 0x80 | (n << 3) | r]) }
    pub fn set(&mut self, n: u8, r: u8) -> &mut Self { self.raw(&[0xCB, 0xC0 | (n << 3) | r]) }

    // ---- stack ----
    pub fn push_bc(&mut self) -> &mut Self { self.op(0xC5) }
    pub fn push_de(&mut self) -> &mut Self { self.op(0xD5) }
    pub fn push_hl(&mut self) -> &mut Self { self.op(0xE5) }
    pub fn push_af(&mut self) -> &mut Self { self.op(0xF5) }
    pub fn pop_bc(&mut self) -> &mut Self { self.op(0xC1) }
    pub fn pop_de(&mut self) -> &mut Self { self.op(0xD1) }
    pub fn pop_hl(&mut self) -> &mut Self { self.op(0xE1) }
    pub fn pop_af(&mut self) -> &mut Self { self.op(0xF1) }
    pub fn di(&mut self) -> &mut Self { self.op(0xF3) }
    pub fn ei(&mut self) -> &mut Self { self.op(0xFB) }
    pub fn nop(&mut self) -> &mut Self { self.op(0x00) }

    // ---- high-level macros ----
    /// Wait for exactly one frame (a fresh LY==145 edge). Inline (no call).
    pub fn wait_vblank(&mut self) -> &mut Self {
        let a = self.uniq("vbA"); let b = self.uniq("vbB");
        self.label(&a).ldh_from(0x44).cp(145).jr(Z_JR, &a);
        self.label(&b).ldh_from(0x44).cp(145).jr(NZ_JR, &b)
    }
    /// memset: fill `len` bytes at `dst` with `val`.
    pub fn memset(&mut self, dst: u16, val: u8, len: u16) -> &mut Self {
        let lp = self.uniq("ms");
        self.ld_hl(dst).ld_bc(len);
        self.label(&lp).ld_a(val).ldi_hl_a().dec_bc().ld_r_r(A, B).or_r(C).jr(NZ_JR, &lp)
    }
    /// memcpy from a ROM label to `dst`, `len` bytes.
    pub fn memcpy_lbl(&mut self, src: &str, dst: u16, len: u16) -> &mut Self {
        let lp = self.uniq("mc");
        self.ld_hl_lbl(src).ld_de(dst).ld_bc(len);
        self.label(&lp).ldi_a_hl().ld_de_a().inc_de().dec_bc().ld_r_r(A, B).or_r(C).jr(NZ_JR, &lp)
    }

    // ---- finalize ----
    fn resolve(&mut self) {
        for (p, l) in &self.rel {
            let t = *self.labels.get(l).unwrap_or_else(|| panic!("missing label {l}")) as isize;
            let off = t - (*p as isize + 1);
            assert!((-128..=127).contains(&off), "jr out of range to {l}: {off}");
            self.c[*p] = off as i8 as u8;
        }
        for (p, l) in &self.abs {
            let a = BASE + *self.labels.get(l).unwrap_or_else(|| panic!("missing label {l}")) as u16;
            self.c[*p] = a as u8; self.c[*p + 1] = (a >> 8) as u8;
        }
    }

    /// Resolve labels and emit a complete 32 KiB ROM with a valid header.
    pub fn build_rom(&mut self, title: &str) -> Vec<u8> {
        self.resolve();
        let mut rom = vec![0u8; 0x8000];
        rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, BASE as u8, (BASE >> 8) as u8]); // nop; jp BASE
        for (i, ch) in title.bytes().take(15).enumerate() { rom[0x0134 + i] = ch; }
        let mut x: u8 = 0;
        for i in 0x0134..=0x014C { x = x.wrapping_sub(rom[i]).wrapping_sub(1); }
        rom[0x014D] = x; // header checksum
        assert!(0x0150 + self.c.len() <= 0x8000, "code overflows bank 0");
        rom[0x0150..0x0150 + self.c.len()].copy_from_slice(&self.c);
        rom
    }
}

/// 4bpp-mask helper: build a tile (16 bytes) from 8 row masks, all at color 3.
pub fn solid_tile(rows: [u8; 8]) -> [u8; 16] {
    let mut t = [0u8; 16];
    for i in 0..8 { t[i * 2] = rows[i]; t[i * 2 + 1] = rows[i]; }
    t
}
