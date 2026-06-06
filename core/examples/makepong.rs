//! Builds an ORIGINAL homebrew Game Boy game — PONG — and writes web/pong.gb.
//!
//! You control the LEFT paddle (D-pad Up/Down, clamped to the court). A simple AI
//! drives the RIGHT paddle: it chases the ball's Y a touch slower than the ball, so
//! it's beatable. A ball bounces between the paddles and off the top/bottom walls.
//! If the ball passes a paddle the other side scores. Both scores (YOU and CPU) are
//! shown on the top row via the bundled font. First to 7 wins -> WIN screen; if the
//! CPU reaches 7 -> GAME OVER. Sound: a tick on every bounce, a bright blip when YOU
//! score, a low tone when the CPU scores. Paddles and ball are sprites (art tiles
//! 0..31). Title screen, on-screen score, game-over with score + PRESS START retry.
//! Hand-assembled SM83 via the shared two-pass assembler. No copyrighted content —
//! REVENANT skips the DMG boot ROM and runs from $0100.
//!
//!   cargo run --release --example makepong   ->  web/pong.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- WRAM layout ($C000.. is free) ----
const BALLX: u16 = 0xC000; //  ball sprite X in OAM coords (screen X = BALLX-8)
const BALLY: u16 = 0xC001; //  ball sprite Y in OAM coords (screen Y = BALLY-16)
const DX: u16 = 0xC002; //     ball X velocity (signed: 0x02 / 0xFE)
const DY: u16 = 0xC003; //     ball Y velocity (signed: 0x02 / 0xFE)
const LPY: u16 = 0xC004; //    LEFT (player) paddle TOP sprite Y (OAM coords)
const RPY: u16 = 0xC005; //    RIGHT (CPU) paddle TOP sprite Y (OAM coords)
const SCOREL: u16 = 0xC006; // player score (0..7)
const SCORER: u16 = 0xC007; // CPU score (0..7)
const FRAME: u16 = 0xC008; //  frame counter (gates CPU paddle speed)
const STATE: u16 = 0xC009; //  0=title 1=play 2=gameover 3=win
const LASTSTART: u16 = 0xC00A; // previous frame's Start bit (edge detect)
const SERVEDIR: u16 = 0xC00B; // which way the next serve heads (toggles)

// Geometry (OAM coords; screen = X-8, Y-16).
const LP_X: u8 = 16; //   left paddle X (screen X = 8)
const RP_X: u8 = 152; //  right paddle X (screen X = 144)
const PAD_H: u8 = 16; //   paddle height in px (two 8px sprite tiles stacked)
const PAD_TOP: u8 = 24; // min paddle TOP Y (screen Y = 8, just under the HUD)
const PAD_BOT: u8 = 136; // max paddle TOP Y (screen Y = 120; bottom = 136)
const PAD_SPD: u8 = 3; //  player paddle speed (px/frame)

const BALL_TILE: u8 = 1;
const PAD_TILE: u8 = 2;

const BALL_MINY: u8 = 24; //  top wall (screen Y = 8)
const BALL_MAXY: u8 = 144; // bottom wall (screen Y = 128, ball 8px tall)

