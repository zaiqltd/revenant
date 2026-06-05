//! Builds an ORIGINAL homebrew Game Boy game — BLASTER — and writes web/blaster.gb.
//!
//! A vertical shooter. Your ship sprite sits at the bottom and slides Left/Right
//! (clamped to both screen edges). Press A to fire a single bullet upward; while a
//! bullet is in flight you cannot fire again. A row of five enemy sprites drifts
//! sideways across the top and slowly descends. A bullet that overlaps an enemy
//! (AABB) removes it, plays a hit tone, and scores +1. Clear the whole wave and a
//! fresh, faster wave respawns. If any enemy reaches the bottom band (or touches
//! the ship) it is GAME OVER: a death tone plays, the final score is shown, and a
//! "PRESS START" prompt restarts a fresh game.
//!
//! A STATE byte drives a title / play / over machine. The score is drawn as three
//! decimal digits via the shared bitmap font. Hand-assembled SM83 through the
//! shared two-pass assembler — no copyrighted content (boot-logo region left zero;
//! REVENANT runs from $0100).
//!
//!   cargo run --release --example makeblaster   ->  web/blaster.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- WRAM layout ($C000.. is free) ----
const STATE: u16 = 0xC000; // 0 = title, 1 = play, 2 = game over
const SHIPX: u16 = 0xC001; // ship OAM X (screen x = SHIPX-8)
const BULX: u16 = 0xC002; // bullet OAM X
const BULY: u16 = 0xC003; // bullet OAM Y (0 = no bullet active)
const D0: u16 = 0xC004; // score ones digit
const D1: u16 = 0xC005; // score tens digit
const D2: u16 = 0xC006; // score hundreds digit
const ALIVE: u16 = 0xC007; // enemies remaining this wave (0..NENEMY)
const EDIR: u16 = 0xC008; // enemy drift direction (1 = right, 0xFF = left)
const TICK: u16 = 0xC009; // frame divider for descent cadence
const WAVE: u16 = 0xC00A; // wave number (raises descent speed)
const LASTBTN: u16 = 0xC00B; // previous frame's Start bit (edge detect)
const ESPEED: u16 = 0xC00C; // pixels enemies descend per descent tick
const RNG: u16 = 0xC00D; // 8-bit LFSR (start X variety)

// Enemy table in WRAM: NENEMY slots, each [Y, X, FLAG]. FLAG !=0 means alive.
const NENEMY: usize = 5;
const ENEMY: u16 = 0xC010; // ENEMY + i*3 -> [Y,X,FLAG]

// ---- OAM (sprite RAM): 4 bytes each = Y, X, tile, attr. screen=(X-8,Y-16) ----
const OAM_SHIP: u16 = 0xFE00; // sprite 0  = ship
const OAM_BUL: u16 = 0xFE04; // sprite 1  = bullet
const OAM_ENEMY: u16 = 0xFE08; // sprites 2.. = enemies

// ---- geometry (OAM coords) ----
const SHIP_Y: u8 = 144; // screen y = 128 (near bottom)
const SHIP_MIN: u8 = 8; // screen x 0
const SHIP_MAX: u8 = 160; // screen x 152 (8px sprite right edge)
const SHIP_STEP: u8 = 3; // px per frame slide

const BUL_STEP: u8 = 4; // px the bullet rises per frame
const BUL_TOP: u8 = 16; // OAM Y at/above which the bullet expires

const ENEMY_Y0: u8 = 24; // OAM Y of the spawned wave (screen y = 8)
const ENEMY_X0: u8 = 24; // first enemy OAM X
const ENEMY_DX: u8 = 24; // spacing between enemies
const ENEMY_LMIN: u8 = 12; // left clamp for the drifting block
const ENEMY_RMAX: u8 = 148; // right clamp (rightmost enemy OAM X)
const ENEMY_BOTTOM: u8 = 136; // OAM Y at/below which an enemy is fatal

const DESCEND_EVERY: u8 = 24; // descend once every N frames

