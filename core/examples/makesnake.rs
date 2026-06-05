//! Builds an ORIGINAL homebrew Game Boy game — SNAKE — and writes web/snake.gb.
//! Borders kill you, food grows you, running into yourself kills you, death
//! restarts. Hand-assembled SM83 with a two-pass label resolver and subroutines
//! (call/ret) to keep the code compact. No copyrighted content (the boot-logo
//! region is left zero; REVENANT skips the boot ROM and runs from $0100).
//!
//!   cargo run --release --example makesnake   ->  web/snake.gb

use std::collections::HashMap;

const BASE: u16 = 0x0150;

// WRAM layout
const DELTA: u16 = 0xC000;   // 16-bit signed VRAM-address step for the current heading
const DIR: u16 = 0xC002;     // 0=up 1=down 2=left 3=right (to forbid 180° turns)
const FRAME: u16 = 0xC003;   // frames remaining until the next step
const HEAD: u16 = 0xC004;    // 16-bit VRAM addr of the snake head
const HEADP: u16 = 0xC006;   // ring-buffer write pointer
const TAILP: u16 = 0xC008;   // ring-buffer read pointer
const RNG: u16 = 0xC00A;     // 8-bit LFSR state
const TMP: u16 = 0xC00B;     // scratch: the tile the head moved onto (push() clobbers regs)
const RING: u16 = 0xC100;    // ring of 16-bit body-cell addresses
const RING_END: u16 = 0xC200;
const SPEED: u8 = 6;         // frames per snake step

#[derive(Default)]
struct Asm {
    c: Vec<u8>,
    labels: HashMap<String, usize>,
    rel: Vec<(usize, String)>,
    abs: Vec<(usize, String)>,
}
impl Asm {
    fn r(&mut self, b: &[u8]) { self.c.extend_from_slice(b); }
    fn here(&mut self, n: &str) { self.labels.insert(n.into(), self.c.len()); }
    fn jr(&mut self, cc: u8, l: &str) { self.r(&[cc]); let p = self.c.len(); self.r(&[0]); self.rel.push((p, l.into())); }
    fn jra(&mut self, l: &str) { self.jr(0x18, l); }
    fn jp(&mut self, cc: u8, l: &str) { self.r(&[cc]); let p = self.c.len(); self.r(&[0, 0]); self.abs.push((p, l.into())); }
    fn call(&mut self, l: &str) { self.r(&[0xCD]); let p = self.c.len(); self.r(&[0, 0]); self.abs.push((p, l.into())); }
    // 16-bit helpers operating on HL / DE
    fn ld_hl(&mut self, v: u16) { self.r(&[0x21, v as u8, (v >> 8) as u8]); }
    fn ld_de(&mut self, v: u16) { self.r(&[0x11, v as u8, (v >> 8) as u8]); }
    fn ld_bc(&mut self, v: u16) { self.r(&[0x01, v as u8, (v >> 8) as u8]); }
    fn lda(&mut self, n: u8) { self.r(&[0x3E, n]); }
    fn lda_nn(&mut self, a: u16) { self.r(&[0xFA, a as u8, (a >> 8) as u8]); }       // ld a,(nn)
    fn nn_a(&mut self, a: u16) { self.r(&[0xEA, a as u8, (a >> 8) as u8]); }          // ld (nn),a
    fn ldh_to(&mut self, n: u8) { self.r(&[0xE0, n]); }
    fn ldh_from(&mut self, n: u8) { self.r(&[0xF0, n]); }
    fn store16(&mut self, a: u16) { self.r(&[0x7D]); self.nn_a(a); self.r(&[0x7C]); self.nn_a(a + 1); } // l->a;hi too
    fn load16(&mut self, a: u16) { self.lda_nn(a); self.r(&[0x6F]); self.lda_nn(a + 1); self.r(&[0x67]); } // ->hl
    fn load_de(&mut self, a: u16) { self.lda_nn(a); self.r(&[0x5F]); self.lda_nn(a + 1); self.r(&[0x57]); } // ->de
    fn resolve(&mut self) {
        for (p, l) in &self.rel {
            let off = self.labels[l] as isize - (*p as isize + 1);
            assert!((-128..=127).contains(&off), "jr range {l} {off}");
            self.c[*p] = off as i8 as u8;
        }
        for (p, l) in &self.abs {
            let a = BASE + self.labels[l] as u16;
            self.c[*p] = a as u8; self.c[*p + 1] = (a >> 8) as u8;
        }
    }
}

