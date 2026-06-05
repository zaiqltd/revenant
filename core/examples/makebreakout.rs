//! Builds an ORIGINAL homebrew Game Boy game — BREAKOUT — and writes web/breakout.gb.
//!
//! A paddle at the bottom (two 8x8 sprites = 16px wide) you slide left/right with
//! the D-pad, a bouncing ball (one 8x8 sprite), and a wall of bricks (BG tiles in
//! the top rows of the $9800 map). Break every brick to WIN; let the ball fall past
//! the paddle to LOSE. A proper TITLE screen, an on-screen SCORE, sound effects on
//! every bounce / break / death, and a GAME OVER / YOU WIN screen with the score and
//! "PRESS START" to retry. Hand-assembled SM83 via the shared two-pass assembler.
//! No copyrighted content — REVENANT skips the DMG boot ROM and runs from $0100.
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
const FRAME: u16 = 0xC006; //  frame counter (gates ball speed)
const STATE: u16 = 0xC007; //  0=title 1=play 2=gameover 3=win
const SCORE: u16 = 0xC008; //  bricks broken so far (0..64)
const LASTSTART: u16 = 0xC009; // previous frame's Start bit (edge detect)

// Geometry constants (OAM coords).
const PAD_Y: u8 = 144; //  paddle sprite Y (screen Y = 128)
const PAD_MIN: u8 = 8; //  paddle left clamp (screen X = 0)
const PAD_MAX: u8 = 144; // paddle left clamp (screen X = 136, right edge 152)
const PAD_W: u8 = 16; //   paddle width in px

const BRICK_TILE: u8 = 3;
const BALL_TILE: u8 = 1;
const PAD_TILE: u8 = 2;

