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

    // ---- sound ----
    /// Power the APU on at full volume, both channels panned to both sides.
    pub fn apu_on(&mut self) -> &mut Self {
        self.ld_a(0x80).ldh_to(0x26)   // NR52: APU on
            .ld_a(0x77).ldh_to(0x24)   // NR50: max master volume L+R
            .ld_a(0xFF).ldh_to(0x25)   // NR51: all channels to both sides
    }
    /// Trigger a short blip on channel 1. `freq` is the 11-bit period (higher =
    /// higher pitch), `env` an NR12 envelope (e.g. 0xF3 = vol 15, decay), `dutylen`
    /// an NR11 duty+length (e.g. 0x80 = 50% duty). Good for SFX (eat/bounce/die).
    pub fn tone(&mut self, freq: u16, env: u8, dutylen: u8) -> &mut Self {
        self.ld_a(0x00).ldh_to(0x10)                              // NR10: no sweep
            .ld_a(dutylen).ldh_to(0x11)                           // NR11: duty + length
            .ld_a(env).ldh_to(0x12)                               // NR12: envelope
            .ld_a(freq as u8).ldh_to(0x13)                        // NR13: freq lo
            .ld_a(0x80 | ((freq >> 8) as u8 & 7)).ldh_to(0x14)    // NR14: trigger + freq hi
    }

    // ---- text (8x8 bitmap font) ----
    /// Copy the bundled font into VRAM tiles $20..$5F (ASCII space..'_'), so a
    /// BG-map cell set to an ASCII byte renders that character. Requires the game
    /// to also place the data once: `a.label("FONT"); a.raw(&font_blob());`
    /// and to run with LCDC tile-data @ $8000 (the default 0x91/0x93).
    pub fn load_font(&mut self) -> &mut Self { self.memcpy_lbl("FONT", 0x8200, 64 * 16) }
    /// Write an ASCII string into consecutive BG-map cells starting at `map_addr`.
    pub fn print(&mut self, map_addr: u16, text: &str) -> &mut Self {
        for (i, ch) in text.bytes().enumerate() {
            let c = if (0x20..0x60).contains(&ch) { ch } else { 0x20 };
            self.ld_a(c).ld_nn_a(map_addr + i as u16);
        }
        self
    }

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

