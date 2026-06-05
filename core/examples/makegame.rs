//! Builds an ORIGINAL homebrew Game Boy ROM (no copyrighted content) and writes
//! it to web/game.gb. It's a tiny interactive demo: a smiley sprite you move
//! around with the D-pad. Hand-assembled SM83 with a two-pass label resolver.
//!
//!   cargo run --release --example makegame
//!
//! The ROM targets REVENANT (which skips the DMG boot ROM), so the Nintendo
//! logo region is left zeroed — it runs in this emulator from $0100 directly.

use std::collections::HashMap;

const BASE: u16 = 0x0150; // where our code lives

#[derive(Default)]
struct Asm {
    code: Vec<u8>,
    labels: HashMap<String, usize>,
    // (pos_of_rel_byte, target_label) for jr; (pos_of_lo_byte, label) for abs refs
    rel_fix: Vec<(usize, String)>,
    abs_fix: Vec<(usize, String)>,
}
impl Asm {
    fn b(&mut self, x: u8) { self.code.push(x); }
    fn bs(&mut self, xs: &[u8]) { self.code.extend_from_slice(xs); }
    fn label(&mut self, n: &str) { self.labels.insert(n.into(), self.code.len()); }
    fn addr(&self) -> u16 { BASE + self.code.len() as u16 }

    // ld a,n / ldh (n),a / ldh a,(n) / ld b,n / ld c,n
    fn ld_a(&mut self, n: u8) { self.bs(&[0x3E, n]); }
    fn ldh_to(&mut self, n: u8) { self.bs(&[0xE0, n]); }
    fn ldh_from(&mut self, n: u8) { self.bs(&[0xF0, n]); }
    fn ld_b(&mut self, n: u8) { self.bs(&[0x06, n]); }
    fn ld_c(&mut self, n: u8) { self.bs(&[0x0E, n]); }
    fn ld_hl(&mut self, nn: u16) { self.bs(&[0x21, nn as u8, (nn >> 8) as u8]); }
    fn ld_de(&mut self, nn: u16) { self.bs(&[0x11, nn as u8, (nn >> 8) as u8]); }
    fn cp(&mut self, n: u8) { self.bs(&[0xFE, n]); }
    fn bit_a(&mut self, bit: u8) { self.bs(&[0xCB, 0x47 | (bit << 3)]); }

    // conditional/uncond relative jumps to a label (back- or forward-patched)
    fn jr(&mut self, cc: Option<u8>, lbl: &str) {
        match cc { Some(op) => self.b(op), None => self.b(0x18) }
        let pos = self.code.len();
        self.b(0x00);
        self.rel_fix.push((pos, lbl.into()));
    }
    fn jp(&mut self, lbl: &str) {
        self.b(0xC3);
        let pos = self.code.len();
        self.bs(&[0, 0]);
        self.abs_fix.push((pos, lbl.into()));
    }

    fn resolve(&mut self) {
        for (pos, lbl) in &self.rel_fix {
            let target = self.labels[lbl] as isize;
            let off = target - (*pos as isize + 1);
            assert!((-128..=127).contains(&off), "jr out of range to {lbl}: {off}");
            self.code[*pos] = off as i8 as u8;
        }
        for (pos, lbl) in &self.abs_fix {
            let a = BASE + self.labels[lbl] as u16;
            self.code[*pos] = a as u8;
            self.code[*pos + 1] = (a >> 8) as u8;
        }
    }
}

// JR condition opcodes
const NZ: u8 = 0x20;
const Z: u8 = 0x28;