// Brick wall geometry in MAP cells ($9800 + row*32 + col).
const BR_ROW0: u8 = 3; //  first brick map row (leave row 1 for the score HUD)
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
    a.ld_sp(0xFFFE);
    a.xor_aa();
    a.ldh_to(0x40); // LCDC = 0 -> LCD off (safe to write VRAM/OAM)

    // Copy game-art tile data (4 tiles * 16 bytes) ROM -> $8000 (tiles 0..3).
    a.memcpy_lbl("TILES", 0x8000, 4 * 16);
    // Copy the font into tiles $20..$5F so print()/digits render.
    a.load_font();

    // Clear all 40 OAM sprites (set Y=0 -> off-screen).
    a.memset(0xFE00, 0x00, 160);

    // Palettes: BGP / OBP0 = 0xE4 (shades 3,2,1,0).
    a.ld_a(0xE4); a.ldh_to(0x47); // BGP
    a.ld_a(0xE4); a.ldh_to(0x48); // OBP0

    // Audio on.
    a.apu_on();

    // Seed FRAME from DIV; clear edge tracker.
    a.ldh_from(0x04); a.ld_nn_a(FRAME);
    a.xor_aa(); a.ld_nn_a(LASTSTART);

    // Begin on the TITLE screen.
    a.call("showtitle");

    // ===================== main loop =====================
    a.label("loop");
    a.wait_vblank();
    // dispatch on STATE
    a.ld_a_nn(STATE);
    a.or_a(0); a.jr(Z_JR, "st_title");      // 0
    a.cp(1); a.jr(Z_JR, "st_play");         // 1
    a.cp(2); a.jr(Z_JR, "st_over");         // 2
    a.jpa("st_win");                        // 3 (win)

    a.label("st_title");
    a.call("waitstart"); a.jr(NZ_JR, "loop_end"); // returns Z if start pressed
    a.call("startgame");
    a.jra("loop_end");

    a.label("st_play");
    a.call("input");
    a.call("moveball");
    // moveball may have ended the round (STATE != 1); if so, don't redraw the
    // play sprites/HUD over the fresh GAME OVER / YOU WIN screen.
    a.ld_a_nn(STATE); a.cp(1); a.jr(NZ_JR, "loop_end");
    a.call("draw");
    a.call("drawscore");
    a.jra("loop_end");

    a.label("st_over");
    a.call("waitstart"); a.jr(NZ_JR, "loop_end");
    a.call("showtitle");
    a.jra("loop_end");

    a.label("st_win");
    a.call("waitstart"); a.jr(NZ_JR, "loop_end");
    a.call("showtitle");

    a.label("loop_end");
    a.jpa("loop");

    // ===================== waitstart: Z if a NEW Start press happened =====
    // Reads Start, edge-detects vs LASTSTART. Returns with Z set on a fresh press.
    a.label("waitstart");
    a.ld_a(0x10); a.ldh_to(0x00); a.ldh_from(0x00); a.ldh_from(0x00);
    a.and_a(0x08);              // Start = bit3 (0 = pressed)
    a.ld_r_r(B, A);            // B = current Start bit (0 pressed, 8 released)
    a.ld_a_nn(LASTSTART);      // A = last frame's bit
    a.ld_r_r(C, A);
    a.ld_r_r(A, B); a.ld_nn_a(LASTSTART); // store current as last
    // fresh press = last was released (C==8) and current pressed (B==0)
    a.ld_r_r(A, B); a.or_a(0); a.jr(NZ_JR, "ws_no"); // current not pressed
    a.ld_r_r(A, C); a.cp(0x08); a.jr(NZ_JR, "ws_no"); // last already pressed
    a.xor_aa();                // set Z (A=0)
    a.ret();
    a.label("ws_no");
    a.ld_a(1); a.or_a(0);      // clear Z (A!=0)
    a.ret();

    // ===================== showtitle: paint TITLE screen, STATE=0 ==========
    a.label("showtitle");
    a.xor_aa(); a.ldh_to(0x40);        // LCD off
    a.memset(0x9800, 0x20, 0x0400);    // clear map to spaces
    a.memset(0xFE00, 0x00, 160);       // hide sprites
    a.print(0x9800 + 7 * 32 + 6, "BREAKOUT");
    a.print(0x9800 + 10 * 32 + 4, "PRESS START");
    a.ld_a(0); a.ld_nn_a(STATE);
    a.ld_a(0x93); a.ldh_to(0x40);      // LCD on (BG only needed, but OBJ fine)
    a.ret();

    // ===================== startgame: init board, STATE=1 =================
    a.label("startgame");
    a.xor_aa(); a.ldh_to(0x40);        // LCD off while we rebuild the map
    a.memset(0x9800, 0x20, 0x0400);    // clear map to spaces
    a.call("makebricks");              // paint wall + set BRICKS count
    // ball: start near the paddle, heading up-right.
    a.ld_a(80); a.ld_nn_a(BALLX);
    a.ld_a(120); a.ld_nn_a(BALLY);
    a.ld_a(0x01); a.ld_nn_a(DX);
    a.ld_a(0xFF); a.ld_nn_a(DY);
    a.ld_a(80); a.ld_nn_a(PADX);       // paddle centred
    a.xor_aa(); a.ld_nn_a(SCORE);      // score = 0
    a.ld_a(1); a.ld_nn_a(STATE);       // STATE = play
    // draw the HUD label once.
    a.print(0x9800 + 1 * 32 + 2, "SCORE-");
    a.ld_a(0x93); a.ldh_to(0x40);      // LCD on
    a.call("drawscore");
    a.ret();

    // ===================== input: D-pad moves paddle =====================
    a.label("input");
    a.ld_a(0x20); a.ldh_to(0x00); a.ldh_from(0x00); a.ldh_from(0x00);
    a.ld_r_r(B, A); // b = dir bits (0 = pressed)
    // LEFT (bit1): move paddle left, clamp at PAD_MIN.
    a.bit(1, B); a.jr(NZ_JR, "in_r");
    a.ld_a_nn(PADX); a.cp(PAD_MIN + 2); a.jr(C_JR, "in_r");
    a.sub_a(2); a.ld_nn_a(PADX);
    a.label("in_r");
    // RIGHT (bit0): move paddle right, clamp at PAD_MAX.
    a.bit(0, B); a.jr(NZ_JR, "in_done");
    a.ld_a_nn(PADX); a.cp(PAD_MAX - 1); a.jr(NC_JR, "in_done");
    a.add_a(2); a.ld_nn_a(PADX);
    a.label("in_done");
    a.ret();

    // ===================== moveball: physics + collisions =====================
    a.label("moveball");
    // X += dx
    a.ld_a_nn(BALLX); a.ld_r_r(B, A); a.ld_a_nn(DX); a.add_r(B); a.ld_nn_a(BALLX);
    // Y += dy
    a.ld_a_nn(BALLY); a.ld_r_r(B, A); a.ld_a_nn(DY); a.add_r(B); a.ld_nn_a(BALLY);

    // --- left wall: if BALLX <= 8 -> dx = +1, tick ---
    a.ld_a_nn(BALLX); a.cp(9); a.jr(NC_JR, "mb_r");
    a.ld_a(0x01); a.ld_nn_a(DX); a.call("sfx_tick");
    a.label("mb_r");
    // --- right wall: if BALLX >= 160 -> dx = -1, tick ---
    a.ld_a_nn(BALLX); a.cp(160); a.jr(C_JR, "mb_top");
    a.ld_a(0xFF); a.ld_nn_a(DX); a.call("sfx_tick");
    a.label("mb_top");
    // --- top wall: if BALLY <= 16 -> dy = +1, tick ---
    a.ld_a_nn(BALLY); a.cp(17); a.jr(NC_JR, "mb_brick");
    a.ld_a(0x01); a.ld_nn_a(DY); a.call("sfx_tick");

    // --- brick collision ---
    a.label("mb_brick");
    a.call("ballcell");        // HL = map addr of ball-centre cell, A = tile there
    a.cp(BRICK_TILE); a.jr(NZ_JR, "mb_pad"); // only react to a brick tile
    // hit a brick: erase it, flip dy, count, score, sfx, maybe win.
    a.ld_a(0x20); a.ld_hl_a();  // (HL) = space (erase brick)
    a.call("flipdy");
    a.call("sfx_break");
    a.ld_a_nn(SCORE); a.inc_r(A); a.ld_nn_a(SCORE);
    a.ld_a_nn(BRICKS); a.dec_r(A); a.ld_nn_a(BRICKS);
    a.jr(NZ_JR, "mb_pad");
    // all bricks gone -> WIN
    a.call("showwin");
    a.ret();

    // --- paddle / lose ---
    a.label("mb_pad");
    a.ld_a_nn(BALLY); a.cp(136); a.jr(C_JR, "mb_done"); // BALLY < 136 -> nothing
    // only bounce if moving DOWN (dy == +1).
    a.ld_a_nn(DY); a.cp(0x01); a.jr(NZ_JR, "mb_lose_chk");
    // check horizontal overlap with paddle (forgiving by a couple px).
    a.ld_a_nn(BALLX); a.sub_a(4); a.ld_r_r(C, A); // C = ball centre screen X
    a.ld_a_nn(PADX); a.sub_a(8 + 2);
    a.ld_r_r(B, A); a.ld_r_r(A, C); a.sub_r(B);
    a.jr(C_JR, "mb_lose_chk");
    a.cp(PAD_W + 4 + 1); a.jr(NC_JR, "mb_lose_chk");
    // HIT the paddle: bounce up, nudge above the line, tick.
    a.ld_a(0xFF); a.ld_nn_a(DY);
    a.ld_a(135); a.ld_nn_a(BALLY);
    a.call("sfx_tick");
    a.jra("mb_done");

    a.label("mb_lose_chk");
    a.ld_a_nn(BALLY); a.cp(154); a.jr(C_JR, "mb_done"); // still on-screen above 154
    // ball lost -> GAME OVER
    a.call("showover");
    a.label("mb_done");
    a.ret();

    // ===================== flipdy: DY = -DY =====================
    a.label("flipdy");
    a.ld_a_nn(DY); a.cpl(); a.inc_r(A); a.ld_nn_a(DY);
    a.ret();

    // ===================== sfx ============================================
    a.label("sfx_tick");  a.tone(1100, 0xF1, 0x80); a.ret();  // soft wall/paddle tick
    a.label("sfx_break"); a.tone(1850, 0xF3, 0x80); a.ret();  // bright brick blip
    a.label("sfx_lose");  a.tone(600, 0xF4, 0x80);  a.ret();  // low death tone

    // ===================== showover / showwin =============================
    a.label("showover");
    a.call("sfx_lose");
    a.xor_aa(); a.ldh_to(0x40);
    a.memset(0x9800, 0x20, 0x0400);
    a.memset(0xFE00, 0x00, 160);
    a.print(0x9800 + 6 * 32 + 5, "GAME OVER");
    a.print(0x9800 + 9 * 32 + 5, "SCORE-");
    a.call("drawscore_at_lose");
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.ld_a(2); a.ld_nn_a(STATE);
    a.ld_a(0x93); a.ldh_to(0x40);
    a.ret();

    a.label("showwin");
    a.tone(1950, 0xF3, 0x80); // win fanfare blip
    a.xor_aa(); a.ldh_to(0x40);
    a.memset(0x9800, 0x20, 0x0400);
    a.memset(0xFE00, 0x00, 160);
    a.print(0x9800 + 6 * 32 + 6, "YOU WIN");
    a.print(0x9800 + 9 * 32 + 5, "SCORE-");
    a.call("drawscore_at_lose");
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.ld_a(3); a.ld_nn_a(STATE);
    a.ld_a(0x93); a.ldh_to(0x40);
    a.ret();

    // draw SCORE as two decimal digits on the over/win screen (row9, col11).
    a.label("drawscore_at_lose");
    a.ld_de(0x9800 + 9 * 32 + 11);
    a.jra("score_digits");

    // ===================== drawscore: HUD two-digit decimal ===============
    a.label("drawscore");
    a.ld_de(0x9800 + 1 * 32 + 8); // after "SCORE-"
    a.label("score_digits");
    // DE = map addr of the tens digit. SCORE in 0..64 -> tens/ones.
    a.ld_a_nn(SCORE);
    a.ld_r_n(B, 0);            // B = tens
    a.label("ds_tens");
    a.cp(10); a.jr(C_JR, "ds_emit");
    a.sub_a(10); a.inc_r(B);
    a.jra("ds_tens");
    a.label("ds_emit");
    // A = ones, B = tens. Write '0'+tens then '0'+ones to DE, DE+1.
    a.ld_r_r(C, A);           // C = ones
    a.ld_r_r(A, B); a.add_a(0x30); // '0'+tens
    a.ld_r_r(L, E); a.ld_r_r(H, D); // HL = DE
    a.ldi_hl_a();             // (HL)=tens digit, HL++
    a.ld_r_r(A, C); a.add_a(0x30); // '0'+ones
    a.ld_hl_a();              // (HL)=ones digit
    a.ret();

    // ===================== ballcell: HL = map cell of ball centre, A = (HL) ====
    a.label("ballcell");
    a.ld_a_nn(BALLX); a.sub_a(4); srl_a(&mut a); srl_a(&mut a); srl_a(&mut a);
    a.ld_r_r(C, A);
    a.ld_a_nn(BALLY); a.sub_a(12); srl_a(&mut a); srl_a(&mut a); srl_a(&mut a);
    a.ld_r_r(B, A);
    a.ld_hl(0x9800);
    a.ld_de(32);
    a.label("bc_loop");
    a.ld_r_r(A, B); a.or_a(0); a.jr(Z_JR, "bc_col");
    a.add_hl_de(); a.dec_r(B); a.jra("bc_loop");
    a.label("bc_col");
    a.ld_r_r(E, C); a.ld_r_n(D, 0);
    a.add_hl_de();
    a.ld_a_hl();
    a.ret();

    // ===================== makebricks: paint wall + set BRICKS count ==========
    a.label("makebricks");
    a.ld_a(BR_ROWS * BR_COLS); a.ld_nn_a(BRICKS);
    a.ld_r_n(D, BR_ROWS);
    a.ld_r_n(B, BR_ROW0);
    a.label("mk_row");
    a.push_de();
    a.ld_hl(0x9800);
    a.ld_de(32);
    a.ld_r_r(A, B);
    a.label("mk_rowadv");
    a.or_a(0); a.jr(Z_JR, "mk_rowdone");
    a.add_hl_de(); a.dec_r(A); a.jra("mk_rowadv");
    a.label("mk_rowdone");
    a.ld_de(BR_COL0 as u16);
    a.add_hl_de();
    a.ld_r_n(C, BR_COLS);
    a.label("mk_col");
    a.ld_a(BRICK_TILE); a.ldi_hl_a();
    a.dec_r(C); a.ld_r_r(A, C); a.or_a(0); a.jr(NZ_JR, "mk_col");
    a.pop_de();
    a.inc_r(B);
    a.dec_r(D); a.ld_r_r(A, D); a.or_a(0); a.jr(NZ_JR, "mk_row");
    a.ret();

    // ===================== draw: write OAM for ball + paddle ==================
    a.label("draw");
    a.ld_a_nn(BALLY); a.ld_nn_a(0xFE00);
    a.ld_a_nn(BALLX); a.ld_nn_a(0xFE01);
    a.ld_a(BALL_TILE); a.ld_nn_a(0xFE02);
    a.xor_aa(); a.ld_nn_a(0xFE03);
    a.ld_a(PAD_Y); a.ld_nn_a(0xFE04);
    a.ld_a_nn(PADX); a.ld_nn_a(0xFE05);
    a.ld_a(PAD_TILE); a.ld_nn_a(0xFE06);
    a.xor_aa(); a.ld_nn_a(0xFE07);
    a.ld_a(PAD_Y); a.ld_nn_a(0xFE08);
    a.ld_a_nn(PADX); a.add_a(8); a.ld_nn_a(0xFE09);
    a.ld_a(PAD_TILE); a.ld_nn_a(0xFE0A);
    a.xor_aa(); a.ld_nn_a(0xFE0B);
    a.ret();

    // ===================== tile data (4 tiles) =====================
    a.label("TILES");
    a.raw(&[0x00; 16]);                       // 0: blank background
    a.raw(&solid_tile([0x3C, 0x7E, 0xFF, 0xFF, 0xFF, 0xFF, 0x7E, 0x3C])); // 1: ball
    a.raw(&solid_tile([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])); // 2: paddle
    a.raw(&brick_tile());                     // 3: brick

    // ===================== font data =====================
    a.label("FONT");
    a.raw(&font_blob());

    // ===================== finalize =====================
    let rom = a.build_rom("BREAKOUT");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/breakout.gb", &rom).unwrap();
    println!("wrote web/breakout.gb ({} bytes code+data at $0150)", a.c.len());
}

// Brick tile: color-3 body with mortar gaps on the right column / bottom row.
fn brick_tile() -> [u8; 16] {
    let body: u8 = 0xFE;
    let mut t = [0u8; 16];
    for i in 0..8 {
        let row = if i == 7 { 0x00 } else { body };
        t[i * 2] = row;
        t[i * 2 + 1] = row;
    }
    t
}
