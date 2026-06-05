//! Builds an ORIGINAL homebrew Game Boy game — BLOCKS — and writes web/blocks.gb.
//!
//! A minimalist falling-block puzzler (my own simple design — NOT Tetris, no
//! tetromino shapes). A single 1x1 block falls from the top of a 10-wide by
//! 14-tall well, one row every N frames. The D-pad slides it Left/Right and Down
//! drops it faster. When it lands (on the floor or a settled block) it settles
//! into the well. Whenever a row is COMPLETELY filled it clears, everything above
//! shifts down, and the score goes up (with a chime). If a fresh block has no room
//! at the top -> GAME OVER.
//!
//! Full game feel: a TITLE screen ("BLOCKS" + "PRESS START"), an on-screen
//! decimal SCORE, sound on land / clear / game-over, and a GAME OVER screen that
//! shows the score and waits for START to retry. Robust over fancy: the well is a
//! 140-byte WRAM mirror so logic never reads VRAM back.
//!
//! Hand-assembled SM83 via the shared two-pass assembler. No copyrighted content
//! (the boot-logo region is left zero; REVENANT skips the DMG boot ROM and runs
//! from $0100).
//!
//!   cargo run --release --example makeblocks   ->  web/blocks.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- well geometry ----
const COLS: u8 = 10; // playfield width  (cells)
const ROWS: u8 = 14; // playfield height (cells)
const ORIGIN_COL: u8 = 5; // map column of well col 0 (centres the 10-wide well)
const ORIGIN_ROW: u8 = 2; // map row    of well row 0 (room for a title bar on top)

// ---- WRAM layout ($C000.. free) ----
const STATE: u16 = 0xC000; // 0 = title, 1 = play, 2 = game over
const PX: u16 = 0xC001; // active block well-column (0..COLS-1)
const PY: u16 = 0xC002; // active block well-row    (0..ROWS-1)
const FALLT: u16 = 0xC003; // frames left until the block falls one row
const STEP: u16 = 0xC004; // current frames-per-fall (shrinks as you score)
const RNG: u16 = 0xC005; // 8-bit LFSR state
const PREVST: u16 = 0xC006; // previous frame's START bit (edge detect)
const D0: u16 = 0xC007; // score ones digit  (0..9)
const D1: u16 = 0xC008; // score tens digit
const D2: u16 = 0xC009; // score hundreds digit
const GRID: u16 = 0xC020; // 140-byte well mirror: GRID[row*COLS + col], 0=empty

// ---- tunables ----
const STEP_START: u8 = 30; // frames per fall at score 0 (~0.5 s)
const STEP_FLOOR: u8 = 8; // fastest the block ever falls
const SOFT_STEP: u8 = 3; // fall cadence while DOWN is held (soft drop)
const MOVE_EVERY: u8 = 6; // frames between horizontal auto-repeats

// ---- tile indices (art in 0..31 so it never collides with the font $20..$5F) ----
const T_EMPTY: u8 = 0; // empty well cell
const T_BLOCK: u8 = 1; // a settled / active block
const T_WALL: u8 = 2; // well border

// score digits as font tiles: '0' is ASCII 0x30 -> font occupies $20..$5F, so a
// digit value d renders as tile (0x30 + d). The font is loaded by load_font().
const DIGIT0: u8 = 0x30;

// score display cells (on the top title bar, inside VRAM map)
const SC_H_CELL: u16 = 0x9800 + 0 * 32 + 16;
const SC_T_CELL: u16 = 0x9800 + 0 * 32 + 17;
const SC_O_CELL: u16 = 0x9800 + 0 * 32 + 18;

// auto-repeat counter for horizontal movement (kept in WRAM)
const MOVET: u16 = 0xC00A;

