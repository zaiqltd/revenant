//! Builds an ORIGINAL homebrew Game Boy game — BREAKOUT — and writes web/breakout.gb.
//!
//! A paddle at the bottom (two 8x8 sprites = 16px wide) you slide left/right with
//! the D-pad, a bouncing ball (one 8x8 sprite), and a wall of bricks (BG tiles in
//! the top rows of the $9800 map). Clear every brick to WIN (a fresh wall respawns);
//! let the ball fall past the paddle to LOSE (the board restarts). Hand-assembled
//! SM83 via the shared two-pass assembler. No copyrighted content — REVENANT skips
//! the DMG boot ROM and runs from $0100.
//!
//!   cargo run --release --example makebreakout   ->  web/breakout.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- WRAM layout ($C000.. is free) ----
const BALLX: u16 = 0xC000; // ball sprite X in OAM coords (screen X = BALLX-8)
const BALLY: u16 = 0xC001; // ball sprite Y in OAM coords (screen Y = BALLY-16)
const DX: u16 = 0xC002; //    ball X velocity (0x01 or 0xFF)
const DY: u16 = 0xC003; //    ball Y velocity (0x01 or 0xFF)
const PADX: u16 = 0xC004; //  paddle LEFT sprite X in OAM coords
const BRICKS: u16 = 0xC005; // bricks remaining (0..64)
const FRAME: u16 = 0xC006; //  frame counter (RNG-ish / unused gate spare)

// Geometry constants (OAM coords).
const PAD_Y: u8 = 144; //  paddle sprite Y (screen Y = 128)
const PAD_MIN: u8 = 8; //  paddle left clamp (screen X = 0)
const PAD_MAX: u8 = 144; // paddle left clamp (screen X = 136, right edge 152)
const PAD_W: u8 = 16; //   paddle width in px

const BRICK_TILE: u8 = 3;
const BALL_TILE: u8 = 1;
const PAD_TILE: u8 = 2;

// Brick wall geometry in MAP cells ($9800 + row*32 + col).
const BR_ROW0: u8 = 2; //  first brick map row
const BR_ROWS: u8 = 4; //  number of brick rows
const BR_COL0: u8 = 2; //  first brick map col
const BR_COLS: u8 = 16; // number of brick cols (2..18, fits 20-wide screen)

// SRL A (logical shift right) — not in the shared API, emit raw.
fn srl_a(a: &mut Asm) { a.raw(&[0xCB, 0x3F]); }