fn main() {
    let mut a = Asm::default();

    // The player tile (8x8, color 3) — a smiley. We place it in ROM and copy it
    // into VRAM tile #1 at $8000. Each row = 2 bytes (both bit-planes = mask).
    let face: [u8; 8] = [0x3C, 0x42, 0xA5, 0x81, 0xA5, 0x99, 0x42, 0x3C];

    // ---- setup ----
    a.b(0xF3); // di
    a.ld_a(0x00); a.ldh_to(0x40); // LCDC = 0 -> LCD off (safe to touch VRAM)

    // copy 16 tile bytes (face, each row twice) from ROM table -> $8000
    a.ld_hl(0); // placeholder, patched to TILE table addr
    let hl_fix = a.code.len() - 2;
    a.ld_de(0x8010); // sprite tile #1 lives at $8000 + 1*16
    a.ld_b(16);
    a.label("copy");
    a.b(0x2A);            // ld a,(hl+)
    a.b(0x12);            // ld (de),a
    a.b(0x13);            // inc de
    a.b(0x05);            // dec b
    a.jr(Some(NZ), "copy");

    // sprite 0: Y=B, X=C, tile=1, attr=0  (write OAM directly)
    a.ld_b(0x48); // Y
    a.ld_c(0x50); // X
    a.ld_a(0x01); a.bs(&[0xEA, 0x02, 0xFE]); // ld ($FE02),a  tile=1
    a.ld_a(0x00); a.bs(&[0xEA, 0x03, 0xFE]); // ld ($FE03),a  attr=0

    // palettes: BGP/OBP0 = 0xE4 (3,2,1,0)
    a.ld_a(0xE4); a.ldh_to(0x47); // BGP
    a.ld_a(0xE4); a.ldh_to(0x48); // OBP0

    // LCD on, OBJ on, BG on  -> LCDC = 0x83
    a.ld_a(0x83); a.ldh_to(0x40);

    // ---- main loop ----
    a.label("loop");
    // wait for VBlank start (LY == 144)
    a.label("w1");
    a.ldh_from(0x44); a.cp(144); a.jr(Some(NZ), "w1");

    // read D-pad: select directions (write $20 to JOYP), then read low nibble
    a.ld_a(0x20); a.ldh_to(0x00);
    a.ldh_from(0x00); // a = directions, 0 = pressed (bits: R0 L1 U2 D3)

    // RIGHT (bit0): if pressed (bit==0 -> Z), inc C
    a.bit_a(0); a.jr(Some(NZ), "nr"); a.b(0x0C); a.label("nr"); // inc c
    // LEFT (bit1): dec C
    a.bit_a(1); a.jr(Some(NZ), "nl"); a.b(0x0D); a.label("nl"); // dec c
    // UP (bit2): dec B
    a.bit_a(2); a.jr(Some(NZ), "nu"); a.b(0x05); a.label("nu"); // dec b
    // DOWN (bit3): inc B
    a.bit_a(3); a.jr(Some(NZ), "nd"); a.b(0x04); a.label("nd"); // inc b

    // write Y,X to OAM sprite 0  (ld a,b; ld ($FE00),a ; ld a,c; ld ($FE01),a)
    a.b(0x78); a.bs(&[0xEA, 0x00, 0xFE]); // Y
    a.b(0x79); a.bs(&[0xEA, 0x01, 0xFE]); // X

    // wait for VBlank end (LY != 144) so we update once per frame
    a.label("w2");
    a.ldh_from(0x44); a.cp(144); a.jr(Some(Z), "w2");
    a.jp("loop");

    // tile data table
    a.label("TILE");
    for row in face { a.bs(&[row, row]); } // both planes = mask -> color 3

    a.resolve();
    // patch the ld hl,TILE
    let tile_addr = BASE + a.labels["TILE"] as u16;
    a.code[hl_fix] = tile_addr as u8;
    a.code[hl_fix + 1] = (tile_addr >> 8) as u8;

    // ---- assemble the 32KB ROM ----
    let mut rom = vec![0u8; 0x8000];
    // entry at $0100: nop ; jp $0150
    rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]);
    // title "REVENANT" at $0134
    for (i, c) in b"REVENANT".iter().enumerate() { rom[0x0134 + i] = *c; }
    // cart type ROM-only, 32KB, no RAM (defaults 0 already)
    // header checksum ($014D)
    let mut x: u8 = 0;
    for i in 0x0134..=0x014C { x = x.wrapping_sub(rom[i]).wrapping_sub(1); }
    rom[0x014D] = x;
    // our code at $0150
    rom[0x0150..0x0150 + a.code.len()].copy_from_slice(&a.code);

    std::fs::create_dir_all("web").ok();
    std::fs::write("web/game.gb", &rom).expect("write rom");
    println!("wrote web/game.gb ({} bytes code at $0150, {}-byte ROM)", a.code.len(), rom.len());
}