// ---- art tiles (indices 0..31 so they never collide with the font $20..$5F) ----
const TILE_SHIP: u8 = 1;
const TILE_BULLET: u8 = 2;
const TILE_ENEMY: u8 = 3;

fn main() {
    let mut a = Asm::new();

    // ===================== one-time setup =====================
    a.label("main");
    a.di();
    a.ld_sp(0xFFFE);
    a.xor_aa().ldh_to(0x40); // LCDC = 0 -> LCD off (safe VRAM/OAM writes)

    // Load our 4 art tiles into $8000 (indices 0..3).
    a.memcpy_lbl("TILES", 0x8000, 4 * 16);
    // Load the bitmap font into tiles $20..$5F.
    a.load_font();

    // Clear OAM (Y=0 -> off-screen) and blank the BG map.
    a.memset(0xFE00, 0x00, 160);
    a.memset(0x9800, 0x00, 0x0400);

    // Palettes: BGP / OBP0 = 0xE4.
    a.ld_a(0xE4).ldh_to(0x47);
    a.ld_a(0xE4).ldh_to(0x48);

    a.apu_on();

    // Seed RNG from DIV (force nonzero).
    a.ldh_from(0x04).or_a(1).ld_nn_a(RNG);
    a.xor_aa().ld_nn_a(LASTBTN);

    // LCD on: 0x93 = LCD + tiledata@$8000 + OBJ on + BG on.
    a.ld_a(0x93).ldh_to(0x40);

    // Start on the title screen.
    a.xor_aa().ld_nn_a(STATE);
    a.call("draw_title");

    // ===================== main loop =====================
    a.label("loop");
    a.wait_vblank();
    a.call("read_start"); // sets C = Start edge (1 on a fresh press)

    // Branch on STATE: 0 title, 1 play, 2 over.
    a.ld_a_nn(STATE);
    a.or_a(0).jr(Z_JR, "st_title");
    a.cp(1).jr(Z_JR, "st_play");
    a.jra("st_over");

    // ---- TITLE: wait for a fresh Start press -> begin a new game ----
    a.label("st_title");
    a.bit(0, C).jr(Z_JR, "loop"); // C bit0 = Start edge; 0 -> keep waiting
    a.call("newgame");
    a.jra("loop");

    // ---- PLAY ----
    a.label("st_play");
    a.call("input"); // slide ship + fire
    a.call("move_bullet"); // advance bullet, expire at top
    a.call("move_enemies"); // drift + periodic descent, lose check
    a.call("collide"); // bullet vs enemies -> score / wave clear
    a.call("draw_play"); // push OAM for ship/bullet/enemies
    a.jra("loop");

    // ---- OVER: wait for Start -> back to a fresh game ----
    a.label("st_over");
    a.bit(0, C).jr(Z_JR, "loop");
    a.call("newgame");
    a.jra("loop");

    // ===================== read_start: C bit0 = fresh Start press =========
    a.label("read_start");
    a.ld_a(0x10).ldh_to(0x00); // select buttons
    a.ldh_from(0x00).ldh_from(0x00); // read twice (matrix settle)
    // bit3 = Start (0 = pressed). Build "pressed now" into bit0 of B.
    a.ld_r_r(B, A);
    a.ld_r_n(C, 0); // C = edge result (0 default)
    a.bit(3, B).jr(NZ_JR, "rs_notpressed"); // not pressed now
    // pressed now: if LASTBTN==0 -> this is a new edge.
    a.ld_a_nn(LASTBTN).or_a(0).jr(NZ_JR, "rs_held");
    a.ld_r_n(C, 1); // fresh edge
    a.label("rs_held");
    a.ld_a(1).ld_nn_a(LASTBTN); // remember held
    a.ret();
    a.label("rs_notpressed");
    a.xor_aa().ld_nn_a(LASTBTN); // released
    a.ret();

    // ===================== newgame: reset everything, spawn wave 1 =========
    a.label("newgame");
    a.ld_a(84).ld_nn_a(SHIPX);
    a.xor_aa().ld_nn_a(BULX);
    a.xor_aa().ld_nn_a(BULY); // no bullet
    a.xor_aa().ld_nn_a(D0);
    a.xor_aa().ld_nn_a(D1);
    a.xor_aa().ld_nn_a(D2);
    a.ld_a(1).ld_nn_a(WAVE);
    a.ld_a(1).ld_nn_a(ESPEED); // wave 1 descends 1px per tick
    a.call("spawn_wave");
    a.ld_a(1).ld_nn_a(STATE); // -> play
    // rebuild the play BG (clear title text, paint score label)
    a.call("draw_play_bg");
    a.call("draw_score");
    a.ret();

    // ===================== spawn_wave: lay out NENEMY enemies =============
    a.label("spawn_wave");
    a.ld_a(1).ld_nn_a(EDIR); // drift right initially
    a.xor_aa().ld_nn_a(TICK);
    a.ld_a(NENEMY as u8).ld_nn_a(ALIVE);
    // write each enemy slot [Y, X, FLAG=1]
    for i in 0..NENEMY {
        let base = ENEMY + (i as u16) * 3;
        a.ld_a(ENEMY_Y0).ld_nn_a(base); // Y
        a.ld_a(ENEMY_X0 + (i as u8) * ENEMY_DX).ld_nn_a(base + 1); // X
        a.ld_a(1).ld_nn_a(base + 2); // FLAG alive
    }
    a.ret();

    // ===================== input: slide ship + fire bullet ================
    a.label("input");
    a.ld_a(0x20).ldh_to(0x00); // select directions
    a.ldh_from(0x00).ldh_from(0x00);
    a.ld_r_r(B, A); // B = dir bits (0 = pressed)

    // LEFT (bit1): SHIPX -= STEP, clamp >= SHIP_MIN
    a.bit(1, B).jr(NZ_JR, "in_r");
    a.ld_a_nn(SHIPX).sub_a(SHIP_STEP).ld_nn_a(SHIPX);
    a.ld_a_nn(SHIPX).cp(SHIP_MIN).jr(NC_JR, "in_r");
    a.ld_a(SHIP_MIN).ld_nn_a(SHIPX);
    a.label("in_r");
    // RIGHT (bit0): SHIPX += STEP, clamp <= SHIP_MAX
    a.bit(0, B).jr(NZ_JR, "in_fire");
    a.ld_a_nn(SHIPX).add_a(SHIP_STEP).ld_nn_a(SHIPX);
    a.ld_a_nn(SHIPX).cp(SHIP_MAX + 1).jr(C_JR, "in_fire");
    a.ld_a(SHIP_MAX).ld_nn_a(SHIPX);
    a.label("in_fire");
    // FIRE (A = buttons bit0). Select buttons, read.
    a.ld_a(0x10).ldh_to(0x00);
    a.ldh_from(0x00).ldh_from(0x00);
    a.ld_r_r(B, A);
    a.bit(0, B).jr(NZ_JR, "in_done"); // A not pressed
    // only fire if no bullet is currently active (BULY == 0)
    a.ld_a_nn(BULY).or_a(0).jr(NZ_JR, "in_done");
    // spawn a bullet just above the ship, aligned to ship centre
    a.ld_a_nn(SHIPX).ld_nn_a(BULX);
    a.ld_a(SHIP_Y - 6).ld_nn_a(BULY);
    a.tone(1600, 0xF3, 0x80); // fire blip
    a.label("in_done");
    a.ret();

    // ===================== move_bullet: rise & expire ====================
    a.label("move_bullet");
    a.ld_a_nn(BULY).or_a(0).jr(Z_JR, "mb_done"); // no bullet
    a.ld_a_nn(BULY).sub_a(BUL_STEP);
    a.cp(BUL_TOP).jr(C_JR, "mb_kill"); // reached top -> expire
    a.ld_nn_a(BULY);
    a.jra("mb_done");
    a.label("mb_kill");
    a.xor_aa().ld_nn_a(BULY);
    a.label("mb_done");
    a.ret();

    // ===================== move_enemies: drift + descend + lose ==========
    a.label("move_enemies");
    // --- horizontal drift: shift all alive enemies by EDIR; if any hits a
    //     clamp, reverse direction (block stays cohesive). Two-pass: find min/max
    //     X of alive enemies to know when to bounce. ---
    // Compute extremes: C = min alive X, D = max alive X.
    a.ld_r_n(C, 0xFF); // min seed (high)
    a.ld_r_n(D, 0x00); // max seed (low)
    for i in 0..NENEMY {
        let base = ENEMY + (i as u16) * 3;
        let skip = a.uniq("me_ext");
        let nomin = a.uniq("me_nomin");
        let nomax = a.uniq("me_nomax");
        a.ld_a_nn(base + 2).or_a(0).jr(Z_JR, &skip); // dead -> skip this enemy
        a.ld_a_nn(base + 1).ld_r_r(E, A); // E = this enemy X
        // min: if E < C then C = E
        a.ld_r_r(A, E).cp_r(C).jr(NC_JR, &nomin); // E >= C -> keep min
        a.ld_r_r(C, E);
        a.label(&nomin);
        // max: if E > D then D = E
        a.ld_r_r(A, E).cp_r(D).jr(C_JR, &nomax); // E < D -> keep max
        a.ld_r_r(D, E);
        a.label(&nomax);
        a.label(&skip);
    }
    // decide direction: if moving right and max >= RMAX -> go left; if moving left
    // and min <= LMIN -> go right.
    a.ld_a_nn(EDIR).cp(1).jr(NZ_JR, "me_left");
    // moving right
    a.ld_r_r(A, D).cp(ENEMY_RMAX).jr(C_JR, "me_apply"); // max < RMAX -> ok
    a.ld_a(0xFF).ld_nn_a(EDIR);
    a.jra("me_apply");
    a.label("me_left");
    a.ld_r_r(A, C).cp(ENEMY_LMIN + 1).jr(NC_JR, "me_apply"); // min > LMIN -> ok
    a.ld_a(1).ld_nn_a(EDIR);
    a.label("me_apply");
    // apply horizontal step to all alive enemies
    a.ld_a_nn(EDIR).ld_r_r(B, A); // B = signed step (1 or 0xFF)
    for i in 0..NENEMY {
        let base = ENEMY + (i as u16) * 3;
        let skip = a.uniq("me_mv");
        a.ld_a_nn(base + 2).or_a(0).jr(Z_JR, &skip);
        a.ld_a_nn(base + 1).add_r(B).ld_nn_a(base + 1);
        a.label(&skip);
    }

    // --- periodic descent ---
    a.ld_a_nn(TICK).inc_r(A).ld_nn_a(TICK);
    a.cp(DESCEND_EVERY).jr(C_JR, "me_loseck"); // not time yet
    a.xor_aa().ld_nn_a(TICK);
    a.ld_a_nn(ESPEED).ld_r_r(B, A); // B = descent px
    for i in 0..NENEMY {
        let base = ENEMY + (i as u16) * 3;
        let skip = a.uniq("me_dn");
        a.ld_a_nn(base + 2).or_a(0).jr(Z_JR, &skip);
        a.ld_a_nn(base).add_r(B).ld_nn_a(base);
        a.label(&skip);
    }

    // --- lose check: any alive enemy with Y >= ENEMY_BOTTOM -> game over ---
    a.label("me_loseck");
    for i in 0..NENEMY {
        let base = ENEMY + (i as u16) * 3;
        let skip = a.uniq("me_ls");
        a.ld_a_nn(base + 2).or_a(0).jr(Z_JR, &skip);
        a.ld_a_nn(base).cp(ENEMY_BOTTOM).jr(C_JR, &skip); // Y < bottom -> safe
        a.jpa("gameover");
        a.label(&skip);
    }
    a.ret();

    // ===================== collide: bullet vs enemies ====================
    a.label("collide");
    a.ld_a_nn(BULY).or_a(0).jp(ZF, "col_done"); // no bullet
    for i in 0..NENEMY {
        let base = ENEMY + (i as u16) * 3;
        let no = a.uniq("col_no");
        a.ld_a_nn(base + 2).or_a(0).jr(Z_JR, &no); // dead enemy
        // X overlap: (enemyX - bulX + 8) in 1..15
        a.ld_a_nn(base + 1); // enemyX
        a.ld_hl(BULX);
        a.sub_r(M);
        a.add_a(8);
        a.cp(1).jr(C_JR, &no);
        a.cp(16).jr(NC_JR, &no);
        // Y overlap
        a.ld_a_nn(base); // enemyY
        a.ld_hl(BULY);
        a.sub_r(M);
        a.add_a(8);
        a.cp(1).jr(C_JR, &no);
        a.cp(16).jr(NC_JR, &no);
        // HIT: kill enemy, kill bullet, score++, tone, maybe clear wave.
        a.xor_aa().ld_nn_a(base + 2); // FLAG = 0 (dead)
        a.ld_a(ENEMY_Y0).ld_nn_a(base); // park off the play area (Y high, but flag dead hides it)
        a.xor_aa().ld_nn_a(BULY); // bullet consumed
        a.tone(1900, 0xF3, 0x80); // hit blip
        a.call("score_inc");
        a.ld_a_nn(ALIVE).dec_r(A).ld_nn_a(ALIVE);
        a.jr(NZ_JR, &no); // wave not cleared yet
        a.call("next_wave");
        a.jpa("col_done"); // wave reset; stop scanning this frame
        a.label(&no);
    }
    a.label("col_done");
    a.ret();

    // ===================== next_wave: faster respawn =====================
    a.label("next_wave");
    a.ld_a_nn(WAVE).inc_r(A).ld_nn_a(WAVE);
    // raise descent speed each wave, cap at 4
    a.ld_a_nn(ESPEED).cp(4).jr(NC_JR, "nw_spawn");
    a.ld_a_nn(ESPEED).inc_r(A).ld_nn_a(ESPEED);
    a.label("nw_spawn");
    a.call("spawn_wave");
    a.xor_aa().ld_nn_a(BULY); // clear any in-flight bullet
    a.ret();

    // ===================== score_inc: decimal ripple D0/D1/D2 ============
    a.label("score_inc");
    a.ld_a_nn(D0).inc_r(A).cp(10).jr(C_JR, "si_d0");
    a.xor_aa().ld_nn_a(D0);
    a.ld_a_nn(D1).inc_r(A).cp(10).jr(C_JR, "si_d1");
    a.xor_aa().ld_nn_a(D1);
    a.ld_a_nn(D2).inc_r(A).cp(10).jr(C_JR, "si_d2");
    a.ld_a(9).ld_nn_a(D2); // clamp at 999
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
    a.call("draw_score");
    a.ret();

    // ===================== gameover: tone + over screen ==================
    // Reached via a jp (not call) from deep inside move_enemies, so reset SP to
    // discard the abandoned call frames before re-entering the main loop.
    a.label("gameover");
    a.ld_sp(0xFFFE);
    a.tone(700, 0xF1, 0x80); // death tone
    a.ld_a(2).ld_nn_a(STATE);
    a.call("draw_over");
    // hide bullet + all enemy sprites immediately
    a.xor_aa().ld_nn_a(BULY);
    a.jpa("loop");

    // ===================== draw_play: push OAM ===========================
    a.label("draw_play");
    // ship (sprite 0)
    a.ld_a(SHIP_Y).ld_nn_a(OAM_SHIP);
    a.ld_a_nn(SHIPX).ld_nn_a(OAM_SHIP + 1);
    a.ld_a(TILE_SHIP).ld_nn_a(OAM_SHIP + 2);
    a.xor_aa().ld_nn_a(OAM_SHIP + 3);
    // bullet (sprite 1) — hidden when BULY==0
    a.ld_a_nn(BULY).or_a(0).jr(NZ_JR, "dp_bul");
    a.xor_aa().ld_nn_a(OAM_BUL); // Y=0 -> off-screen
    a.jra("dp_enemies");
    a.label("dp_bul");
    a.ld_a_nn(BULY).ld_nn_a(OAM_BUL);
    a.ld_a_nn(BULX).ld_nn_a(OAM_BUL + 1);
    a.ld_a(TILE_BULLET).ld_nn_a(OAM_BUL + 2);
    a.xor_aa().ld_nn_a(OAM_BUL + 3);
    a.label("dp_enemies");
    for i in 0..NENEMY {
        let base = ENEMY + (i as u16) * 3;
        let oam = OAM_ENEMY + (i as u16) * 4;
        let hide = a.uniq("de_hide");
        let done = a.uniq("de_done");
        a.ld_a_nn(base + 2).or_a(0).jr(Z_JR, &hide); // dead -> hide
        a.ld_a_nn(base).ld_nn_a(oam); // Y
        a.ld_a_nn(base + 1).ld_nn_a(oam + 1); // X
        a.ld_a(TILE_ENEMY).ld_nn_a(oam + 2);
        a.xor_aa().ld_nn_a(oam + 3);
        a.jra(&done);
        a.label(&hide);
        a.xor_aa().ld_nn_a(oam); // Y=0 -> off-screen
        a.label(&done);
    }
    a.ret();

    // ===================== draw_score: "SCORE:nnn" digits =================
    // Digits use the font: ASCII '0' = 0x30, plus the digit value.
    a.label("draw_score");
    a.ld_a_nn(D2).add_a(0x30).ld_nn_a(0x9800 + 7); // col 7
    a.ld_a_nn(D1).add_a(0x30).ld_nn_a(0x9800 + 8);
    a.ld_a_nn(D0).add_a(0x30).ld_nn_a(0x9800 + 9);
    a.ret();

    // ===================== draw_play_bg: label + clear ===================
    a.label("draw_play_bg");
    a.memset(0x9800, 0x00, 0x0400); // clear map
    a.print(0x9800, "SCORE:");
    a.ret();

    // ===================== draw_title ====================================
    a.label("draw_title");
    a.memset(0x9800, 0x00, 0x0400);
    a.print(0x9800 + 6 * 32 + 6, "BLASTER");
    a.print(0x9800 + 9 * 32 + 4, "PRESS START");
    a.ret();

    // ===================== draw_over =====================================
    a.label("draw_over");
    a.memset(0x9800, 0x00, 0x0400);
    a.print(0x9800 + 5 * 32 + 5, "GAME OVER");
    a.print(0x9800 + 8 * 32 + 5, "SCORE:");
    a.ld_a_nn(D2).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 5 + 6);
    a.ld_a_nn(D1).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 5 + 7);
    a.ld_a_nn(D0).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 5 + 8);
    a.print(0x9800 + 11 * 32 + 4, "PRESS START");
    // hide every sprite so the over screen is clean
    a.memset(0xFE00, 0x00, 160);
    a.ret();

    // ===================== font + tile data ==============================
    a.label("FONT");
    a.raw(&font_blob());

    a.label("TILES");
    // 0: blank
    a.raw(&[0u8; 16]);
    // 1: ship — an upward arrow / wedge (color 3)
    a.raw(&solid_tile([
        0x18, 0x18, 0x3C, 0x3C, 0x7E, 0x7E, 0xFF, 0xDB,
    ]));
    // 2: bullet — a small vertical bolt (color 3)
    a.raw(&solid_tile([
        0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00,
    ]));
    // 3: enemy — a chunky invader-ish blob (color 3, original shape)
    a.raw(&solid_tile([
        0x3C, 0x7E, 0xDB, 0xFF, 0xBD, 0x99, 0x42, 0x24,
    ]));

    // ===================== emit ROM ======================================
    let rom = a.build_rom("BLASTER");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/blaster.gb", &rom).unwrap();
    println!("wrote web/blaster.gb ({} bytes code+data at $0150)", a.c.len());
}