// jr / jp condition bytes
const JRNZ: u8 = 0x20; const JRZ: u8 = 0x28; const JRNC: u8 = 0x30;
const JPNZ: u8 = 0xC2; const JPZ: u8 = 0xCA; const JP: u8 = 0xC3;

fn main() {
    let mut a = Asm::default();

    // ===== main: one-time setup =====
    a.here("main");
    a.r(&[0xF3]);                         // di
    a.r(&[0xAF]); a.ldh_to(0x40);          // LCDC=0 (LCD off)
    a.ld_hl(0); let tdfix = a.c.len() - 2; // ld hl,TILES (patched)
    a.ld_de(0x8000); a.r(&[0x06, 64]);     // de=tile RAM, b=64
    a.here("cpt");
    a.r(&[0x2A, 0x12, 0x13, 0x05]);        // ld a,(hl+); ld (de),a; inc de; dec b
    a.jr(JRNZ, "cpt");
    a.lda(0xE4); a.ldh_to(0x47);           // BGP = 0xE4
    a.ldh_from(0x04); a.nn_a(RNG);         // seed LFSR from DIV
    a.jp(JP, "restart");

    // ===== restart: (re)start a game =====
    a.here("restart");
    a.r(&[0xAF]); a.ldh_to(0x40);          // LCD off
    a.ld_hl(0x9800); a.ld_bc(0x0400);      // clear the whole 32x32 map -> EMPTY
    a.here("clr");
    a.r(&[0xAF, 0x22, 0x0B, 0x78, 0xB1]);  // xor a; ld(hl+),a; dec bc; ld a,b; or c
    a.jr(JRNZ, "clr");
    a.call("border");
    a.ld_hl(0x0001); a.store16(DELTA);     // heading = right (+1)
    a.lda(3); a.nn_a(DIR);
    a.lda(SPEED); a.nn_a(FRAME);
    a.ld_hl(RING); a.store16(HEADP); a.store16(TAILP);
    a.r(&[0x06, 8, 0x0E, 8]); a.call("cell"); // b=row8 c=col8 -> hl
    a.r(&[0x36, 1]);                       // (hl)=BODY
    a.store16(HEAD);
    a.call("push");                        // seed the body ring with the head
    a.call("food");
    a.lda(0x91); a.ldh_to(0x40);           // LCD on, BG on, tiles @8000
    // fall through to game loop

    // ===== game loop =====
    a.here("loop");
    a.call("vbl");
    a.call("input");
    a.lda_nn(FRAME); a.r(&[0x3D]); a.nn_a(FRAME); // dec FRAME
    a.jr(JRNZ, "loop");
    a.lda(SPEED); a.nn_a(FRAME);
    a.call("step");
    a.jra("loop");

    // ===== step: advance the snake one cell =====
    a.here("step");
    a.load_de(DELTA);
    a.load16(HEAD);
    a.r(&[0x19]);                          // add hl,de -> new head addr
    a.r(&[0x7E]);                          // a=(hl) target tile
    a.r(&[0xFE, 3]); a.jp(JPZ, "restart"); // WALL -> die
    a.r(&[0xFE, 1]); a.jp(JPZ, "restart"); // BODY -> die
    a.nn_a(TMP);                           // save target tile to RAM (push() clobbers regs)
    a.r(&[0x36, 1]);                       // (hl)=BODY
    a.store16(HEAD);
    a.call("push");                        // record new head in the ring
    a.lda_nn(TMP); a.r(&[0xFE, 2]);        // a=target tile; cp FOOD
    a.jr(JRNZ, "noeat");
    a.call("food");                        // ate -> new food, keep the tail (grow)
    a.r(&[0xC9]);                          // ret
    a.here("noeat");
    a.call("pop");                         // hl = tail addr
    a.r(&[0x36, 0]);                       // (hl)=EMPTY (erase tail)
    a.r(&[0xC9]);

    // ===== push: ring[HEADP]=HL, advance with wrap =====
    a.here("push");
    a.r(&[0x44, 0x4D]);                    // b=h, c=l (save value)
    a.load16(HEADP);
    a.r(&[0x71, 0x23, 0x70, 0x23]);        // ld(hl),c; inc hl; ld(hl),b; inc hl
    a.r(&[0x7C, 0xFE, (RING_END >> 8) as u8]); a.jr(JRNZ, "pnw"); // h==C2?
    a.r(&[0x7D, 0xFE, RING_END as u8]); a.jr(JRNZ, "pnw");        // l==00?
    a.ld_hl(RING);
    a.here("pnw");
    a.store16(HEADP);
    a.r(&[0xC9]);

    // ===== pop: hl = ring[TAILP], advance with wrap =====
    a.here("pop");
    a.load16(TAILP);
    a.r(&[0x4E, 0x23, 0x46, 0x23]);        // ld c,(hl); inc hl; ld b,(hl); inc hl
    a.r(&[0x7C, 0xFE, (RING_END >> 8) as u8]); a.jr(JRNZ, "qnw");
    a.r(&[0x7D, 0xFE, RING_END as u8]); a.jr(JRNZ, "qnw");
    a.ld_hl(RING);
    a.here("qnw");
    a.store16(TAILP);
    a.r(&[0x60, 0x69]);                    // h=b; l=c -> hl = popped addr
    a.r(&[0xC9]);

    // ===== input: D-pad -> heading (no 180°) =====
    a.here("input");
    a.lda(0x20); a.ldh_to(0x00); a.ldh_from(0x00); // select dirs, read
    a.r(&[0x47]);                          // b = dir bits (0=pressed)
    // UP (bit2), forbid if DIR==down(1)
    a.r(&[0xCB, 0x50]); a.jr(JRNZ, "iu");  // bit2,b
    a.lda_nn(DIR); a.r(&[0xFE, 1]); a.jr(JRZ, "iu");
    a.ld_hl(0xFFE0); a.store16(DELTA); a.lda(0); a.nn_a(DIR);
    a.here("iu");
    // DOWN (bit3), forbid if DIR==up(0)
    a.r(&[0xCB, 0x58]); a.jr(JRNZ, "id");
    a.lda_nn(DIR); a.r(&[0xB7]); a.jr(JRZ, "id"); // or a -> Z if DIR==0
    a.ld_hl(0x0020); a.store16(DELTA); a.lda(1); a.nn_a(DIR);
    a.here("id");
    // LEFT (bit1), forbid if DIR==right(3)
    a.r(&[0xCB, 0x48]); a.jr(JRNZ, "il");
    a.lda_nn(DIR); a.r(&[0xFE, 3]); a.jr(JRZ, "il");
    a.ld_hl(0xFFFF); a.store16(DELTA); a.lda(2); a.nn_a(DIR);
    a.here("il");
    // RIGHT (bit0), forbid if DIR==left(2)
    a.r(&[0xCB, 0x40]); a.jr(JRNZ, "ir");
    a.lda_nn(DIR); a.r(&[0xFE, 2]); a.jr(JRZ, "ir");
    a.ld_hl(0x0001); a.store16(DELTA); a.lda(3); a.nn_a(DIR);
    a.here("ir");
    a.r(&[0xC9]);

    // ===== vbl: block exactly one frame (wait for a fresh LY==145 edge) =====
    a.here("vbl");
    a.ldh_from(0x44); a.r(&[0xFE, 145]); a.jr(JRZ, "vbl");   // first wait until LY leaves 145
    a.here("vbl2");
    a.ldh_from(0x44); a.r(&[0xFE, 145]); a.jr(JRNZ, "vbl2"); // then wait until it returns to 145
    a.r(&[0xC9]);

    // ===== cell: b=row c=col -> hl = 0x9800 + row*32 + col =====
    a.here("cell");
    a.ld_hl(0x9800); a.r(&[0x16, 0, 0x1E, 32]); // de=32
    a.r(&[0x78, 0xB7]); a.jr(JRZ, "ccol");      // a=row; or a; z->skip
    a.here("crow");
    a.r(&[0x19, 0x3D]);                    // add hl,de; dec a
    a.jr(JRNZ, "crow");
    a.here("ccol");
    a.r(&[0x16, 0, 0x59, 0x19]);           // d=0; e=c; add hl,de
    a.r(&[0xC9]);

    // ===== rng: 8-bit LFSR (tap 0x1D) mixed with DIV =====
    a.here("rng");
    a.lda_nn(RNG); a.r(&[0x87]); a.jr(JRNC, "rs"); a.r(&[0xEE, 0x1D]); // a<<1; if carry xor 0x1D
    a.here("rs");
    a.r(&[0x47]); a.ldh_from(0x04); a.r(&[0xA8]); // b=a; a^=DIV (xor b)
    a.nn_a(RNG);
    a.r(&[0xC9]);

    // ===== food: place FOOD on a random empty cell =====
    a.here("food");
    a.here("ftry");
    a.call("rng"); a.r(&[0xE6, 0x0F, 0x3C, 0x4F]); // and 0x0F; inc a; c=a (col 1..16)
    a.call("rng"); a.r(&[0xE6, 0x0F, 0x3C, 0x47]); // and 0x0F; inc a; b=a (row 1..16)
    a.call("cell");
    a.r(&[0x7E, 0xB7]); a.jr(JRNZ, "ftry"); // (hl)!=EMPTY -> retry
    a.r(&[0x36, 2, 0xC9]);                 // (hl)=FOOD; ret

    // ===== border: draw the wall ring (rows/cols 0 & 17) =====
    a.here("border");
    a.r(&[0x06, 0]);                       // b = i = 0
    a.here("bl");
    a.r(&[0xC5]);                          // push bc
    // top (row0,col=i): cell wants b=row,c=col
    a.r(&[0x48, 0x06, 0]); a.call("cell"); a.r(&[0x36, 3]); // c=b(i); b=0; (hl)=WALL
    a.r(&[0xC1, 0xC5]);                    // pop bc; push bc
    a.r(&[0x48, 0x06, 17]); a.call("cell"); a.r(&[0x36, 3]); // bottom row17
    a.r(&[0xC1, 0xC5]);
    a.r(&[0x0E, 0]); a.call("cell"); a.r(&[0x36, 3]); // left col0 (b=i already=row)
    a.r(&[0xC1, 0xC5]);
    a.r(&[0x0E, 17]); a.call("cell"); a.r(&[0x36, 3]); // right col17
    a.r(&[0xC1]);                          // pop bc
    a.r(&[0x04, 0x78, 0xFE, 18]); a.jr(JRNZ, "bl"); // inc b; a=b; cp 18
    a.r(&[0xC9]);

    // ===== tile data (4 tiles x 16 bytes) =====
    a.here("TILES");
    a.r(&[0x00; 16]);                                 // 0 EMPTY
    a.r(&[0xFF; 16]);                                 // 1 BODY (solid, color 3)
    a.r(&[0,0, 0x3C,0x3C, 0x7E,0x7E, 0x7E,0x7E, 0x7E,0x7E, 0x7E,0x7E, 0x3C,0x3C, 0,0]); // 2 FOOD dot
    a.r(&[0,0xFF, 0,0xFF, 0,0xFF, 0,0xFF, 0,0xFF, 0,0xFF, 0,0xFF, 0,0xFF]);             // 3 WALL (color 2)

    a.resolve();
    let tiles = BASE + a.labels["TILES"] as u16;
    a.c[tdfix] = tiles as u8; a.c[tdfix + 1] = (tiles >> 8) as u8;

    // ROM image
    let mut rom = vec![0u8; 0x8000];
    rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]); // nop; jp $0150
    for (i, c) in b"SNAKE".iter().enumerate() { rom[0x0134 + i] = *c; }
    let mut x: u8 = 0; for i in 0x0134..=0x014C { x = x.wrapping_sub(rom[i]).wrapping_sub(1); }
    rom[0x014D] = x;
    rom[0x0150..0x0150 + a.c.len()].copy_from_slice(&a.c);
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/snake.gb", &rom).unwrap();
    println!("wrote web/snake.gb  ({} bytes of code+data at $0150)", a.c.len());
}
