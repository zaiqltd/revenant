//! Builds an ORIGINAL homebrew Game Boy game — MEMORY — and writes web/memory.gb.
//!
//! A "repeat the sequence" memory game (Simon-style, original art). Four pads are
//! drawn as four big blocks on screen — one per D-pad direction:
//!   UP pad (top), DOWN pad (bottom), LEFT pad (left), RIGHT pad (right).
//! Each round the game FLASHES a growing sequence: each step lights its pad and
//! plays that pad's distinct tone (UP=1950, DOWN=1750, LEFT=1500, RIGHT=1200).
//! Then the player must repeat the whole sequence on the D-pad. A correct full
//! repeat grows the sequence by one, bumps the score (= sequence length reached),
//! and plays a reward chirp. A wrong press is GAME OVER.
//!
//! Sequence is stored in WRAM (up to 32 steps), each new step picked by an LFSR.
//! TITLE ("MEMORY" + PRESS START), on-screen score, GAME OVER + score + retry.
//!
//! Hand-assembled SM83 through the shared two-pass assembler. No copyrighted
//! content (the boot-logo region is left zero; REVENANT runs from $0100).
//!
//!   cargo run --release --example makememory   ->  web/memory.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- WRAM layout ($C000.. free) ----
const SEQ: u16 = 0xC000; // sequence buffer: 32 bytes, each 0..3 (pad index)
const LEN: u16 = 0xC020; // current sequence length (1..32)
const POS: u16 = 0xC021; // play-phase: index of next expected step
const STATE: u16 = 0xC022; // 0=title, 1=flashing, 2=player input, 3=over
const RNG: u16 = 0xC023; // 8-bit LFSR
const TICK: u16 = 0xC024; // frame divider for flash timing
const FLPOS: u16 = 0xC025; // flash phase: index of step being flashed
const FLON: u16 = 0xC026; // flash phase: 1 while a pad is lit, 0 in the gap
const D0: u16 = 0xC027; // score ones digit
const D1: u16 = 0xC028; // score tens digit
const LASTBTN: u16 = 0xC029; // previous frame's D-pad+Start bits (0=pressed), for edges
const LASTSTART: u16 = 0xC02A; // previous frame's Start bit (0=pressed)

// Pad index -> tone period (higher = higher pitch).
const TONE_UP: u16 = 1950;
const TONE_DOWN: u16 = 1750;
const TONE_LEFT: u16 = 1500;
const TONE_RIGHT: u16 = 1200;

// Flash timing (in game ticks; one tick == 1 frame here).
const FLASH_ON: u8 = 24; // frames a pad stays lit
const FLASH_OFF: u8 = 14; // dark gap between flashes

// Tile indices (art lives in 0..31; font owns $20..$5F). Index 0 is the blank tile.
const T_DIM: u8 = 1; // pad at rest (color 1)
const T_LIT: u8 = 2; // pad flashing / pressed (color 3)

const SCORE_AT: u16 = 0x9800 + 0 * 32 + 0; // "SCORE nn" top-left