fn main() {
    let mut a = Asm::new();

    // ===================== one-time setup =====================
    a.label("main");
    a.di();
    a.ld_sp(0xFFFE);
    a.xor_aa().ldh_to(0x40); // LCDC = 0 (LCD off so VRAM is safe to write)

    a.apu_on();

    // Load the 3 art tiles (EMPTY, BLOCK, WALL) into $8000..$802F, then the font
    // into $8200.. (tiles $20..$5F). Art tiles 0..31 never collide with the font.
    a.memcpy_lbl("TILES", 0x8000, 3 * 16);
    a.load_font();

    a.ld_a(0xE4).ldh_to(0x47); // BGP = shades 3,2,1,0
    a.ld_a(0xE4).ldh_to(0x48); // OBP0
    a.ldh_from(0x04).or_a(1).ld_nn_a(RNG); // seed LFSR from DIV (force nonzero)

    a.xor_aa().ld_nn_a(PREVST); // no START held yet
    a.ld_a(0x91).ldh_to(0x40); // LCD on, BG on, tiles @ $8000

    // Begin on the title screen.
    a.call("title_draw");
    a.ld_a(0).ld_nn_a(STATE);

    // ===================== main loop =====================
    a.label("loop");
    a.wait_vblank();
    a.ld_a_nn(STATE);
    a.cp(0).jr(Z_JR, "do_title");
    a.cp(1).jr(Z_JR, "do_play");
    // STATE == 2 -> game over
    a.call("over_tick");
    a.jra("loop");
    a.label("do_title");
    a.call("title_tick");
    a.jra("loop");
    a.label("do_play");
    a.call("play_tick");
    a.jra("loop");

    // ===================== title_tick: wait for START to begin =====================
    a.label("title_tick");
    a.call("start_edge"); // A = 1 on a fresh START press, else 0
    a.cp(1).jr(NZ_JR, "tt_ret");
    a.call("newgame");
    a.label("tt_ret");
    a.ret();

    // ===================== over_tick: wait for START to retry =====================
    a.label("over_tick");
    a.call("start_edge");
    a.cp(1).jr(NZ_JR, "ot_ret");
    a.call("newgame");
    a.label("ot_ret");
    a.ret();

    // ===================== newgame: init a fresh game and enter PLAY ===============
    a.label("newgame");
    a.xor_aa().ldh_to(0x40); // LCD off so we can rebuild the map cleanly

    // clear the entire 32x32 BG map to EMPTY (tile 0)
    a.memset(0x9800, 0x00, 0x0400);

    // clear the WRAM grid mirror (ROWS*COLS bytes)
    a.memset(GRID, 0x00, (ROWS as u16) * (COLS as u16));

    a.call("draw_border"); // paint the static well walls into the map

    // reset score to 000
    a.xor_aa().ld_nn_a(D0).ld_nn_a(D1).ld_nn_a(D2);

    // fall cadence
    a.ld_a(STEP_START).ld_nn_a(STEP).ld_nn_a(FALLT);
    a.xor_aa().ld_nn_a(MOVET);

    a.call("spawn"); // place the first block (also draws score header text)

    a.call("draw_score"); // header label + digits
    a.ld_a(0x91).ldh_to(0x40); // LCD back on
    a.ld_a(1).ld_nn_a(STATE); // -> PLAY
    a.ret();

    // ===================== play_tick: one frame of gameplay =====================
    a.label("play_tick");
    a.call("input"); // horizontal move + soft-drop cadence select

    // count down the fall timer; when it expires, try to fall one row
    a.ld_a_nn(FALLT).dec_r(A).ld_nn_a(FALLT);
    a.jr(NZ_JR, "pt_ret"); // not time to fall yet
    a.call("reload_fallt"); // FALLT = STEP (or SOFT_STEP if DOWN held)
    a.call("fall_one"); // advance / settle / clear / maybe spawn / maybe game-over
    a.label("pt_ret");
    a.ret();

    // ===================== reload_fallt: pick fall cadence =====================
    // If DOWN (bit3) is held, use SOFT_STEP, else the score-scaled STEP.
    a.label("reload_fallt");
    a.ld_a(0x20).ldh_to(0x00).ldh_from(0x00).ldh_from(0x00); // read d-pad
    a.bit(3, A).jr(NZ_JR, "rf_normal"); // DOWN not pressed
    a.ld_a(SOFT_STEP).ld_nn_a(FALLT);
    a.ret();
    a.label("rf_normal");
    a.ld_a_nn(STEP).ld_nn_a(FALLT);
    a.ret();

    // ===================== input: horizontal movement (auto-repeat) =====================
    a.label("input");
    // throttle horizontal movement so a held d-pad slides at a sane rate
    a.ld_a_nn(MOVET);
    a.or_a(0).jr(Z_JR, "in_read");
    a.dec_r(A).ld_nn_a(MOVET);
    a.ret(); // still cooling down -> ignore horizontal this frame
    a.label("in_read");
    a.ld_a(0x20).ldh_to(0x00).ldh_from(0x00).ldh_from(0x00); // select dirs, read
    a.ld_r_r(B, A); // b = dir bits (0 = pressed)

    // LEFT (bit1): try PX-1
    a.bit(1, B).jr(NZ_JR, "in_right");
    a.ld_a_nn(PX).or_a(0).jr(Z_JR, "in_right"); // already at col 0
    a.dec_r(A).ld_r_r(C, A); // C = target col
    a.ld_a_nn(PY).ld_r_r(B, A); // B = current row
    a.call("cell_empty"); // Z if GRID[B,C] empty
    a.jr(NZ_JR, "in_right"); // blocked
    a.call("erase_active");
    a.ld_a_nn(PX).dec_r(A).ld_nn_a(PX);
    a.call("draw_active");
    a.ld_a(MOVE_EVERY).ld_nn_a(MOVET);
    a.ret();

    a.label("in_right");
    // re-read bits (B may be clobbered by calls above only on the left branch; on
    // this path B is still valid, but re-read to be safe and cheap)
    a.ld_a(0x20).ldh_to(0x00).ldh_from(0x00).ldh_from(0x00);
    a.ld_r_r(B, A);
    a.bit(0, B).jr(NZ_JR, "in_done"); // RIGHT bit0
    a.ld_a_nn(PX).cp(COLS - 1).jr(NC_JR, "in_done"); // at right wall
    a.inc_r(A).ld_r_r(C, A); // C = target col
    a.ld_a_nn(PY).ld_r_r(B, A); // B = row
    a.call("cell_empty");
    a.jr(NZ_JR, "in_done"); // blocked
    a.call("erase_active");
    a.ld_a_nn(PX).inc_r(A).ld_nn_a(PX);
    a.call("draw_active");
    a.ld_a(MOVE_EVERY).ld_nn_a(MOVET);
    a.label("in_done");
    a.ret();

    // ===================== fall_one: advance the active block one row =====================
    a.label("fall_one");
    // if already on the last row -> settle here
    a.ld_a_nn(PY).cp(ROWS - 1).jr(Z_JR, "fo_settle");
    // probe the cell directly below
    a.ld_a_nn(PY).inc_r(A).ld_r_r(B, A); // B = row+1
    a.ld_a_nn(PX).ld_r_r(C, A); // C = col
    a.call("cell_empty"); // Z if empty below
    a.jr(NZ_JR, "fo_settle"); // not empty -> settle
    // move down one row
    a.call("erase_active");
    a.ld_a_nn(PY).inc_r(A).ld_nn_a(PY);
    a.call("draw_active");
    a.ret();

    a.label("fo_settle");
    // write the block into the grid mirror at (PY,PX)
    a.ld_a_nn(PY).ld_r_r(B, A);
    a.ld_a_nn(PX).ld_r_r(C, A);
    a.call("cell_addr"); // HL = &GRID[B,C]
    a.ld_hl_imm(T_BLOCK);
    a.call("land_snd");
    a.call("clear_rows"); // clear any full rows, bump score, redraw well
    a.call("spawn"); // next block (sets STATE=2 on game over)
    a.ret();

    // ===================== spawn: drop a new block at top-centre =====================
    a.label("spawn");
    a.ld_a(COLS / 2).ld_nn_a(PX); // centre column
    a.xor_aa().ld_nn_a(PY); // top row
    // game over if the spawn cell is already occupied
    a.xor_aa().ld_r_r(B, A); // row 0
    a.ld_a(COLS / 2).ld_r_r(C, A);
    a.call("cell_empty");
    a.jr(Z_JR, "sp_ok"); // empty -> fine
    a.call("gameover");
    a.ret();
    a.label("sp_ok");
    a.call("draw_active"); // show the new block
    a.ret();

    // ===================== clear_rows: remove full rows, shift down, score =====================
    // Scan from the bottom row up. For each full row, copy every row above it down
    // by one and bump the score. Re-test the same row index after a shift (a new
    // row has dropped into it). Finally repaint the whole well from the grid.
    a.label("clear_rows");
    a.ld_a(ROWS - 1).ld_nn_a(0xC00B); // CR_ROW scratch = bottom row
    a.label("cr_loop");
    // if CR_ROW underflowed (0xFF) we're done
    a.ld_a_nn(0xC00B).cp(0xFF).jr(Z_JR, "cr_done");
    // test whether row CR_ROW is completely filled
    a.ld_a_nn(0xC00B).ld_r_r(B, A);
    a.call("row_full"); // Z if row full
    a.jr(NZ_JR, "cr_next"); // not full -> move up a row
    // FULL: score++ and shift everything above down by one
    a.call("score_inc");
    a.call("clear_snd");
    a.ld_a_nn(0xC00B).ld_r_r(B, A); // B = the full row
    a.call("shift_down"); // rows 0..B move into 1..B (row 0 becomes empty)
    a.jra("cr_loop"); // re-test the SAME row (something fell into it)
    a.label("cr_next");
    a.ld_a_nn(0xC00B).dec_r(A).ld_nn_a(0xC00B); // CR_ROW--
    a.jra("cr_loop");
    a.label("cr_done");
    a.call("redraw_well"); // repaint all cells from the grid mirror
    a.ret();

    // ===================== row_full: Z if GRID row B is all non-empty =====================
    a.label("row_full");
    a.ld_r_n(C, 0); // C = col
    a.label("rfu_loop");
    a.call("cell_empty"); // Z if GRID[B,C] empty
    a.jr(Z_JR, "rfu_no"); // found an empty -> not full (return NZ)
    a.inc_r(C).ld_r_r(A, C).cp(COLS).jr(C_JR, "rfu_loop");
    // all cols filled -> return Z (set Z via cp equal)
    a.xor_aa(); // Z=1
    a.ret();
    a.label("rfu_no");
    a.ld_a(1).or_a(0); // NZ
    a.ret();

    // ===================== shift_down: rows 0..B-1 move down into 1..B =====================
    // For r from B down to 1: GRID[r] = GRID[r-1]. Then GRID[0] = empty.
    a.label("shift_down");
    a.ld_r_r(A, B).ld_nn_a(0xC00C); // SD_R scratch = B
    a.label("sd_loop");
    a.ld_a_nn(0xC00C).or_a(0).jr(Z_JR, "sd_top"); // r==0 -> done shifting
    // copy row (r-1) into row r, col by col
    a.ld_r_n(C, 0);
    a.label("sd_col");
    // src = GRID[(r-1), C]
    a.ld_a_nn(0xC00C).dec_r(A).ld_r_r(B, A);
    a.call("cell_addr"); // HL = &GRID[r-1, C]
    a.ld_a_hl().ld_r_r(D, A); // D = src tile
    // dst = GRID[r, C]
    a.ld_a_nn(0xC00C).ld_r_r(B, A);
    a.call("cell_addr"); // HL = &GRID[r, C]
    a.ld_r_r(A, D).ld_hl_a(); // GRID[r,C] = src
    a.inc_r(C).ld_r_r(A, C).cp(COLS).jr(C_JR, "sd_col");
    a.ld_a_nn(0xC00C).dec_r(A).ld_nn_a(0xC00C); // r--
    a.jra("sd_loop");
    a.label("sd_top");
    // clear row 0
    a.ld_r_n(C, 0);
    a.label("sd_clr");
    a.xor_aa().ld_r_r(B, A); // row 0
    a.call("cell_addr");
    a.ld_hl_imm(T_EMPTY);
    a.inc_r(C).ld_r_r(A, C).cp(COLS).jr(C_JR, "sd_clr");
    a.ret();

    // ===================== redraw_well: repaint every cell from the grid =====================
    a.label("redraw_well");
    a.ld_r_n(B, 0); // B = row
    a.label("rw_row");
    a.ld_r_n(C, 0); // C = col
    a.label("rw_col");
    a.push_bc();
    a.call("cell_addr"); // HL = &GRID[B,C]
    a.ld_a_hl().ld_r_r(D, A); // D = tile value (0 or T_BLOCK)
    a.call("map_addr"); // HL = map addr for (B,C)
    a.ld_r_r(A, D).ld_hl_a(); // write tile to VRAM
    a.pop_bc();
    a.inc_r(C).ld_r_r(A, C).cp(COLS).jr(C_JR, "rw_col");
    a.inc_r(B).ld_r_r(A, B).cp(ROWS).jr(C_JR, "rw_row");
    a.ret();

    // ===================== score_inc: ripple D0/D1/D2, speed up, redraw =====================
    a.label("score_inc");
    a.ld_a_nn(D0).inc_r(A).cp(10).jr(C_JR, "si_d0");
    a.xor_aa().ld_nn_a(D0);
    a.ld_a_nn(D1).inc_r(A).cp(10).jr(C_JR, "si_d1");
    a.xor_aa().ld_nn_a(D1);
    a.ld_a_nn(D2).inc_r(A).cp(10).jr(C_JR, "si_d2");
    a.ld_a(9).ld_nn_a(D2); // clamp at 999
    a.jra("si_speed");
    a.label("si_d2");
    a.ld_nn_a(D2);
    a.jra("si_speed");
    a.label("si_d1");
    a.ld_nn_a(D1);
    a.jra("si_speed");
    a.label("si_d0");
    a.ld_nn_a(D0);
    a.label("si_speed");
    // speed up: STEP-- unless already at the floor
    a.ld_a_nn(STEP).cp(STEP_FLOOR + 1).jr(C_JR, "si_draw");
    a.dec_r(A).ld_nn_a(STEP);
    a.label("si_draw");
    a.call("draw_score");
    a.ret();

    // ===================== draw_score: header label + three digits =====================
    a.label("draw_score");
    a.print(0x9800 + 9, "SCORE");
    a.ld_a_nn(D2).add_a(DIGIT0).ld_nn_a(SC_H_CELL);
    a.ld_a_nn(D1).add_a(DIGIT0).ld_nn_a(SC_T_CELL);
    a.ld_a_nn(D0).add_a(DIGIT0).ld_nn_a(SC_O_CELL);
    a.ret();

    // ===================== gameover: stop play, show the over screen =====================
    a.label("gameover");
    a.tone(700, 0xF2, 0x80); // low descending-ish thud
    a.ld_a(2).ld_nn_a(STATE);
    // overlay GAME OVER text + the final score; the well stays painted underneath.
    a.print(0x9800 + 6 * 32 + 6, "GAME OVER");
    a.print(0x9800 + 9 * 32 + 4, "PRESS START");
    a.ret();

    // ===================== land_snd / clear_snd =====================
    a.label("land_snd");
    a.tone(1100, 0xF1, 0x80); // short tick on land
    a.ret();
    a.label("clear_snd");
    a.tone(1700, 0xF3, 0x80); // bright chime on a clear
    a.ret();

    // ===================== title_draw / title_tick helpers =====================
    a.label("title_draw");
    a.xor_aa().ldh_to(0x40); // LCD off to rebuild the map
    a.memset(0x9800, 0x00, 0x0400);
    a.print(0x9800 + 5 * 32 + 7, "BLOCKS");
    a.print(0x9800 + 8 * 32 + 4, "PRESS START");
    a.print(0x9800 + 12 * 32 + 3, "LEFT-RIGHT-DOWN");
    a.ld_a(0x91).ldh_to(0x40); // LCD on
    a.ret();

    // ===================== start_edge: A=1 on a fresh START press =====================
    a.label("start_edge");
    a.ld_a(0x10).ldh_to(0x00).ldh_from(0x00).ldh_from(0x00); // select buttons
    a.bit(3, A).jr(NZ_JR, "se_up"); // START is bit3, 0 = pressed
    // START currently down. Was it down last frame?
    a.ld_a_nn(PREVST).or_a(0).jr(NZ_JR, "se_held");
    a.ld_a(1).ld_nn_a(PREVST); // remember held
    a.ld_a(1); // -> fresh press
    a.ret();
    a.label("se_held");
    a.xor_aa(); // already held -> no edge
    a.ret();
    a.label("se_up");
    a.xor_aa().ld_nn_a(PREVST); // released
    a.ret();

    // ===================== cell_empty: Z if GRID[B,C] == 0 =====================
    // Computes &GRID[B*COLS + C], loads it, sets flags. Preserves B and C.
    a.label("cell_empty");
    a.call("cell_addr");
    a.ld_a_hl().or_a(0);
    a.ret();

    // ===================== cell_addr: HL = &GRID[B*COLS + C], preserves B,C =====================
    a.label("cell_addr");
    a.ld_hl(GRID);
    a.ld_de(COLS as u16);
    a.ld_r_r(A, B).or_a(0).jr(Z_JR, "ca_col"); // row 0 -> skip
    a.push_bc();
    a.label("ca_row");
    a.add_hl_de();
    a.dec_r(B).ld_r_r(A, B).or_a(0).jr(NZ_JR, "ca_row");
    a.pop_bc();
    a.label("ca_col");
    a.ld_r_r(E, C).ld_r_n(D, 0).add_hl_de(); // + col
    a.ret();

    // ===================== map_addr: HL = VRAM map addr for (B=row, C=col) =====================
    // map = 0x9800 + (ORIGIN_ROW+B)*32 + (ORIGIN_COL+C). Preserves B,C.
    a.label("map_addr");
    a.ld_hl(0x9800 + (ORIGIN_ROW as u16) * 32 + ORIGIN_COL as u16);
    a.ld_de(32);
    a.ld_r_r(A, B).or_a(0).jr(Z_JR, "ma_col");
    a.push_bc();
    a.label("ma_row");
    a.add_hl_de();
    a.dec_r(B).ld_r_r(A, B).or_a(0).jr(NZ_JR, "ma_row");
    a.pop_bc();
    a.label("ma_col");
    a.ld_r_r(E, C).ld_r_n(D, 0).add_hl_de();
    a.ret();

    // ===================== draw_active / erase_active =====================
    // Paint the active block (PY,PX) to VRAM as a BLOCK / erase it back to EMPTY.
    a.label("draw_active");
    a.ld_a_nn(PY).ld_r_r(B, A);
    a.ld_a_nn(PX).ld_r_r(C, A);
    a.call("map_addr");
    a.ld_hl_imm(T_BLOCK);
    a.ret();
    a.label("erase_active");
    a.ld_a_nn(PY).ld_r_r(B, A);
    a.ld_a_nn(PX).ld_r_r(C, A);
    a.call("map_addr");
    a.ld_hl_imm(T_EMPTY);
    a.ret();

    // ===================== draw_border: paint the well walls into the map =====================
    // Left wall at map col (ORIGIN_COL-1), right wall at (ORIGIN_COL+COLS), and a
    // floor row under the well. Drawn once per game (the map is static otherwise).
    a.label("draw_border");
    a.ld_r_n(B, 0); // B = well row index 0..ROWS-1
    a.label("db_row");
    a.push_bc();
    // left wall cell: map_addr(B,0) - 1
    a.ld_r_n(C, 0);
    a.call("map_addr");
    a.dec_hl(); // one column left of the well
    a.ld_hl_imm(T_WALL);
    a.pop_bc();
    a.push_bc();
    // right wall cell: map_addr(B, COLS-1) + 1
    a.ld_r_n(C, COLS - 1);
    a.call("map_addr");
    a.inc_hl(); // one column right of the well
    a.ld_hl_imm(T_WALL);
    a.pop_bc();
    a.inc_r(B).ld_r_r(A, B).cp(ROWS).jr(C_JR, "db_row");
    // floor row beneath the well
    a.ld_r_n(C, 0);
    a.label("db_floor");
    a.push_bc();
    a.ld_r_n(B, ROWS - 1);
    a.call("map_addr"); // (last row, C)
    a.ld_de(32);
    a.add_hl_de(); // one row below the well
    a.ld_hl_imm(T_WALL);
    a.pop_bc();
    a.inc_r(C).ld_r_r(A, C).cp(COLS).jr(C_JR, "db_floor");
    // also cap the two bottom corners (left/right of the floor)
    a.ret();

    // ===================== tile data: 3 art tiles (0..2) =====================
    a.label("TILES");
    a.raw(&[0x00; 16]); // 0 EMPTY
    a.raw(&block_tile()); // 1 BLOCK (bevelled solid)
    a.raw(&wall_tile()); // 2 WALL (hatched)

    // ===================== font data (placed ONCE) =====================
    a.label("FONT");
    a.raw(&font_blob());

    // ===================== emit ROM =====================
    let rom = a.build_rom("BLOCKS");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/blocks.gb", &rom).unwrap();
    println!("wrote web/blocks.gb ({} bytes code+data at $0150)", a.c.len());
}

// A bevelled solid block: full body (color 3) with a light top/left highlight
// pattern so settled blocks read as distinct cells.
fn block_tile() -> [u8; 16] {
    // rows: top edge solid, body solid with a 1px gap on the right & bottom for a
    // grid look.
    let body: u8 = 0xFE; // 7px wide (gap on the right)
    let mut t = [0u8; 16];
    for i in 0..8 {
        let row = if i == 7 { 0x00 } else { body };
        t[i * 2] = row;
        t[i * 2 + 1] = row;
    }
    t
}

// Wall tile: a dense hatch at color 2 (set the high plane only).
fn wall_tile() -> [u8; 16] {
    let mut t = [0u8; 16];
    for i in 0..8 {
        let row = if i % 2 == 0 { 0xFF } else { 0xAA };
        t[i * 2] = 0x00; // low plane 0
        t[i * 2 + 1] = row; // high plane -> color 2
    }
    t
}
