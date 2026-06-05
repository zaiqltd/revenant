//! Builds an ORIGINAL homebrew Game Boy game — SNAKE — and writes web/snake.gb.
//!
//! Polished edition (uses the shared assembler at examples/common/asm.rs):
//!   * the snake starts at LENGTH 4 heading right, so it reads as a snake on frame 1
//!   * an on-screen 3-digit decimal SCORE is drawn on the top wall row and ticks
//!     up (with carry across digits) each time food is eaten
//!   * gentle difficulty: the per-step frame budget shrinks as the score climbs,
//!     down to a playable floor
//!   * walls and self-collision kill you; death cleanly restarts to the length-4
//!     start with the score reset and a fresh board
//!   * no-180° steering, LFSR-placed food on a random EMPTY cell
//!
//! No copyrighted content (the boot-logo region is left zero; REVENANT skips the
//! boot ROM and runs from $0100).
//!
//!   cargo run --release --example makesnake   ->  web/snake.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- WRAM layout (all in free RAM $C000..$DFFF) ----
const DELTA: u16 = 0xC000; // 16-bit signed VRAM-address step for the current heading
const DIR: u16 = 0xC002; // 0=up 1=down 2=left 3=right (to forbid 180° turns)
const FRAME: u16 = 0xC003; // frames remaining until the next step
const HEAD: u16 = 0xC004; // 16-bit VRAM addr of the snake head
const HEADP: u16 = 0xC006; // ring-buffer write pointer
const TAILP: u16 = 0xC008; // ring-buffer read pointer
const RNG: u16 = 0xC00A; // 8-bit LFSR state
const TMP: u16 = 0xC00B; // scratch: the tile the head moved onto (push() clobbers regs)
const STEP: u16 = 0xC00C; // current frames-per-step (shrinks as you score)
const SC_H: u16 = 0xC00D; // score hundreds digit (0..9)
const SC_T: u16 = 0xC00E; // score tens digit (0..9)
const SC_O: u16 = 0xC00F; // score ones digit (0..9)
const RING: u16 = 0xC100; // ring of 16-bit body-cell addresses
const RING_END: u16 = 0xC200;

// ---- tunables ----
const STEP_START: u8 = 8; // frames per step at score 0 (~7.5 steps/sec)
const STEP_FLOOR: u8 = 3; // fastest the snake ever moves (~20 steps/sec)

// ---- tile indices in VRAM tile RAM @ $8000 ----
const T_EMPTY: u8 = 0;
const T_BODY: u8 = 1;
const T_FOOD: u8 = 2;
const T_WALL: u8 = 3;
const T_DIGIT0: u8 = 4; // digits 0..9 occupy tiles 4..13

// score display cells on the top wall row (row 0), three columns
const SCORE_H_CELL: u16 = 0x9800 + 14;
const SCORE_T_CELL: u16 = 0x9800 + 15;
const SCORE_O_CELL: u16 = 0x9800 + 16;

