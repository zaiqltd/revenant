//! Builds an ORIGINAL homebrew Game Boy game — DODGE — and writes web/dodge.gb.
//!
//! You are a paddle near the bottom of the screen. Three hazard blocks fall from
//! the top; slide Left/Right (clamped to both screen edges) to avoid them. Each
//! hazard that safely passes the bottom respawns at a random X (LFSR), plays a
//! short "tick" and bumps your score; the fall speed ramps up slowly as the
//! score climbs. Touch a hazard and a low tone plays — GAME OVER shows your
//! score and "PRESS START" to retry.
//!
//! A proper little game: a TITLE screen, sound effects, an on-screen score, and
//! a clear game-over screen. STATE byte drives the loop (0=title,1=play,2=over).
//!
//! Hand-assembled SM83 through the shared two-pass assembler. No copyrighted
//! content (the boot-logo region is left zero; REVENANT runs from $0100).
//!
//!   cargo run --release --example makedodge   ->  web/dodge.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- WRAM layout ($C000.. free) ----
const PX: u16 = 0xC000; // player sprite X (OAM coordinate, PX_MIN..PX_MAX)
const D0: u16 = 0xC002; // ones digit  (0..9)
const D1: u16 = 0xC003; // tens digit
const D2: u16 = 0xC004; // hundreds digit
const SPEED: u16 = 0xC005; // current hazard fall speed (pixels/tick)
const RNG: u16 = 0xC006; // 8-bit LFSR
const TICK: u16 = 0xC007; // frame divider for game logic
const RAMP: u16 = 0xC008; // hazards survived since last speed-up
const STATE: u16 = 0xC009; // 0=title, 1=play, 2=over
const LASTSTART: u16 = 0xC00A; // previous frame's Start bit (0=was pressed)

// OAM (sprite RAM): 4 bytes each = Y, X, tile, attr. On-screen = (X-8, Y-16).
const OAM_PLAYER: u16 = 0xFE00; // sprite 0 = player
const HAZ: [u16; 3] = [0xFE04, 0xFE08, 0xFE0C]; // sprites 1..3 = hazards

const PLAYER_Y: u8 = 144; // OAM Y -> screen y = 128 (near the bottom)
const PX_MIN: u8 = 8; // OAM X 8  -> screen x 0   (left edge)
const PX_MAX: u8 = 160; // OAM X 160 -> screen x 152 (right edge for an 8px sprite)
const FALL_CUTOFF: u8 = 168; // OAM Y past which a hazard has cleared the bottom

const TICKS_PER_STEP: u8 = 2; // game logic runs every 2 frames (~30 Hz)
const PLAYER_STEP: u8 = 3; // pixels the paddle slides per step
const MAX_SPEED: u8 = 5; // cap on the fall speed ramp
const RAMP_EVERY: u8 = 5; // speed up after every N hazards cleared

// BG map cell addresses (0x9800 + row*32 + col).
const SCORE_AT: u16 = 0x9800 + 0 * 32 + 1; // "SCORE nnn" on the top row, left side