fn main() {
    let mut a = Asm::new();

    // ===================== one-time setup =====================
    a.label("main");
    a.di();
    a.ld_sp(0xFFFE);
    a.xor_aa();
    a.ldh_to(0x40); // LCDC = 0 -> LCD off (safe to write VRAM/OAM)

    // Copy game-art tile data (3 tiles * 16 bytes) ROM -> $8000 (tiles 0..2).
    a.memcpy_lbl("TILES", 0x8000, 3 * 16);
    // Copy the font into tiles $20..$5F so print()/digits render.
    a.load_font();

    // Clear all 40 OAM sprites (Y=0 -> off-screen).
    a.memset(0xFE00, 0x00, 160);

    // Palettes: BGP / OBP0 = 0xE4 (shades 3,2,1,0).
    a.ld_a(0xE4); a.ldh_to(0x47); // BGP
    a.ld_a(0xE4); a.ldh_to(0x48); // OBP0

    // Audio on.
    a.apu_on();

    // Seed FRAME from DIV; clear edge tracker; first serve heads right.
    a.ldh_from(0x04); a.ld_nn_a(FRAME);
    a.xor_aa(); a.ld_nn_a(LASTSTART);
    a.ld_a(0x02); a.ld_nn_a(SERVEDIR);

    // Begin on the TITLE screen.
    a.call("showtitle");

    // ===================== main loop =====================
    a.label("loop");
    a.wait_vblank();
    a.ld_a_nn(FRAME); a.inc_r(A); a.ld_nn_a(FRAME);

    a.ld_a_nn(STATE);
    a.or_a(0); a.jr(Z_JR, "st_title");      // 0
    a.cp(1); a.jr(Z_JR, "st_play");         // 1
    a.cp(2); a.jr(Z_JR, "st_over");         // 2
    a.jpa("st_win");                        // 3 (win)

    a.label("st_title");
    a.call("waitstart"); a.jr(NZ_JR, "loop_end"); // Z if a fresh Start press
    a.call("startgame");
    a.jra("loop_end");

    a.label("st_play");
    a.call("input");
    a.call("cpumove");
    a.call("moveball");
    // moveball may have ended the round (STATE != 1); if so don't redraw the play
    // sprites/HUD over the fresh GAME OVER / YOU WIN screen.
    a.ld_a_nn(STATE); a.cp(1); a.jr(NZ_JR, "loop_end");
    a.call("draw");
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
    a.label("waitstart");
    a.ld_a(0x10); a.ldh_to(0x00); a.ldh_from(0x00); a.ldh_from(0x00);
    a.and_a(0x08);             // Start = bit3 (0 = pressed)
    a.ld_r_r(B, A);           // B = current Start bit (0 pressed, 8 released)
    a.ld_a_nn(LASTSTART);
    a.ld_r_r(C, A);           // C = last frame's bit
    a.ld_r_r(A, B); a.ld_nn_a(LASTSTART); // store current as last
    a.ld_r_r(A, B); a.or_a(0); a.jr(NZ_JR, "ws_no"); // current not pressed
    a.ld_r_r(A, C); a.cp(0x08); a.jr(NZ_JR, "ws_no"); // last already pressed
    a.xor_aa();               // set Z (A=0) -> fresh press
    a.ret();
    a.label("ws_no");
    a.ld_a(1); a.or_a(0);     // clear Z (A!=0)
    a.ret();

    // ===================== showtitle: paint TITLE screen, STATE=0 ==========
    a.label("showtitle");
    a.xor_aa(); a.ldh_to(0x40);        // LCD off
    a.memset(0x9800, 0x20, 0x0400);    // clear map to spaces
    a.memset(0xFE00, 0x00, 160);       // hide sprites
    a.print(0x9800 + 4 * 32 + 8, "PONG");
    a.print(0x9800 + 7 * 32 + 3, "YOU VS CPU");
    a.print(0x9800 + 9 * 32 + 3, "FIRST TO 7");
    a.print(0x9800 + 11 * 32 + 2, "UP-DOWN TO MOVE");
    a.print(0x9800 + 14 * 32 + 4, "PRESS START");
    a.ld_a(0); a.ld_nn_a(STATE);
    a.ld_a(0x93); a.ldh_to(0x40);      // LCD on
    a.ret();

    // ===================== startgame: init match, STATE=1 =================
    a.label("startgame");
    a.xor_aa(); a.ldh_to(0x40);        // LCD off while we rebuild the map
    a.memset(0x9800, 0x20, 0x0400);    // clear map to spaces
    // Scores reset.
    a.xor_aa(); a.ld_nn_a(SCOREL);
    a.xor_aa(); a.ld_nn_a(SCORER);
    // Paddles centred (top Y so the 16px paddle is vertically centred ~ Y 80).
    a.ld_a(72); a.ld_nn_a(LPY);
    a.ld_a(72); a.ld_nn_a(RPY);
    a.ld_a(1); a.ld_nn_a(STATE);
    // HUD labels: "YOU" left, "CPU" right.
    a.print(0x9800 + 0 * 32 + 2, "YOU");
    a.print(0x9800 + 0 * 32 + 15, "CPU");
    a.call("serve");                   // place/launch the ball
    a.call("drawscore");
    a.ld_a(0x93); a.ldh_to(0x40);      // LCD on
    a.ret();

    // ===================== serve: centre ball, launch per SERVEDIR ==========
    a.label("serve");
    a.ld_a(80); a.ld_nn_a(BALLX);      // centre-ish
    // Vertical start mixed with DIV for a little variety.
    a.ldh_from(0x04); a.and_a(0x3F); a.add_a(56); a.ld_nn_a(BALLY);
    // Horizontal direction from SERVEDIR, then toggle it for next serve.
    a.ld_a_nn(SERVEDIR); a.ld_nn_a(DX);
    a.ld_a_nn(SERVEDIR); a.cpl(); a.inc_r(A); a.ld_nn_a(SERVEDIR); // negate -> alternate
    // Vertical direction: down (+2) if DIV bit0 set, else up.
    a.ldh_from(0x04); a.bit(0, A); a.jr(Z_JR, "sv_up");
    a.ld_a(0x02); a.ld_nn_a(DY); a.jra("sv_done");
    a.label("sv_up");
    a.ld_a(0xFE); a.ld_nn_a(DY);
    a.label("sv_done");
    a.ret();

    // ===================== input: D-pad moves LEFT paddle =================
    a.label("input");
    a.ld_a(0x20); a.ldh_to(0x00); a.ldh_from(0x00); a.ldh_from(0x00);
    a.ld_r_r(B, A); // b = dir bits (0 = pressed)
    // UP (bit2): move paddle up, clamp at PAD_TOP.
    a.bit(2, B); a.jr(NZ_JR, "in_dn");
    a.ld_a_nn(LPY); a.cp(PAD_TOP + PAD_SPD); a.jr(C_JR, "in_dn"); // too high -> skip
    a.sub_a(PAD_SPD); a.ld_nn_a(LPY);
    a.label("in_dn");
    // DOWN (bit3): move paddle down, clamp at PAD_BOT.
    a.bit(3, B); a.jr(NZ_JR, "in_done");
    a.ld_a_nn(LPY); a.cp(PAD_BOT - PAD_SPD + 1); a.jr(NC_JR, "in_done"); // too low -> skip
    a.add_a(PAD_SPD); a.ld_nn_a(LPY);
    a.label("in_done");
    a.ret();

    // ===================== cpumove: AI follows the ball, beatable ==========
    // Move RIGHT paddle toward ball centre. Speed 2px/frame but only on ~3 of 4
    // frames (skip when FRAME&3==0) so it's a touch slower than the ball (2px).
    a.label("cpumove");
    a.ld_a_nn(FRAME); a.and_a(0x03); a.jr(Z_JR, "cm_done"); // 1-in-4 frames: idle
    // ball centre Y = BALLY + 4 ; paddle centre Y = RPY + 8. Compare.
    a.ld_a_nn(BALLY); a.add_a(4); a.ld_r_r(C, A); // C = ball centre
    a.ld_a_nn(RPY); a.add_a(8); a.ld_r_r(B, A);   // B = paddle centre
    // if ball centre < paddle centre -> move up, else down. Dead-zone of 2px.
    a.ld_r_r(A, C); a.cp_r(B); a.jr(Z_JR, "cm_done");
    a.jr(C_JR, "cm_up");
    // ball below paddle -> move down (clamp PAD_BOT).
    a.ld_a_nn(RPY); a.cp(PAD_BOT - 1); a.jr(NC_JR, "cm_done");
    a.add_a(2); a.ld_nn_a(RPY); a.jra("cm_done");
    a.label("cm_up");
    a.ld_a_nn(RPY); a.cp(PAD_TOP + 2); a.jr(C_JR, "cm_done");
    a.sub_a(2); a.ld_nn_a(RPY);
    a.label("cm_done");
    a.ret();

    // ===================== moveball: physics + collisions ==================
    a.label("moveball");
    // X += dx
    a.ld_a_nn(BALLX); a.ld_r_r(B, A); a.ld_a_nn(DX); a.add_r(B); a.ld_nn_a(BALLX);
    // Y += dy
    a.ld_a_nn(BALLY); a.ld_r_r(B, A); a.ld_a_nn(DY); a.add_r(B); a.ld_nn_a(BALLY);

    // --- top wall: if BALLY <= BALL_MINY -> dy = +2, tick ---
    a.ld_a_nn(BALLY); a.cp(BALL_MINY + 1); a.jr(NC_JR, "mb_bot");
    a.ld_a(BALL_MINY + 1); a.ld_nn_a(BALLY);
    a.ld_a(0x02); a.ld_nn_a(DY); a.call("sfx_tick");
    a.label("mb_bot");
    // --- bottom wall: if BALLY >= BALL_MAXY -> dy = -2, tick ---
    a.ld_a_nn(BALLY); a.cp(BALL_MAXY); a.jr(C_JR, "mb_left");
    a.ld_a(BALL_MAXY - 1); a.ld_nn_a(BALLY);
    a.ld_a(0xFE); a.ld_nn_a(DY); a.call("sfx_tick");

    // --- LEFT paddle: only when moving left (dx negative) and near LP_X ---
    a.label("mb_left");
    a.ld_a_nn(DX); a.bit(7, A); a.jr(Z_JR, "mb_right"); // dx>=0 -> not heading left
    a.ld_a_nn(BALLX); a.cp(LP_X + 8); a.jr(NC_JR, "mb_missL_chk"); // ball still right of paddle face
    a.ld_a_nn(BALLX); a.cp(LP_X); a.jr(C_JR, "mb_missL_chk");      // ball already past face
    // ball is at the paddle's X face: check vertical overlap with LEFT paddle.
    a.ld_a_nn(LPY); a.ld_r_r(B, A);    // B = paddle top
    a.ld_a_nn(BALLY); a.add_a(4);      // ball centre Y
    a.sub_r(B);                        // centre - top
    a.jr(C_JR, "mb_missL_chk");        // above paddle top
    a.cp(PAD_H); a.jr(NC_JR, "mb_missL_chk"); // below paddle bottom
    // HIT left paddle: bounce right, nudge, tick.
    a.ld_a(0x02); a.ld_nn_a(DX);
    a.ld_a(LP_X + 8); a.ld_nn_a(BALLX);
    a.call("sfx_tick");
    a.jra("mb_right");
    a.label("mb_missL_chk");
    // If ball went fully off the LEFT edge -> CPU scores.
    a.ld_a_nn(BALLX); a.cp(10); a.jr(NC_JR, "mb_right");
    a.call("cpu_scores");
    a.ret();

    // --- RIGHT paddle: only when moving right (dx positive) and near RP_X ---
    a.label("mb_right");
    a.ld_a_nn(DX); a.bit(7, A); a.jr(NZ_JR, "mb_done"); // dx<0 -> not heading right
    a.ld_a_nn(BALLX); a.cp(RP_X - 8 + 1); a.jr(C_JR, "mb_done"); // not at face yet
    a.ld_a_nn(BALLX); a.cp(RP_X + 1); a.jr(NC_JR, "mb_missR_chk"); // already past face
    // vertical overlap with RIGHT paddle.
    a.ld_a_nn(RPY); a.ld_r_r(B, A);    // B = paddle top
    a.ld_a_nn(BALLY); a.add_a(4);      // ball centre
    a.sub_r(B);
    a.jr(C_JR, "mb_missR_chk");
    a.cp(PAD_H); a.jr(NC_JR, "mb_missR_chk");
    // HIT right paddle: bounce left, nudge, tick.
    a.ld_a(0xFE); a.ld_nn_a(DX);
    a.ld_a(RP_X - 8); a.ld_nn_a(BALLX);
    a.call("sfx_tick");
    a.jra("mb_done");
    a.label("mb_missR_chk");
    // If ball went fully off the RIGHT edge -> YOU score.
    a.ld_a_nn(BALLX); a.cp(166); a.jr(C_JR, "mb_done");
    a.call("you_score");
    a.ret();

    a.label("mb_done");
    a.ret();

    // ===================== scoring ========================================
    // YOU score: blip, ++SCOREL, win at 7, else re-serve.
    a.label("you_score");
    a.call("sfx_you");
    a.ld_a_nn(SCOREL); a.inc_r(A); a.ld_nn_a(SCOREL);
    a.cp(7); a.jr(C_JR, "ys_next");
    a.call("showwin");
    a.ret();
    a.label("ys_next");
    a.call("serve");
    a.call("drawscore");
    a.ret();

    // CPU score: low tone, ++SCORER, game-over at 7, else re-serve.
    a.label("cpu_scores");
    a.call("sfx_cpu");
    a.ld_a_nn(SCORER); a.inc_r(A); a.ld_nn_a(SCORER);
    a.cp(7); a.jr(C_JR, "cs_next");
    a.call("showover");
    a.ret();
    a.label("cs_next");
    a.call("serve");
    a.call("drawscore");
    a.ret();

    // ===================== sfx ===========================================
    a.label("sfx_tick"); a.tone(1200, 0xF1, 0x80); a.ret(); // soft bounce tick
    a.label("sfx_you");  a.tone(1900, 0xF3, 0x80); a.ret(); // bright score blip
    a.label("sfx_cpu");  a.tone(600, 0xF4, 0x80);  a.ret(); // low CPU-score tone

    // ===================== showover / showwin ============================
    a.label("showover");
    a.call("sfx_cpu");
    a.xor_aa(); a.ldh_to(0x40);
    a.memset(0x9800, 0x20, 0x0400);
    a.memset(0xFE00, 0x00, 160);
    a.print(0x9800 + 5 * 32 + 5, "GAME OVER");
    a.print(0x9800 + 8 * 32 + 4, "YOU 0  CPU 0");
    a.call("drawscore_end");
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.ld_a(2); a.ld_nn_a(STATE);
    a.ld_a(0x93); a.ldh_to(0x40);
    a.ret();

    a.label("showwin");
    a.tone(1950, 0xF3, 0x80); // win fanfare blip
    a.xor_aa(); a.ldh_to(0x40);
    a.memset(0x9800, 0x20, 0x0400);
    a.memset(0xFE00, 0x00, 160);
    a.print(0x9800 + 5 * 32 + 6, "YOU WIN");
    a.print(0x9800 + 8 * 32 + 4, "YOU 0  CPU 0");
    a.call("drawscore_end");
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.ld_a(3); a.ld_nn_a(STATE);
    a.ld_a(0x93); a.ldh_to(0x40);
    a.ret();

    // Draw both scores on the end screen "YOU x  CPU y" (row 8).
    // "YOU " starts at col4 -> the digit sits at col8; "CPU " digit at col15.
    a.label("drawscore_end");
    a.ld_a_nn(SCOREL); a.add_a(0x30); a.ld_nn_a(0x9800 + 8 * 32 + 8);
    a.ld_a_nn(SCORER); a.add_a(0x30); a.ld_nn_a(0x9800 + 8 * 32 + 15);
    a.ret();

    // ===================== drawscore: HUD single digit each side ==========
    // YOU score under "YOU" label (row0 col6); CPU score under "CPU" (row0 col19).
    a.label("drawscore");
    a.ld_a_nn(SCOREL); a.add_a(0x30); a.ld_nn_a(0x9800 + 0 * 32 + 6);
    a.ld_a_nn(SCORER); a.add_a(0x30); a.ld_nn_a(0x9800 + 0 * 32 + 19);
    a.ret();

    // ===================== draw: write OAM (ball + 4 paddle tiles) =========
    a.label("draw");
    // sprite 0: ball
    a.ld_a_nn(BALLY); a.ld_nn_a(0xFE00);
    a.ld_a_nn(BALLX); a.ld_nn_a(0xFE01);
    a.ld_a(BALL_TILE); a.ld_nn_a(0xFE02);
    a.xor_aa(); a.ld_nn_a(0xFE03);
    // sprite 1: left paddle top
    a.ld_a_nn(LPY); a.ld_nn_a(0xFE04);
    a.ld_a(LP_X); a.ld_nn_a(0xFE05);
    a.ld_a(PAD_TILE); a.ld_nn_a(0xFE06);
    a.xor_aa(); a.ld_nn_a(0xFE07);
    // sprite 2: left paddle bottom (+8)
    a.ld_a_nn(LPY); a.add_a(8); a.ld_nn_a(0xFE08);
    a.ld_a(LP_X); a.ld_nn_a(0xFE09);
    a.ld_a(PAD_TILE); a.ld_nn_a(0xFE0A);
    a.xor_aa(); a.ld_nn_a(0xFE0B);
    // sprite 3: right paddle top
    a.ld_a_nn(RPY); a.ld_nn_a(0xFE0C);
    a.ld_a(RP_X); a.ld_nn_a(0xFE0D);
    a.ld_a(PAD_TILE); a.ld_nn_a(0xFE0E);
    a.xor_aa(); a.ld_nn_a(0xFE0F);
    // sprite 4: right paddle bottom (+8)
    a.ld_a_nn(RPY); a.add_a(8); a.ld_nn_a(0xFE10);
    a.ld_a(RP_X); a.ld_nn_a(0xFE11);
    a.ld_a(PAD_TILE); a.ld_nn_a(0xFE12);
    a.xor_aa(); a.ld_nn_a(0xFE13);
    a.ret();

    // ===================== tile data (3 tiles) =====================
    a.label("TILES");
    a.raw(&[0x00; 16]);                                                   // 0: blank
    a.raw(&solid_tile([0x3C, 0x7E, 0xFF, 0xFF, 0xFF, 0xFF, 0x7E, 0x3C])); // 1: ball
    a.raw(&solid_tile([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])); // 2: paddle

    // ===================== font data =====================
    a.label("FONT");
    a.raw(&font_blob());

    // ===================== finalize =====================
    let rom = a.build_rom("PONG");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/pong.gb", &rom).unwrap();
    println!("wrote web/pong.gb ({} bytes code+data at $0150)", a.c.len());
}