fn main() {
    let mut a = Asm::new();

    // ===== main: one-time setup =====
    a.label("main");
    a.di();
    a.xor_aa().ldh_to(0x40); // LCDC=0 (LCD off, safe to touch VRAM)
    // copy 14 tiles * 16 bytes (EMPTY,BODY,FOOD,WALL + ten digits) ROM -> $8000
    a.ld_hl_lbl("TILES").ld_de(0x8000).ld_bc(14 * 16);
    a.label("cpt");
    a.ldi_a_hl().ld_de_a().inc_de().dec_bc().ld_r_r(A, B).or_r(C).jr(NZ_JR, "cpt");
    a.ld_a(0xE4).ldh_to(0x47); // BGP = shades 3,2,1,0
    a.ldh_from(0x04).ld_nn_a(RNG); // seed LFSR from DIV
    a.jpa("restart");

    // ===== restart: (re)start a game =====
    a.label("restart");
    a.xor_aa().ldh_to(0x40); // LCD off
    // clear the whole 32x32 map -> EMPTY
    a.ld_hl(0x9800).ld_bc(0x0400);
    a.label("clr");
    a.xor_aa().ldi_hl_a().dec_bc().ld_r_r(A, B).or_r(C).jr(NZ_JR, "clr");
    a.call("border");
    // reset score to 000 and draw it
    a.xor_aa().ld_nn_a(SC_H).ld_nn_a(SC_T).ld_nn_a(SC_O);
    a.call("drawscore");
    // heading = right (+1), dir=3, step budget = STEP_START
    a.ld_hl(0x0001).store16(DELTA);
    a.ld_a(3).ld_nn_a(DIR);
    a.ld_a(STEP_START).ld_nn_a(STEP).ld_nn_a(FRAME);
    // empty ring
    a.ld_hl(RING).store16(HEADP).store16(TAILP);
    // --- lay a length-4 snake on row 8, cols 5..8, pushing TAIL-first so the
    //     oldest cell (col5) is popped first as the snake advances ---
    // col5 (tail)
    a.ld_r_n(B, 8).ld_r_n(C, 5).call("cell").ld_hl_imm(T_BODY).call("push");
    // col6
    a.ld_r_n(B, 8).ld_r_n(C, 6).call("cell").ld_hl_imm(T_BODY).call("push");
    // col7
    a.ld_r_n(B, 8).ld_r_n(C, 7).call("cell").ld_hl_imm(T_BODY).call("push");
    // col8 (head)
    a.ld_r_n(B, 8).ld_r_n(C, 8).call("cell").ld_hl_imm(T_BODY);
    a.store16(HEAD).call("push");
    a.call("food");
    a.ld_a(0x91).ldh_to(0x40); // LCD on, BG on, tiles @ $8000
    // fall through to the game loop

    // ===== game loop =====
    a.label("loop");
    a.call("vbl");
    a.call("input");
    a.ld_a_nn(FRAME).dec_r(A).ld_nn_a(FRAME); // dec FRAME
    a.jr(NZ_JR, "loop");
    a.ld_a_nn(STEP).ld_nn_a(FRAME); // reload step budget (dynamic speed)
    a.call("step");
    a.jra("loop");

    // ===== step: advance the snake one cell =====
    a.label("step");
    a.load_de(DELTA);
    a.load16(HEAD);
    a.add_hl_de(); // hl = new head addr
    a.ld_a_hl(); // a = target tile
    a.cp(T_WALL).jp(ZF, "restart"); // WALL -> die
    a.cp(T_BODY).jp(ZF, "restart"); // BODY -> die
    a.ld_nn_a(TMP); // save target tile (push clobbers regs)
    a.ld_hl_imm(T_BODY); // draw BODY at the new head
    a.store16(HEAD);
    a.call("push"); // record new head in the ring
    a.ld_a_nn(TMP).cp(T_FOOD);
    a.jr(NZ_JR, "noeat");
    // ate food: bump score, place new food, KEEP the tail (grow)
    a.call("scoreup");
    a.call("food");
    a.ret();
    a.label("noeat");
    a.call("pop"); // hl = tail addr
    a.ld_hl_imm(T_EMPTY); // erase the tail
    a.ret();

    // ===== scoreup: ++score (ones->tens->hundreds carry), speed up, redraw =====
    a.label("scoreup");
    // ones
    a.ld_a_nn(SC_O).inc_r(A).cp(10).jr(NZ_JR, "sc_done_o");
    a.xor_aa().ld_nn_a(SC_O);
    // tens
    a.ld_a_nn(SC_T).inc_r(A).cp(10).jr(NZ_JR, "sc_done_t");
    a.xor_aa().ld_nn_a(SC_T);
    // hundreds (caps at 9; score >999 just stops climbing the hundreds digit)
    a.ld_a_nn(SC_H).inc_r(A).cp(10).jr(NZ_JR, "sc_done_h");
    a.ld_a(9); // clamp
    a.label("sc_done_h");
    a.ld_nn_a(SC_H);
    a.jra("sc_speed");
    a.label("sc_done_t");
    a.ld_nn_a(SC_T);
    a.jra("sc_speed");
    a.label("sc_done_o");
    a.ld_nn_a(SC_O);
    // speed up: STEP-- unless already at the floor
    a.label("sc_speed");
    a.ld_a_nn(STEP).cp(STEP_FLOOR + 1).jr(C_JR, "sc_draw"); // STEP <= FLOOR -> skip
    a.dec_r(A).ld_nn_a(STEP);
    a.label("sc_draw");
    a.call("drawscore");
    a.ret();

    // ===== drawscore: write the three digit tiles onto the top wall row =====
    a.label("drawscore");
    a.ld_a_nn(SC_H).add_a(T_DIGIT0).ld_nn_a(SCORE_H_CELL);
    a.ld_a_nn(SC_T).add_a(T_DIGIT0).ld_nn_a(SCORE_T_CELL);
    a.ld_a_nn(SC_O).add_a(T_DIGIT0).ld_nn_a(SCORE_O_CELL);
    a.ret();

    // ===== push: ring[HEADP]=HL, advance with wrap =====
    a.label("push");
    a.ld_r_r(B, H).ld_r_r(C, L); // save value
    a.load16(HEADP);
    a.ld_r_r(M, C).inc_hl().ld_r_r(M, B).inc_hl();
    a.ld_r_r(A, H).cp((RING_END >> 8) as u8).jr(NZ_JR, "pnw");
    a.ld_r_r(A, L).cp(RING_END as u8).jr(NZ_JR, "pnw");
    a.ld_hl(RING);
    a.label("pnw");
    a.store16(HEADP);
    a.ret();

    // ===== pop: hl = ring[TAILP], advance with wrap =====
    a.label("pop");
    a.load16(TAILP);
    a.ld_r_r(C, M).inc_hl().ld_r_r(B, M).inc_hl();
    a.ld_r_r(A, H).cp((RING_END >> 8) as u8).jr(NZ_JR, "qnw");
    a.ld_r_r(A, L).cp(RING_END as u8).jr(NZ_JR, "qnw");
    a.ld_hl(RING);
    a.label("qnw");
    a.store16(TAILP);
    a.ld_r_r(H, B).ld_r_r(L, C); // hl = popped addr
    a.ret();

    // ===== input: D-pad -> heading (no 180°) =====
    a.label("input");
    a.ld_a(0x20).ldh_to(0x00).ldh_from(0x00); // select dirs, read
    a.ld_r_r(B, A); // b = dir bits (0=pressed)
    // UP (bit2), forbid if DIR==down(1)
    a.bit(2, B).jr(NZ_JR, "iu");
    a.ld_a_nn(DIR).cp(1).jr(Z_JR, "iu");
    a.ld_hl(0xFFE0).store16(DELTA).ld_a(0).ld_nn_a(DIR);
    a.label("iu");
    // DOWN (bit3), forbid if DIR==up(0)
    a.bit(3, B).jr(NZ_JR, "id");
    a.ld_a_nn(DIR).or_r(A).jr(Z_JR, "id"); // Z if DIR==0
    a.ld_hl(0x0020).store16(DELTA).ld_a(1).ld_nn_a(DIR);
    a.label("id");
    // LEFT (bit1), forbid if DIR==right(3)
    a.bit(1, B).jr(NZ_JR, "il");
    a.ld_a_nn(DIR).cp(3).jr(Z_JR, "il");
    a.ld_hl(0xFFFF).store16(DELTA).ld_a(2).ld_nn_a(DIR);
    a.label("il");
    // RIGHT (bit0), forbid if DIR==left(2)
    a.bit(0, B).jr(NZ_JR, "ir");
    a.ld_a_nn(DIR).cp(2).jr(Z_JR, "ir");
    a.ld_hl(0x0001).store16(DELTA).ld_a(3).ld_nn_a(DIR);
    a.label("ir");
    a.ret();

    // ===== vbl: block exactly one frame (wait for a fresh LY==145 edge) =====
    a.label("vbl");
    a.ldh_from(0x44).cp(145).jr(Z_JR, "vbl"); // wait until LY leaves 145
    a.label("vbl2");
    a.ldh_from(0x44).cp(145).jr(NZ_JR, "vbl2"); // then wait until it returns
    a.ret();

    // ===== cell: b=row c=col -> hl = $9800 + row*32 + col =====
    a.label("cell");
    a.ld_hl(0x9800).ld_r_n(D, 0).ld_r_n(E, 32); // de=32
    a.ld_r_r(A, B).or_r(A).jr(Z_JR, "ccol"); // a=row; z -> no row loop
    a.label("crow");
    a.add_hl_de().dec_r(A).jr(NZ_JR, "crow");
    a.label("ccol");
    a.ld_r_n(D, 0).ld_r_r(E, C).add_hl_de(); // + col
    a.ret();

    // ===== rng: 8-bit LFSR (tap 0x1D) mixed with DIV =====
    a.label("rng");
    a.ld_a_nn(RNG).add_aa().jr(NC_JR, "rs").xor_a(0x1D); // a<<1; if carry xor 0x1D
    a.label("rs");
    a.ld_r_r(B, A).ldh_from(0x04).xor_r(B); // a ^= DIV
    a.ld_nn_a(RNG);
    a.ret();

    // ===== food: place FOOD on a random empty cell (cols/rows 1..16) =====
    a.label("food");
    a.label("ftry");
    a.call("rng").and_a(0x0F).inc_r(A).ld_r_r(C, A); // col 1..16
    a.call("rng").and_a(0x0F).inc_r(A).ld_r_r(B, A); // row 1..16
    a.call("cell");
    a.ld_a_hl().or_r(A).jr(NZ_JR, "ftry"); // (hl)!=EMPTY -> retry
    a.ld_hl_imm(T_FOOD).ret();

    // ===== border: draw the wall ring (rows/cols 0 & 17) =====
    a.label("border");
    a.ld_r_n(B, 0); // b = i = 0
    a.label("bl");
    a.push_bc();
    // top row0, col=i
    a.ld_r_r(C, B).ld_r_n(B, 0).call("cell").ld_hl_imm(T_WALL);
    a.pop_bc().push_bc();
    // bottom row17, col=i
    a.ld_r_r(C, B).ld_r_n(B, 17).call("cell").ld_hl_imm(T_WALL);
    a.pop_bc().push_bc();
    // left col0, row=i
    a.ld_r_n(C, 0).call("cell").ld_hl_imm(T_WALL);
    a.pop_bc().push_bc();
    // right col17, row=i
    a.ld_r_n(C, 17).call("cell").ld_hl_imm(T_WALL);
    a.pop_bc();
    a.inc_r(B).ld_r_r(A, B).cp(18).jr(NZ_JR, "bl");
    a.ret();

    // ===== tile data: 4 game tiles + ten 5x7-ish digit tiles =====
    a.label("TILES");
    a.raw(&[0x00; 16]); // 0 EMPTY
    a.raw(&[0xFF; 16]); // 1 BODY (solid color 3)
    a.raw(&[
        0, 0, 0x3C, 0x3C, 0x7E, 0x7E, 0x7E, 0x7E, 0x7E, 0x7E, 0x7E, 0x7E, 0x3C, 0x3C, 0, 0,
    ]); // 2 FOOD dot
    a.raw(&[
        0, 0xFF, 0, 0xFF, 0, 0xFF, 0, 0xFF, 0, 0xFF, 0, 0xFF, 0, 0xFF, 0, 0xFF,
    ]); // 3 WALL (color 2 hatch)
    // digits 0..9 — each a 5-wide glyph in the high bits, color 3
    for g in DIGITS {
        for row in g {
            a.raw(&[row, row]); // both planes -> color 3
        }
    }

    let rom = a.build_rom("SNAKE");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/snake.gb", &rom).unwrap();
    println!("wrote web/snake.gb ({} bytes code+data at $0150)", a.c.len());
}