fn main() {
    let mut a = Asm::new();

    // ================= one-time setup =================
    a.label("main");
    a.di();
    a.ld_sp(0xFFFE);
    a.xor_aa().ldh_to(0x40); // LCDC = 0 (LCD off so VRAM is safe to write)

    a.memcpy_lbl("TILES", 0x8000, 3 * 16); // load 3 art tiles into $8000 (indices 0..2)
    a.load_font(); // font -> tiles $20..$5F
    a.memset(0x9800, 0x20, 0x0400); // blank the BG map with the font SPACE glyph

    a.ld_a(0xE4).ldh_to(0x47); // BGP  = 3,2,1,0
    a.ld_a(0xE4).ldh_to(0x48); // OBP0 = 3,2,1,0

    a.apu_on(); // power up the sound hardware once

    a.ldh_from(0x04).or_a(1).ld_nn_a(RNG); // seed LFSR from DIV (force nonzero)

    a.ld_a(0x93).ldh_to(0x40); // LCD on, OBJ on, BG on, tiles @ $8000

    a.xor_aa().ld_nn_a(STATE); // start on the title screen
    a.ld_a(1).ld_nn_a(LASTSTART); // assume Start not held at boot
    a.ld_a(0x0F).ld_nn_a(LASTBTN); // assume no D-pad held at boot (all high)
    a.call("show_title"); // paint the title once

    // ================= main loop =================
    a.label("loop");
    a.wait_vblank();

    a.ld_a_nn(STATE);
    a.cp(1).jr(Z_JR, "st_flash");
    a.cp(2).jr(Z_JR, "st_input");
    a.cp(3).jr(Z_JR, "st_over");
    // ---- STATE 0: TITLE — wait for a fresh Start press ----
    a.call("start_edge"); // Z set => Start newly pressed
    a.jr(NZ_JR, "loop");
    a.call("begin_run"); // init a fresh run -> starts flashing
    a.jpa("loop");

    // ---- STATE 1: FLASH — play back the sequence, then hand off to input ----
    a.label("st_flash");
    a.call("flash_step");
    a.jpa("loop");

    // ---- STATE 2: INPUT — read the player's D-pad, compare to sequence ----
    a.label("st_input");
    a.call("player_input");
    a.jpa("loop");

    // ---- STATE 3: GAME OVER — wait for a fresh Start to retry ----
    a.label("st_over");
    a.call("start_edge");
    a.jr(NZ_JR, "loop");
    a.call("begin_run");
    a.jpa("loop");

    // ================= start_edge: Z if Start was just pressed this frame ===
    a.label("start_edge");
    a.ld_a(0x10).ldh_to(0x00); // select buttons
    a.ldh_from(0x00).ldh_from(0x00); // read twice to settle the matrix
    a.and_a(0x08); // isolate Start (bit3): 0=pressed
    a.ld_r_r(C, A); // c = current Start bit
    a.ld_a_nn(LASTSTART);
    a.ld_r_r(B, A); // b = last
    a.ld_r_r(A, C).ld_nn_a(LASTSTART); // store current as next frame's "last"
    a.ld_r_r(A, C).or_r(A).jr(NZ_JR, "se_no"); // current!=0 -> not pressed
    a.ld_r_r(A, B).or_r(A).jr(Z_JR, "se_no"); // last==0 (held) -> not an edge
    a.xor_aa(); // set Z => edge!
    a.ret();
    a.label("se_no");
    a.or_a(1); // clear Z => no edge
    a.ret();

    // ================= show_title: paint the TITLE screen =================
    a.label("show_title");
    a.call("clear_map");
    a.print(0x9800 + 4 * 32 + 7, "MEMORY");
    a.print(0x9800 + 7 * 32 + 2, "REPEAT THE TUNE");
    a.print(0x9800 + 9 * 32 + 4, "USE THE D-PAD");
    a.print(0x9800 + 14 * 32 + 4, "PRESS START");
    a.ret();

    // ================= begin_run: (re)start a fresh game =================
    a.label("begin_run");
    a.call("draw_pads"); // paint the four dim pads + score label
    a.xor_aa().ld_nn_a(D0); // score -> 0
    a.xor_aa().ld_nn_a(D1);
    a.call("drawscore");
    // Build the first step of the sequence.
    a.ld_a(1).ld_nn_a(LEN); // length 1
    a.call("rng");
    a.and_a(3).ld_nn_a(SEQ); // SEQ[0] = rng & 3
    a.call("start_flash"); // -> STATE 1
    a.ret();

    // ================= start_flash: enter the FLASH phase from the start =====
    a.label("start_flash");
    a.xor_aa().ld_nn_a(FLPOS); // flash from step 0
    a.xor_aa().ld_nn_a(FLON); // start in the (dark) gap so first thing is a clean light
    a.ld_a(FLASH_OFF).ld_nn_a(TICK); // tiny lead-in: expire immediately -> light step 0
    a.ld_a(1).ld_nn_a(STATE);
    a.call("pads_dark"); // make sure all pads are dim
    a.ret();

    // ================= flash_step: drive the sequence playback ===============
    // Runs once per frame in STATE 1. A TICK counter splits time into ON (pad lit)
    // and OFF (dark gap) windows. When all LEN steps have flashed, hand to input.
    a.label("flash_step");
    a.ld_a_nn(TICK).inc_r(A).ld_nn_a(TICK);
    a.ld_a_nn(FLON).or_r(A).jr(NZ_JR, "fl_on"); // FLON!=0 -> we're in an ON window

    // ----- OFF window: wait FLASH_OFF, then light the next step -----
    a.ld_a_nn(TICK).cp(FLASH_OFF).jr(C_JR, "fl_done"); // not long enough yet
    a.xor_aa().ld_nn_a(TICK);
    // Are there steps left to flash?  FLPOS < LEN ?
    a.ld_a_nn(FLPOS);
    a.ld_hl(LEN);
    a.cp_r(M).jr(C_JR, "fl_light"); // FLPOS < LEN -> light it
    // No steps left: playback finished -> player's turn.
    a.xor_aa().ld_nn_a(POS); // expect from step 0
    a.ld_a(2).ld_nn_a(STATE);
    a.call("pads_dark");
    a.jra("fl_done");

    a.label("fl_light");
    // Light pad SEQ[FLPOS] and play its tone.
    a.ld_a_nn(FLPOS);
    a.ld_hl(SEQ);
    a.ld_r_r(C, A).ld_r_n(B, 0);
    a.add_hl_bc(); // hl = SEQ + FLPOS
    a.ld_a_hl(); // a = pad index 0..3
    a.ld_r_r(C, A); // c = pad index (saved across calls below)
    a.call("light_pad"); // light pad C
    a.ld_r_r(A, C);
    a.call("pad_tone"); // play tone for pad C
    a.ld_a(1).ld_nn_a(FLON); // now in ON window
    a.jra("fl_done");

    // ----- ON window: keep lit until FLASH_ON, then darken + advance -----
    a.label("fl_on");
    a.ld_a_nn(TICK).cp(FLASH_ON).jr(C_JR, "fl_done");
    a.xor_aa().ld_nn_a(TICK);
    a.xor_aa().ld_nn_a(FLON); // back to OFF window
    a.call("pads_dark"); // darken the pad
    a.ld_a_nn(FLPOS).inc_r(A).ld_nn_a(FLPOS); // advance to next step
    a.label("fl_done");
    a.ret();

    // ================= player_input: read a fresh D-pad press, compare ======
    // One pad press per frame edge. UP/DOWN/LEFT/RIGHT map to pad 0/1/2/3.
    // Bits (active-low, 0=pressed): 0=Right,1=Left,2=Up,3=Down. LASTBTN holds
    // last frame's raw bits; a fresh press is a bit 1-last that is 0-now.
    a.label("player_input");
    a.ld_a(0x20).ldh_to(0x00); // select directions
    a.ldh_from(0x00).ldh_from(0x00); // read twice to debounce the matrix
    a.and_a(0x0F); // keep R,L,U,D (0=pressed)
    a.ld_r_r(C, A); // c = cur bits
    a.ld_a_nn(LASTBTN); // a = prev raw bits
    a.ld_r_r(B, A); // b = prev
    a.ld_r_r(A, C).ld_nn_a(LASTBTN); // save cur as next frame's prev
    // fresh-press = prev(=1, was high) AND ~cur(=1, now low)
    a.ld_r_r(A, C).cpl(); // a = ~cur (1 where pressed now)
    a.ld_r_r(E, A); // e = pressed-now mask
    a.ld_r_r(A, B).and_r(E); // a = prev & pressed-now = fresh press bits
    a.and_a(0x0F);
    a.jr(Z_JR, "pi_none"); // no fresh dir press this frame
    a.ld_r_r(D, A); // d = fresh-press bits, decode to a pad index

    // Decode fresh-press bits to a pad index. Bits: 0=Right,1=Left,2=Up,3=Down.
    // Priority Up,Down,Left,Right (only one acted on per frame).
    a.ld_r_r(A, D).bit(2, A).jr(Z_JR, "pi_chkdn"); // bit2 set? -> Up pressed
    a.ld_a(0).jra("pi_have"); // pad 0 = UP
    a.label("pi_chkdn");
    a.ld_r_r(A, D).bit(3, A).jr(Z_JR, "pi_chkl");
    a.ld_a(1).jra("pi_have"); // pad 1 = DOWN
    a.label("pi_chkl");
    a.ld_r_r(A, D).bit(1, A).jr(Z_JR, "pi_chkr");
    a.ld_a(2).jra("pi_have"); // pad 2 = LEFT
    a.label("pi_chkr");
    a.ld_r_r(A, D).bit(0, A).jr(Z_JR, "pi_none");
    a.ld_a(3); // pad 3 = RIGHT
    a.label("pi_have");
    // a = pressed pad index. Flash it + tone for feedback.
    a.ld_r_r(C, A); // c = pressed pad
    a.call("pads_dark");
    a.ld_r_r(A, C).call("light_pad");
    a.ld_r_r(A, C).call("pad_tone");
    // Compare against SEQ[POS].
    a.ld_a_nn(POS);
    a.ld_hl(SEQ);
    a.ld_r_r(E, A).ld_r_n(D, 0);
    a.add_hl_de();
    a.ld_a_hl(); // a = expected pad
    a.cp_r(C).jr(NZ_JR, "pi_wrong"); // mismatch -> game over
    // Correct. Advance POS; if POS==LEN the round is complete.
    a.ld_a_nn(POS).inc_r(A).ld_nn_a(POS);
    a.ld_hl(LEN);
    a.cp_r(M).jr(C_JR, "pi_none"); // POS < LEN -> keep waiting for next press
    // ---- whole sequence repeated! score++, grow sequence, replay ----
    a.call("round_won");
    a.label("pi_none");
    a.ret();

    a.label("pi_wrong");
    a.jpa("gameover");

    // ================= round_won: score++, append a step, re-flash =========
    a.label("round_won");
    a.tone(1900, 0xF3, 0x80); // reward chirp
    a.call("score_inc");
    // Grow: if LEN < 32, append a new random step and bump LEN.
    a.ld_a_nn(LEN).cp(32).jr(NC_JR, "rw_noappend");
    a.call("rng");
    a.and_a(3).ld_r_r(C, A); // c = new pad index
    a.ld_a_nn(LEN);
    a.ld_hl(SEQ);
    a.ld_r_r(E, A).ld_r_n(D, 0);
    a.add_hl_de(); // hl = SEQ + LEN
    a.ld_r_r(A, C).ld_hl_a(); // SEQ[LEN] = new step
    a.ld_a_nn(LEN).inc_r(A).ld_nn_a(LEN); // LEN++
    a.label("rw_noappend");
    a.call("start_flash"); // replay the (longer) sequence
    a.ret();

    // ================= gameover: low tone, paint over screen, STATE=3 ========
    a.label("gameover");
    a.tone(700, 0xF1, 0x80); // low "wrong" tone
    a.call("clear_map");
    a.print(0x9800 + 5 * 32 + 5, "GAME OVER");
    a.print(0x9800 + 8 * 32 + 5, "SCORE");
    a.ld_a_nn(D1).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 11);
    a.ld_a_nn(D0).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 12);
    a.print(0x9800 + 13 * 32 + 4, "PRESS START");
    a.ld_a(3).ld_nn_a(STATE);
    a.jpa("loop");

    // ================= score_inc: 2-digit decimal ripple (D0 ones, D1 tens) ==
    a.label("score_inc");
    a.ld_a_nn(D0).inc_r(A).cp(10).jr(C_JR, "si_d0");
    a.xor_aa().ld_nn_a(D0); // ones rolls to 0
    a.ld_a_nn(D1).inc_r(A).cp(10).jr(C_JR, "si_d1");
    a.ld_a(9).ld_nn_a(D1); // clamp at 99
    a.jra("si_draw");
    a.label("si_d1");
    a.ld_nn_a(D1);
    a.jra("si_draw");
    a.label("si_d0");
    a.ld_nn_a(D0);
    a.label("si_draw");
    a.call("drawscore");
    a.ret();

    // ================= drawscore: write "SCORE nn" digits =================
    a.label("drawscore");
    a.print(SCORE_AT, "SCORE");
    a.ld_a_nn(D1).add_a(0x30).ld_nn_a(SCORE_AT + 6);
    a.ld_a_nn(D0).add_a(0x30).ld_nn_a(SCORE_AT + 7);
    a.ret();

    // ================= draw_pads: paint the four dim pads (fresh field) =====
    a.label("draw_pads");
    a.call("clear_map");
    a.call("pads_dark"); // fills the four pad rectangles with the DIM tile
    a.ret();

    // ================= pads_dark: fill all four pad rectangles with T_DIM ===
    a.label("pads_dark");
    a.ld_a(T_DIM);
    a.call("paint_pads"); // paint every pad with the value in A
    a.ret();

    // ================= light_pad: paint pad index A with T_LIT ==============
    // A = pad index 0..3. Repaints just that one pad rectangle bright.
    a.label("light_pad");
    a.cp(0).jr(NZ_JR, "lp_1");
    a.ld_a(T_LIT).call("fill_up");
    a.ret();
    a.label("lp_1");
    a.cp(1).jr(NZ_JR, "lp_2");
    a.ld_a(T_LIT).call("fill_dn");
    a.ret();
    a.label("lp_2");
    a.cp(2).jr(NZ_JR, "lp_3");
    a.ld_a(T_LIT).call("fill_lf");
    a.ret();
    a.label("lp_3");
    a.ld_a(T_LIT).call("fill_rt");
    a.ret();

    // ================= paint_pads: fill all 4 pad rectangles with tile in A ==
    a.label("paint_pads");
    a.ld_r_r(B, A); // b = tile value
    // UP pad
    a.ld_r_r(A, B).call("fill_up");
    a.ld_r_r(A, B).call("fill_dn");
    a.ld_r_r(A, B).call("fill_lf");
    a.ld_r_r(A, B).call("fill_rt");
    a.ret();

    // Per-pad fillers. Each fills a rectangle of BG-map cells with tile in A.
    // Layout (cols 0..19, rows 2..17 used; row 0 = score line):
    //   UP   : rows 3..6,   cols 7..12
    //   DOWN : rows 13..16, cols 7..12
    //   LEFT : rows 8..11,  cols 1..6
    //   RIGHT: rows 8..11,  cols 13..18
    emit_fill(&mut a, "fill_up", 3, 7, 4, 6);
    emit_fill(&mut a, "fill_dn", 13, 7, 4, 6);
    emit_fill(&mut a, "fill_lf", 8, 1, 4, 6);
    emit_fill(&mut a, "fill_rt", 8, 13, 4, 6);

    // ================= pad_tone: play the tone for pad index in A ===========
    a.label("pad_tone");
    a.cp(0).jr(NZ_JR, "pt_1");
    a.tone(TONE_UP, 0xF3, 0x80);
    a.ret();
    a.label("pt_1");
    a.cp(1).jr(NZ_JR, "pt_2");
    a.tone(TONE_DOWN, 0xF3, 0x80);
    a.ret();
    a.label("pt_2");
    a.cp(2).jr(NZ_JR, "pt_3");
    a.tone(TONE_LEFT, 0xF3, 0x80);
    a.ret();
    a.label("pt_3");
    a.tone(TONE_RIGHT, 0xF3, 0x80);
    a.ret();

    // ================= clear_map: blank the visible BG with SPACE ($20) ======
    a.label("clear_map");
    a.memset(0x9800, 0x20, 0x0400);
    a.ret();

    // ================= rng: 8-bit LFSR (tap 0x1D) mixed with DIV =========
    a.label("rng");
    a.ld_a_nn(RNG).add_aa().jr(NC_JR, "rng_ns").xor_a(0x1D);
    a.label("rng_ns");
    a.ld_r_r(B, A).ldh_from(0x04).xor_r(B); // mix in DIV
    a.ld_nn_a(RNG);
    a.ret();

    // ================= tile data: 3 tiles x 16 bytes =================
    a.label("TILES");
    // 0: blank
    a.raw(&[0u8; 16]);
    // 1: pad at rest — checkerboard at color 1 (dim)
    a.raw(&dim_tile([0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA]));
    // 2: pad lit — solid color 3 (bright)
    a.raw(&solid_tile([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]));

    // ================= font data (tiles $20..$5F) =================
    a.label("FONT");
    a.raw(&font_blob());

    // ================= emit ROM =================
    let rom = a.build_rom("MEMORY");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/memory.gb", &rom).unwrap();
    println!("wrote web/memory.gb ({} bytes code+data at $0150)", a.c.len());
}