/// Build the 1 KiB font blob: 64 tiles for ASCII $20..$5F, each at color 3.
/// Original 5x7-in-8x8 glyphs. Place via `a.label("FONT"); a.raw(&font_blob());`
pub fn font_blob() -> Vec<u8> {
    // Each glyph is 8 rows of 8 chars; '#' = pixel. Designed on a 5-wide cell.
    let g = |rows: [&str; 8]| -> [u8; 8] {
        let mut out = [0u8; 8];
        for (r, line) in rows.iter().enumerate() {
            let mut b = 0u8;
            for (i, ch) in line.bytes().enumerate() { if ch == b'#' { b |= 0x80 >> i; } }
            out[r] = b;
        }
        out
    };
    let blank = [0u8; 8];
    let glyph = |ch: u8| -> [u8; 8] {
        match ch {
            b'0' => g([" ### ", "#   #", "#  ##", "# # #", "##  #", "#   #", " ### ", "     "].map_pad()),
            b'1' => g(["  #  ", " ##  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### ", "     "].map_pad()),
            b'2' => g([" ### ", "#   #", "    #", "   # ", "  #  ", " #   ", "#####", "     "].map_pad()),
            b'3' => g(["#### ", "    #", "    #", " ### ", "    #", "    #", "#### ", "     "].map_pad()),
            b'4' => g(["   # ", "  ## ", " # # ", "#  # ", "#####", "   # ", "   # ", "     "].map_pad()),
            b'5' => g(["#####", "#    ", "#### ", "    #", "    #", "#   #", " ### ", "     "].map_pad()),
            b'6' => g([" ### ", "#    ", "#    ", "#### ", "#   #", "#   #", " ### ", "     "].map_pad()),
            b'7' => g(["#####", "    #", "   # ", "  #  ", " #   ", " #   ", " #   ", "     "].map_pad()),
            b'8' => g([" ### ", "#   #", "#   #", " ### ", "#   #", "#   #", " ### ", "     "].map_pad()),
            b'9' => g([" ### ", "#   #", "#   #", " ####", "    #", "    #", " ### ", "     "].map_pad()),
            b'A' => g([" ### ", "#   #", "#   #", "#####", "#   #", "#   #", "#   #", "     "].map_pad()),
            b'B' => g(["#### ", "#   #", "#   #", "#### ", "#   #", "#   #", "#### ", "     "].map_pad()),
            b'C' => g([" ### ", "#   #", "#    ", "#    ", "#    ", "#   #", " ### ", "     "].map_pad()),
            b'D' => g(["###  ", "#  # ", "#   #", "#   #", "#   #", "#  # ", "###  ", "     "].map_pad()),
            b'E' => g(["#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#####", "     "].map_pad()),
            b'F' => g(["#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#    ", "     "].map_pad()),
            b'G' => g([" ### ", "#   #", "#    ", "# ###", "#   #", "#   #", " ### ", "     "].map_pad()),
            b'H' => g(["#   #", "#   #", "#   #", "#####", "#   #", "#   #", "#   #", "     "].map_pad()),
            b'I' => g([" ### ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### ", "     "].map_pad()),
            b'J' => g(["  ###", "   # ", "   # ", "   # ", "#  # ", "#  # ", " ##  ", "     "].map_pad()),
            b'K' => g(["#   #", "#  # ", "# #  ", "##   ", "# #  ", "#  # ", "#   #", "     "].map_pad()),
            b'L' => g(["#    ", "#    ", "#    ", "#    ", "#    ", "#    ", "#####", "     "].map_pad()),
            b'M' => g(["#   #", "## ##", "# # #", "#   #", "#   #", "#   #", "#   #", "     "].map_pad()),
            b'N' => g(["#   #", "##  #", "# # #", "#  ##", "#   #", "#   #", "#   #", "     "].map_pad()),
            b'O' => g([" ### ", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ", "     "].map_pad()),
            b'P' => g(["#### ", "#   #", "#   #", "#### ", "#    ", "#    ", "#    ", "     "].map_pad()),
            b'Q' => g([" ### ", "#   #", "#   #", "#   #", "# # #", "#  # ", " ## #", "     "].map_pad()),
            b'R' => g(["#### ", "#   #", "#   #", "#### ", "# #  ", "#  # ", "#   #", "     "].map_pad()),
            b'S' => g([" ####", "#    ", "#    ", " ### ", "    #", "    #", "#### ", "     "].map_pad()),
            b'T' => g(["#####", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "     "].map_pad()),
            b'U' => g(["#   #", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ", "     "].map_pad()),
            b'V' => g(["#   #", "#   #", "#   #", "#   #", "#   #", " # # ", "  #  ", "     "].map_pad()),
            b'W' => g(["#   #", "#   #", "#   #", "#   #", "# # #", "## ##", "#   #", "     "].map_pad()),
            b'X' => g(["#   #", "#   #", " # # ", "  #  ", " # # ", "#   #", "#   #", "     "].map_pad()),
            b'Y' => g(["#   #", "#   #", " # # ", "  #  ", "  #  ", "  #  ", "  #  ", "     "].map_pad()),
            b'Z' => g(["#####", "    #", "   # ", "  #  ", " #   ", "#    ", "#####", "     "].map_pad()),
            b'!' => g(["  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "     ", "  #  ", "     "].map_pad()),
            b'-' => g(["     ", "     ", "     ", "#####", "     ", "     ", "     ", "     "].map_pad()),
            b'.' => g(["     ", "     ", "     ", "     ", "     ", "     ", "  #  ", "     "].map_pad()),
            b':' => g(["     ", "  #  ", "     ", "     ", "     ", "  #  ", "     ", "     "].map_pad()),
            b'/' => g(["    #", "    #", "   # ", "  #  ", " #   ", "#    ", "#    ", "     "].map_pad()),
            _ => blank,
        }
    };
    let mut out = Vec::with_capacity(64 * 16);
    for code in 0x20u8..0x60 {
        for row in glyph(code) { out.push(row); out.push(row); } // lo=hi -> color 3
    }
    out
}

// Helper: pad each 5-char row to the 8-wide field (1px left margin).
trait Pad { fn map_pad(self) -> [&'static str; 8]; }
impl Pad for [&'static str; 8] {
    fn map_pad(self) -> [&'static str; 8] {
        // Glyphs are authored 5 wide; shift right by 1 column for spacing.
        const PADDED: [&str; 0] = [];
        let _ = PADDED;
        self.map(|s| {
            // Leak a padded copy (build-time only, tiny, runs in an example binary).
            let padded = format!(" {s}");
            Box::leak(padded.into_boxed_str()) as &str
        })
    }
}