// 8-row glyphs (one byte per pixel row, MSB = leftmost pixel). Drawn at color 3.
#[rustfmt::skip]
const DIGITS: [[u8; 8]; 10] = [
    // 0
    [0b01110000, 0b10001000, 0b10011000, 0b10101000, 0b11001000, 0b10001000, 0b01110000, 0],
    // 1
    [0b00100000, 0b01100000, 0b00100000, 0b00100000, 0b00100000, 0b00100000, 0b01110000, 0],
    // 2
    [0b01110000, 0b10001000, 0b00001000, 0b00010000, 0b00100000, 0b01000000, 0b11111000, 0],
    // 3
    [0b11111000, 0b00010000, 0b00100000, 0b00010000, 0b00001000, 0b10001000, 0b01110000, 0],
    // 4
    [0b00010000, 0b00110000, 0b01010000, 0b10010000, 0b11111000, 0b00010000, 0b00010000, 0],
    // 5
    [0b11111000, 0b10000000, 0b11110000, 0b00001000, 0b00001000, 0b10001000, 0b01110000, 0],
    // 6
    [0b00110000, 0b01000000, 0b10000000, 0b11110000, 0b10001000, 0b10001000, 0b01110000, 0],
    // 7
    [0b11111000, 0b00001000, 0b00010000, 0b00100000, 0b01000000, 0b01000000, 0b01000000, 0],
    // 8
    [0b01110000, 0b10001000, 0b10001000, 0b01110000, 0b10001000, 0b10001000, 0b01110000, 0],
    // 9
    [0b01110000, 0b10001000, 0b10001000, 0b01111000, 0b00001000, 0b00010000, 0b01100000, 0],
];