/// Emit a labelled routine that fills a `w`x`h` rectangle of BG-map cells (top-left
/// at row,col on the 32-wide map $9800) with the tile value passed in register A.
fn emit_fill(a: &mut Asm, name: &str, row: u16, col: u16, h: u16, w: u16) {
    a.label(name);
    a.ld_r_r(B, A); // b = tile value
    let base = 0x9800 + row * 32 + col;
    let rowlp = a.uniq("frow");
    let collp = a.uniq("fcol");
    let skip = a.uniq("fcar");
    // D = remaining rows, start HL at the first cell of the rectangle.
    a.ld_r_n(D, h as u8);
    a.ld_hl(base);
    a.label(&rowlp);
    a.ld_r_n(E, w as u8); // E = remaining cols in this row
    a.label(&collp);
    a.ld_r_r(A, B).ldi_hl_a(); // write tile, advance HL
    a.dec_r(E).jr(NZ_JR, &collp);
    // Advance HL to the next row start: it moved +w already, add (32 - w).
    a.ld_r_r(A, L).add_a((32 - w) as u8).ld_r_r(L, A); // ld r,r keeps the add's carry
    a.jr(NC_JR, &skip); // no carry into H -> skip the inc
    a.inc_r(H);
    a.label(&skip);
    a.dec_r(D).jr(NZ_JR, &rowlp);
    a.ret();
}

/// 4bpp helper: build a tile (16 bytes) from 8 row masks rendered at color 1
/// (low bitplane set, high bitplane clear -> palette index 1, the "dim" shade).
fn dim_tile(rows: [u8; 8]) -> [u8; 16] {
    let mut t = [0u8; 16];
    for i in 0..8 {
        t[i * 2] = rows[i]; // low plane = pattern
        t[i * 2 + 1] = 0; // high plane = 0 -> color index 1 where low set
    }
    t
}