fn main() {
    let mut a = Asm::new();

    // ===================== one-time setup =====================
    a.label("main");
    a.di();
    a.xor_aa();
    a.ldh_to(0x40); // LCDC = 0 -> LCD off (safe to write VRAM/OAM)

    // Copy tile data (4 tiles * 16 bytes) ROM -> $8000.
    a.memcpy_lbl("TILES", 0x8000, 4 * 16);

    // Clear all 40 OAM sprites (set Y=0 -> off-screen).
    a.memset(0xFE00, 0x00, 160);

    // Palettes: BGP / OBP0 = 0xE4 (shades 3,2,1,0).
    a.ld_a(0xE4); a.ldh_to(0x47); // BGP
    a.ld_a(0xE4); a.ldh_to(0x48); // OBP0

    // Seed FRAME from DIV (light entropy for nothing critical; kept for variety).
    a.ldh_from(0x04); a.ld_nn_a(FRAME);

    // ===================== (re)start a board =====================
    a.label("restart");
    a.xor_aa(); a.ldh_to(0x40); // LCD off while we rebuild the map

    // Clear the whole 32x32 BG map to tile 0 (blank).
    a.memset(0x9800, 0x00, 0x0400);

    // Paint the brick wall and count bricks.
    a.call("makebricks");

    // Ball: start near the paddle, heading up-right.
    a.ld_a(80); a.ld_nn_a(BALLX);  // screen X = 72
    a.ld_a(120); a.ld_nn_a(BALLY); // screen Y = 104
    a.ld_a(0x01); a.ld_nn_a(DX);   // dx = +1
    a.ld_a(0xFF); a.ld_nn_a(DY);   // dy = -1 (up)

    // Paddle: centred. left screen X = 72.
    a.ld_a(80); a.ld_nn_a(PADX);

    // LCD on: LCDC = 0x93 (LCD + tiledata@8000 + OBJ on + BG on).
    a.ld_a(0x93); a.ldh_to(0x40);

    // ===================== main loop =====================
    a.label("loop");
    a.wait_vblank();
    a.call("input");   // move paddle (clamped)
    a.call("moveball"); // advance ball, all collisions, win/lose
    a.call("draw");     // write OAM for ball + paddle
    a.jpa("loop");

    // ===================== input: D-pad moves paddle =====================
    a.label("input");
    a.ld_a(0x20); a.ldh_to(0x00); a.ldh_from(0x00); // select dirs, read
    a.ld_r_r(B, A); // b = dir bits (0 = pressed)
    // LEFT (bit1): move paddle left, clamp at PAD_MIN.
    a.bit(1, B); a.jr(NZ_JR, "in_r");
    a.ld_a_nn(PADX); a.cp(PAD_MIN + 2); a.jr(C_JR, "in_r"); // already at/under min -> skip
    a.sub_a(2); a.ld_nn_a(PADX);
    a.label("in_r");
    // RIGHT (bit0): move paddle right, clamp at PAD_MAX.
    a.bit(0, B); a.jr(NZ_JR, "in_done");
    a.ld_a_nn(PADX); a.cp(PAD_MAX - 1); a.jr(NC_JR, "in_done"); // at/over max -> skip
    a.add_a(2); a.ld_nn_a(PADX);
    a.label("in_done");
    a.ret();

    // ===================== moveball: physics + collisions =====================
    a.label("moveball");
    // X += dx
    a.ld_a_nn(BALLX); a.ld_r_r(B, A); a.ld_a_nn(DX); a.add_r(B); a.ld_nn_a(BALLX);
    // Y += dy
    a.ld_a_nn(BALLY); a.ld_r_r(B, A); a.ld_a_nn(DY); a.add_r(B); a.ld_nn_a(BALLY);

    // --- left wall: if BALLX <= 8 -> dx = +1 ---
    a.ld_a_nn(BALLX); a.cp(9); a.jr(NC_JR, "mb_r");
    a.ld_a(0x01); a.ld_nn_a(DX);
    a.label("mb_r");
    // --- right wall: if BALLX >= 160 -> dx = -1 ---
    a.ld_a_nn(BALLX); a.cp(160); a.jr(C_JR, "mb_top");
    a.ld_a(0xFF); a.ld_nn_a(DX);
    a.label("mb_top");
    // --- top wall: if BALLY <= 16 -> dy = +1 ---
    a.ld_a_nn(BALLY); a.cp(17); a.jr(NC_JR, "mb_brick");
    a.ld_a(0x01); a.ld_nn_a(DY);

    // --- brick collision ---
    a.label("mb_brick");
    a.call("ballcell");        // HL = map addr of ball-centre cell, A = tile there
    a.or_a(0); a.jr(Z_JR, "mb_pad"); // empty -> no brick
    // hit a brick: erase it, flip dy, decrement count, maybe win.
    a.ld_hl_imm(0x00);         // (HL) = 0 (erase brick)
    a.call("flipdy");
    a.ld_a_nn(BRICKS); a.dec_r(A); a.ld_nn_a(BRICKS);
    a.jr(NZ_JR, "mb_pad");
    a.jpa("restart");          // all bricks gone -> WIN -> fresh board

    // --- paddle / lose ---
    a.label("mb_pad");
    // only relevant when ball is at/below the paddle line.
    // paddle screen-top Y = PAD_Y-16 = 128; ball bottom touches at screenY+8>=128
    // => BALLY >= 136. Use 136 as the trigger.
    a.ld_a_nn(BALLY); a.cp(136); a.jr(C_JR, "mb_done"); // BALLY < 136 -> nothing
    // ball is low. Only bounce if moving DOWN (dy == +1).
    a.ld_a_nn(DY); a.cp(0x01); a.jr(NZ_JR, "mb_lose_chk");
    // check horizontal overlap with paddle (forgiving by a couple px).
    // ball centre screen X = BALLX - 4 ; paddle spans [PADX-8 .. PADX-8+PAD_W].
    // overlap if (BALLX-4) >= (PADX-8 - 2) and (BALLX-4) <= (PADX-8 + PAD_W + 2).
    a.ld_a_nn(BALLX); a.sub_a(4); a.ld_r_r(C, A); // C = ball centre screen X
    a.ld_a_nn(PADX); a.sub_a(8 + 2);              // A = paddle left edge - 2 (low bound)
    a.ld_r_r(B, A); a.ld_r_r(A, C); a.sub_r(B);   // A = centre - lowbound
    a.jr(C_JR, "mb_lose_chk");                     // centre < lowbound -> miss left
    // upper bound: centre - lowbound must be <= (PAD_W + 4)
    a.cp(PAD_W + 4 + 1); a.jr(NC_JR, "mb_lose_chk"); // too far right -> miss
    // HIT the paddle: bounce up, nudge ball above the line so it can't stick.
    a.ld_a(0xFF); a.ld_nn_a(DY);
    a.ld_a(135); a.ld_nn_a(BALLY);
    a.jra("mb_done");

    a.label("mb_lose_chk");
    // missed the paddle: if the ball has fully dropped past it -> LOSE.
    a.ld_a_nn(BALLY); a.cp(154); a.jr(C_JR, "mb_done"); // still on-screen above 154
    a.jpa("restart"); // ball lost -> restart the board

    a.label("mb_done");
    a.ret();

    // ===================== flipdy: DY = -DY =====================
    a.label("flipdy");
    a.ld_a_nn(DY); a.cpl(); a.inc_r(A); a.ld_nn_a(DY); // two's complement negate
    a.ret();

    // ===================== ballcell: HL = map cell of ball centre, A = (HL) ====
    // ball centre screen: cx = BALLX - 8 + 4 = BALLX - 4 ; cy = BALLY - 16 + 4 = BALLY - 12
    // col = cx >> 3 ; row = cy >> 3 ; HL = 0x9800 + row*32 + col
    a.label("ballcell");
    // C = col
    a.ld_a_nn(BALLX); a.sub_a(4); srl_a(&mut a); srl_a(&mut a); srl_a(&mut a);
    a.ld_r_r(C, A);
    // B = row
    a.ld_a_nn(BALLY); a.sub_a(12); srl_a(&mut a); srl_a(&mut a); srl_a(&mut a);
    a.ld_r_r(B, A);
    // HL = 0x9800 + row*32  (row*32 = row<<5)
    a.ld_hl(0x9800);
    a.ld_de(32);
    a.label("bc_loop");
    a.ld_r_r(A, B); a.or_a(0); a.jr(Z_JR, "bc_col"); // row==0 done
    a.add_hl_de(); a.dec_r(B); a.jra("bc_loop");
    a.label("bc_col");
    // HL += col  (col is small, add via DE)
    a.ld_r_r(E, C); a.ld_r_n(D, 0);
    a.add_hl_de();
    a.ld_a_hl(); // A = tile at the cell
    a.ret();

    // ===================== makebricks: paint wall + set BRICKS count ==========
    a.label("makebricks");
    a.ld_a(BR_ROWS * BR_COLS); a.ld_nn_a(BRICKS);
    // outer rows: D = row index counter (BR_ROWS), start map row BR_ROW0
    a.ld_r_n(D, BR_ROWS);
    a.ld_r_n(B, BR_ROW0); // B = current map row
    a.label("mk_row");
    a.push_de();
    // HL = 0x9800 + B*32 + BR_COL0  via the cell helper inline:
    a.ld_hl(0x9800);
    a.ld_de(32);
    a.ld_r_r(A, B); // A = row
    a.label("mk_rowadv");
    a.or_a(0); a.jr(Z_JR, "mk_rowdone");
    a.add_hl_de(); a.dec_r(A); a.jra("mk_rowadv");
    a.label("mk_rowdone");
    // HL += BR_COL0
    a.ld_de(BR_COL0 as u16);
    a.add_hl_de();
    // write BR_COLS brick tiles across
    a.ld_r_n(C, BR_COLS);
    a.label("mk_col");
    a.ld_a(BRICK_TILE); a.ldi_hl_a(); // (HL)=brick ; HL++
    a.dec_r(C); a.ld_r_r(A, C); a.or_a(0); a.jr(NZ_JR, "mk_col");
    a.pop_de();
    a.inc_r(B);          // next map row
    a.dec_r(D); a.ld_r_r(A, D); a.or_a(0); a.jr(NZ_JR, "mk_row");
    a.ret();

    // ===================== draw: write OAM for ball + paddle ==================
    a.label("draw");
    // sprite 0 = ball: [Y, X, tile, attr]
    a.ld_a_nn(BALLY); a.ld_nn_a(0xFE00);
    a.ld_a_nn(BALLX); a.ld_nn_a(0xFE01);
    a.ld_a(BALL_TILE); a.ld_nn_a(0xFE02);
    a.xor_aa(); a.ld_nn_a(0xFE03);
    // sprite 1 = paddle left
    a.ld_a(PAD_Y); a.ld_nn_a(0xFE04);
    a.ld_a_nn(PADX); a.ld_nn_a(0xFE05);
    a.ld_a(PAD_TILE); a.ld_nn_a(0xFE06);
    a.xor_aa(); a.ld_nn_a(0xFE07);
    // sprite 2 = paddle right (PADX + 8)
    a.ld_a(PAD_Y); a.ld_nn_a(0xFE08);
    a.ld_a_nn(PADX); a.add_a(8); a.ld_nn_a(0xFE09);
    a.ld_a(PAD_TILE); a.ld_nn_a(0xFE0A);
    a.xor_aa(); a.ld_nn_a(0xFE0B);
    a.ret();

    // ===================== tile data (4 tiles) =====================
    a.label("TILES");
    a.raw(&[0x00; 16]);                       // 0: blank background
    // 1: ball — a filled disc (color 3)
    a.raw(&solid_tile([0x3C, 0x7E, 0xFF, 0xFF, 0xFF, 0xFF, 0x7E, 0x3C]));
    // 2: paddle — solid bar with a 1px top/bottom rim (color 3)
    a.raw(&solid_tile([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]));
    // 3: brick — solid block with a dark mortar gap on right/bottom edges (color 2
    //    on the gaps gives a tiled masonry look). Use color-3 body with cutouts.
    a.raw(&brick_tile());

    // ===================== finalize =====================
    let rom = a.build_rom("BREAKOUT");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/breakout.gb", &rom).unwrap();
    println!("wrote web/breakout.gb ({} bytes code+data at $0150)", a.c.len());
}

// Brick tile: color-3 body, with the rightmost column and bottom row left as
// color 0 to draw "mortar" gaps so the wall reads as separate bricks.
fn brick_tile() -> [u8; 16] {
    let body: u8 = 0xFE; // 7 px wide (leave rightmost pixel as gap)
    let mut t = [0u8; 16];
    for i in 0..8 {
        let row = if i == 7 { 0x00 } else { body };
        t[i * 2] = row;
        t[i * 2 + 1] = row;
    }
    t
}