fn main() {
    let mut a = Asm::new();

    // ================= one-time setup =================
    a.label("main");
    a.di();
    a.ld_sp(0xFFFE);
    a.xor_aa().ldh_to(0x40); // LCDC = 0 (LCD off so VRAM is safe to write)

    a.memcpy_lbl("TILES", 0x8000, 16 * 16); // load 16 art tiles into $8000 (indices 0..15)
    a.load_font(); // font -> tiles $20..$5F
    a.memset(0x9800, 0x20, 0x0400); // blank the BG map with the font SPACE glyph

    a.ld_a(0xE4).ldh_to(0x47); // BGP  = 3,2,1,0
    a.ld_a(0xE4).ldh_to(0x48); // OBP0 = 3,2,1,0

    a.apu_on(); // power up the sound hardware once

    a.ldh_from(0x04).or_a(1).ld_nn_a(RNG); // seed LFSR from DIV (force nonzero)

    // Static OAM fields that never change: player Y/tile/attr, hazard tiles/attrs.
    a.ld_a(PLAYER_Y).ld_nn_a(OAM_PLAYER);
    a.ld_a(1).ld_nn_a(OAM_PLAYER + 2); // player tile = 1
    a.xor_aa().ld_nn_a(OAM_PLAYER + 3); // player attr = 0
    for h in HAZ {
        a.ld_a(2).ld_nn_a(h + 2); // hazard tile = 2
        a.xor_aa().ld_nn_a(h + 3); // hazard attr = 0
    }

    a.ld_a(0x93).ldh_to(0x40); // LCD on, OBJ on, BG on, tiles @ $8000

    a.xor_aa().ld_nn_a(STATE); // start on the title screen
    a.ld_a(1).ld_nn_a(LASTSTART); // assume Start not held at boot
    a.call("show_title"); // paint the title once

    // ================= main loop =================
    a.label("loop");
    a.wait_vblank();

    // Dispatch on STATE every frame.
    a.ld_a_nn(STATE);
    a.cp(1).jr(Z_JR, "st_play");
    a.cp(2).jr(Z_JR, "st_over");
    // ---- STATE 0: TITLE — wait for a fresh Start press ----
    a.call("start_edge"); // Z set => Start newly pressed
    a.jr(NZ_JR, "loop");
    a.call("begin_run"); // init a fresh run, draw the play field
    a.jra("loop");

    // ---- STATE 1: PLAY ----
    a.label("st_play");
    // Push the live paddle X to OAM every frame for responsive feel.
    a.ld_a_nn(PX).ld_nn_a(OAM_PLAYER + 1);
    // Throttle game logic to every TICKS_PER_STEP frames.
    a.ld_a_nn(TICK).inc_r(A).ld_nn_a(TICK);
    a.cp(TICKS_PER_STEP).jr(C_JR, "loop"); // TICK<N -> keep drawing only
    a.xor_aa().ld_nn_a(TICK);
    a.call("input");
    a.call("fall"); // move hazards, score, respawn (tick SFX on a clear)
    a.call("collide"); // AABB test -> game over on a hit
    a.jra("loop");

    // ---- STATE 2: GAME OVER — wait for a fresh Start press to retry ----
    a.label("st_over");
    a.call("start_edge");
    a.jr(NZ_JR, "loop");
    a.call("begin_run"); // Start retries straight into a fresh run
    a.jra("loop");

    // ================= start_edge: Z if Start was just pressed this frame ===
    // Reads buttons, edge-detects bit3 (Start) against LASTSTART. Returns with
    // the Z flag SET when a new press happened (was high, now low).
    a.label("start_edge");
    a.ld_a(0x10).ldh_to(0x00); // select buttons
    a.ldh_from(0x00).ldh_from(0x00); // read twice to settle the matrix
    a.and_a(0x08); // isolate Start (bit3): 0=pressed
    a.ld_r_r(C, A); // c = current Start bit
    // newly pressed = (now == 0) AND (last == 1)
    a.ld_a_nn(LASTSTART); // a = last bit (0 or 8)
    a.ld_r_r(B, A); // b = last
    a.ld_r_r(A, C).ld_nn_a(LASTSTART); // store current as next frame's "last"
    // edge if current==0 and last!=0
    a.ld_r_r(A, C).or_r(A).jr(NZ_JR, "se_no"); // current!=0 -> not pressed
    a.ld_r_r(A, B).or_r(A).jr(Z_JR, "se_no"); // last==0 (held) -> not an edge
    a.xor_aa(); // set Z (a=0 -> Z=1) => edge!
    a.ret();
    a.label("se_no");
    a.or_a(1); // clear Z (a!=0) => no edge
    a.ret();

    // ================= show_title: paint the TITLE screen =================
    a.label("show_title");
    a.call("clear_map");
    a.print(0x9800 + 5 * 32 + 7, "DODGE");
    a.print(0x9800 + 7 * 32 + 4, "DODGE THE BLOCKS");
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.call("hide_haz"); // tuck hazards off-screen so the title is clean
    // keep the player paddle parked but visible-low; move it off too for clarity
    a.ld_a(0).ld_nn_a(OAM_PLAYER + 1); // X=0 -> fully off-screen (X-8)
    a.ret();

    // ================= begin_run: (re)start a fresh play run =================
    a.label("begin_run");
    a.call("clear_map");
    a.ld_a(84).ld_nn_a(PX); // centre the paddle
    a.ld_a_nn(PX).ld_nn_a(OAM_PLAYER + 1); // and show it immediately
    a.xor_aa().ld_nn_a(D0); // score digits -> 0
    a.xor_aa().ld_nn_a(D1);
    a.xor_aa().ld_nn_a(D2);
    a.ld_a(1).ld_nn_a(SPEED); // base fall speed
    a.xor_aa().ld_nn_a(TICK);
    a.xor_aa().ld_nn_a(RAMP);

    // Stagger the three hazards: distinct X and staggered (small) Y so they enter
    // the screen at different times.
    let starts = [(40u8, 24u8), (96, 8), (132, 40)];
    for (i, h) in HAZ.iter().enumerate() {
        a.ld_a(starts[i].1).ld_nn_a(*h); // Y
        a.ld_a(starts[i].0).ld_nn_a(h + 1); // X
    }

    a.print(SCORE_AT, "SCORE"); // static label
    a.call("drawscore"); // paint the initial 000
    a.ld_a(1).ld_nn_a(STATE); // -> PLAY
    a.ret();

    // ================= input: D-pad -> move paddle, clamp both edges =====
    a.label("input");
    a.ld_a(0x20).ldh_to(0x00); // select directions
    a.ldh_from(0x00).ldh_from(0x00); // read twice to debounce the matrix
    a.ld_r_r(B, A); // b = dir bits (0 = pressed)

    // LEFT (bit1): PX -= STEP then clamp to >= PX_MIN
    a.bit(1, B).jr(NZ_JR, "in_r");
    a.ld_a_nn(PX).sub_a(PLAYER_STEP).ld_nn_a(PX);
    a.ld_a_nn(PX).cp(PX_MIN).jr(NC_JR, "in_r"); // PX >= MIN -> fine
    a.ld_a(PX_MIN).ld_nn_a(PX);
    a.label("in_r");
    // RIGHT (bit0): PX += STEP then clamp to <= PX_MAX
    a.bit(0, B).jr(NZ_JR, "in_done");
    a.ld_a_nn(PX).add_a(PLAYER_STEP).ld_nn_a(PX);
    a.ld_a_nn(PX).cp(PX_MAX + 1).jr(C_JR, "in_done"); // PX <= MAX -> fine
    a.ld_a(PX_MAX).ld_nn_a(PX);
    a.label("in_done");
    a.ret();

    // ================= fall: advance the three hazards =================
    // Each hazard is handled inline against fixed OAM addresses so we only use
    // ld_a_nn / ld_nn_a (no DE-relative writes needed).
    a.label("fall");
    for (i, h) in HAZ.iter().enumerate() {
        let skip = a.uniq("nofall");
        let done = a.uniq("falldone");
        // a = Y + SPEED
        a.ld_a_nn(SPEED).ld_r_r(C, A);
        a.ld_a_nn(*h).add_r(C);
        a.cp(FALL_CUTOFF).jr(C_JR, &skip); // still falling -> store new Y
        // ---- cleared the bottom: respawn at top with a random X, score++ ----
        a.call("rng");
        a.and_a(0x7F).add_a(16).ld_nn_a(h + 1); // X in 16..143
        a.ld_a(0).ld_nn_a(*h); // Y back to the very top
        a.call("score_inc"); // bumps score + plays the "tick" SFX
        let _ = i;
        a.jra(&done);
        a.label(&skip);
        a.ld_nn_a(*h); // store Y+SPEED
        a.label(&done);
    }
    a.ret();

    // ================= collide: AABB player vs each hazard =================
    // Player box: X in [PX, PX+7], Y fixed at PLAYER_Y. A hazard overlaps when
    // |hazX-PX| < 8  AND  |hazY-PLAYER_Y| < 8. We test with subtract+cp.
    a.label("collide");
    for h in HAZ {
        let no = a.uniq("nohit");
        // --- X overlap: compute (hazX - PX + 8); overlap if result in 1..15 ---
        a.ld_a_nn(h + 1); // a = hazX
        a.ld_hl(PX); // (hl) = PX
        a.sub_r(M); // a = hazX - PX   (signed, wraps)
        a.add_a(8); // shift so overlap band is 1..15 (avoids 0 edge)
        a.cp(1).jr(C_JR, &no); // a < 1  -> no overlap
        a.cp(16).jr(NC_JR, &no); // a >= 16 -> no overlap
        // --- Y overlap: same trick against PLAYER_Y ---
        a.ld_a_nn(h); // a = hazY
        a.sub_a(PLAYER_Y);
        a.add_a(8);
        a.cp(1).jr(C_JR, &no);
        a.cp(16).jr(NC_JR, &no);
        // both axes overlap -> hit
        a.jpa("gameover");
        a.label(&no);
    }
    a.ret();

    // ================= gameover: low tone, paint the over screen, STATE=2 ====
    a.label("gameover");
    a.tone(900, 0xF1, 0x80); // low "hit" tone
    a.call("clear_map");
    a.call("hide_haz");
    a.ld_a(0).ld_nn_a(OAM_PLAYER + 1); // hide the paddle
    a.print(0x9800 + 5 * 32 + 5, "GAME OVER");
    a.print(0x9800 + 8 * 32 + 6, "SCORE");
    // draw the three score digits after "SCORE " at row 8
    a.ld_a_nn(D2).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 12);
    a.ld_a_nn(D1).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 13);
    a.ld_a_nn(D0).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 14);
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.ld_a(2).ld_nn_a(STATE);
    a.jpa("loop"); // resume the dispatcher

    // ================= score_inc: D0/D1/D2 decimal ripple, ramp speed =====
    a.label("score_inc");
    a.tone(1700, 0xF3, 0x80); // bright "tick" when a block is dodged
    a.ld_a_nn(D0).inc_r(A).cp(10).jr(C_JR, "si_d0"); // ones++ ; <10 -> store
    a.xor_aa().ld_nn_a(D0); // carry: ones=0
    a.ld_a_nn(D1).inc_r(A).cp(10).jr(C_JR, "si_d1"); // tens++
    a.xor_aa().ld_nn_a(D1);
    a.ld_a_nn(D2).inc_r(A).cp(10).jr(C_JR, "si_d2"); // hundreds++
    a.ld_a(9).ld_nn_a(D2); // clamp at 999 (overflow holds 9)
    a.jra("si_draw");
    a.label("si_d2");
    a.ld_nn_a(D2);
    a.jra("si_draw");
    a.label("si_d1");
    a.ld_nn_a(D1);
    a.jra("si_draw");
    a.label("si_d0");
    a.ld_nn_a(D0);
    a.label("si_draw");
    // --- ramp the fall speed every RAMP_EVERY hazards (up to MAX_SPEED) ---
    a.ld_a_nn(RAMP).inc_r(A).ld_nn_a(RAMP);
    a.cp(RAMP_EVERY).jr(C_JR, "si_end"); // RAMP<N -> no speed change
    a.xor_aa().ld_nn_a(RAMP);
    a.ld_a_nn(SPEED).cp(MAX_SPEED).jr(NC_JR, "si_end"); // already maxed
    a.ld_a_nn(SPEED).inc_r(A).ld_nn_a(SPEED);
    a.label("si_end");
    a.call("drawscore");
    a.ret();

    // ================= drawscore: write D2 D1 D0 as font digits ($30+n) =====
    a.label("drawscore");
    a.ld_a_nn(D2).add_a(0x30).ld_nn_a(SCORE_AT + 6); // after "SCORE "
    a.ld_a_nn(D1).add_a(0x30).ld_nn_a(SCORE_AT + 7);
    a.ld_a_nn(D0).add_a(0x30).ld_nn_a(SCORE_AT + 8);
    a.ret();

    // ================= clear_map: blank the visible BG with SPACE ($20) ======
    a.label("clear_map");
    a.memset(0x9800, 0x20, 0x0400);
    a.ret();

    // ================= hide_haz: park the three hazard sprites off-screen ====
    a.label("hide_haz");
    for h in HAZ {
        a.xor_aa().ld_nn_a(h); // Y=0 -> screen y -16 (hidden)
        a.xor_aa().ld_nn_a(h + 1); // X=0 -> hidden
    }
    a.ret();

    // ================= rng: 8-bit LFSR (tap 0x1D) mixed with DIV =========
    a.label("rng");
    a.ld_a_nn(RNG).add_aa().jr(NC_JR, "rng_ns").xor_a(0x1D);
    a.label("rng_ns");
    a.ld_r_r(B, A).ldh_from(0x04).xor_r(B); // mix in DIV
    a.ld_nn_a(RNG);
    a.ret();

    // ================= tile data: 16 tiles x 16 bytes =================
    a.label("TILES");
    // 0: blank
    a.raw(&[0u8; 16]);
    // 1: player paddle — a solid wide bar with a notch (color 3)
    a.raw(&solid_tile([0x00, 0x00, 0x7E, 0xFF, 0xFF, 0xFF, 0x7E, 0x00]));
    // 2: hazard block — a spiky filled box (color 3)
    a.raw(&solid_tile([0xFF, 0xDB, 0xFF, 0xFF, 0xBD, 0xFF, 0xDB, 0xFF]));
    // 3..15: spare (blank) — art tiles live in 0..31, font owns $20..$5F
    for _ in 3..16 {
        a.raw(&[0u8; 16]);
    }

    // ================= font data (tiles $20..$5F) =================
    a.label("FONT");
    a.raw(&font_blob());

    // ================= emit ROM =================
    let rom = a.build_rom("DODGE");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/dodge.gb", &rom).unwrap();
    println!("wrote web/dodge.gb ({} bytes code+data at $0150)", a.c.len());
}
