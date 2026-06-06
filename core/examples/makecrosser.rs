//! Builds an ORIGINAL homebrew Game Boy game — CROSSER — and writes web/crosser.gb.
//!
//! A frogger-style crossing game. Your hopper starts at the BOTTOM and must reach
//! the TOP across five lanes of moving hazards (cars). The D-pad hops the player
//! one cell up/down/left/right, clamped to the screen. Each lane's cars slide
//! horizontally at a lane-specific speed and wrap around the edges. Any AABB
//! overlap between the player and a car = GAME OVER (a low tone). Reaching the top
//! row scores +1, chimes, and drops the player back to the bottom (and nudges the
//! car speeds up a touch). A move plays a short hop tick.
//!
//! A proper little game: TITLE screen, sound effects, on-screen score, and a clear
//! GAME OVER screen. A STATE byte drives the loop (0=title,1=play,2=over).
//!
//! Hand-assembled SM83 through the shared two-pass assembler. No copyrighted
//! content (the boot-logo region is left zero; REVENANT runs from $0100).
//!
//!   cargo run --release --example makecrosser   ->  web/crosser.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- WRAM layout ($C000.. free) ----
const PX: u16 = 0xC000; // player OAM X (PX_MIN..PX_MAX), grid-aligned
const PY: u16 = 0xC001; // player OAM Y (PY_TOP..PY_BOT), grid-aligned
const D0: u16 = 0xC002; // ones digit
const D1: u16 = 0xC003; // tens digit
const D2: u16 = 0xC004; // hundreds digit
const SPEEDUP: u16 = 0xC005; // extra speed added to every lane (ramps with score)
const RNG: u16 = 0xC006; // 8-bit LFSR
const TICK: u16 = 0xC007; // frame divider for game logic
const STATE: u16 = 0xC008; // 0=title, 1=play, 2=over
const LASTBTN: u16 = 0xC009; // previous frame's D-pad+button bits (edge detect)
const LASTSTART: u16 = 0xC00A; // previous frame's Start bit

// OAM (sprite RAM): 4 bytes each = Y, X, tile, attr. On-screen = (X-8, Y-16).
const OAM_PLAYER: u16 = 0xFE00; // sprite 0 = player hopper

// Five lanes, two cars each (sprites 1..10). Each car: [base OAM addr].
// Cars in the same lane share a Y and a per-lane signed speed.
const CARS: [u16; 10] = [
    0xFE04, 0xFE08, // lane 0
    0xFE0C, 0xFE10, // lane 1
    0xFE14, 0xFE18, // lane 2
    0xFE1C, 0xFE20, // lane 3
    0xFE24, 0xFE28, // lane 4
];
// Per-lane OAM Y (screen y = Y-16). Lanes sit between the bottom and top rows.
const LANE_Y: [u8; 5] = [128, 112, 96, 80, 64];
// Per-lane signed step (two's-complement u8). Alternating directions, varied speed.
const LANE_SPEED: [u8; 5] = [2, 0xFE /* -2 */, 3, 0xFD /* -3 */, 2];

const PY_BOT: u8 = 144; // start row (OAM Y) -> screen y 128
const PY_TOP: u8 = 32; // goal row (OAM Y)  -> screen y 16
const PY_STEP: u8 = 16; // one lane per vertical hop
const PX_MIN: u8 = 8; // OAM X 8   -> screen x 0
const PX_MAX: u8 = 160; // OAM X 160 -> screen x 152 (rightmost 8px sprite)
const PX_STEP: u8 = 16; // horizontal hop
const PX_START: u8 = 80; // centred start column

const CAR_LO: u8 = 8; // car X wrap low bound (OAM X)
const CAR_HI: u8 = 160; // car X wrap high bound (OAM X)

const TICKS_PER_STEP: u8 = 4; // car logic runs every 4 frames (smooth, playable)

// BG map cell addresses (0x9800 + row*32 + col).
const SCORE_AT: u16 = 0x9800 + 0 * 32 + 1; // "SCORE nnn" top-left

fn main() {
    let mut a = Asm::new();

    // ================= one-time setup =================
    a.label("main");
    a.di();
    a.ld_sp(0xFFFE);
    a.xor_aa().ldh_to(0x40); // LCDC = 0 (LCD off so VRAM is safe to write)

    a.memcpy_lbl("TILES", 0x8000, 16 * 16); // 16 art tiles -> $8000 (indices 0..15)
    a.load_font(); // font -> tiles $20..$5F
    a.memset(0x9800, 0x20, 0x0400); // blank the BG map with SPACE

    a.ld_a(0xE4).ldh_to(0x47); // BGP
    a.ld_a(0xE4).ldh_to(0x48); // OBP0

    a.apu_on();
    a.ldh_from(0x04).or_a(1).ld_nn_a(RNG); // seed LFSR from DIV (nonzero)

    // Static OAM fields: player tile/attr, car tiles/attrs.
    a.ld_a(1).ld_nn_a(OAM_PLAYER + 2); // player tile = 1
    a.xor_aa().ld_nn_a(OAM_PLAYER + 3); // player attr = 0
    for c in CARS {
        a.ld_a(2).ld_nn_a(c + 2); // car tile = 2
        a.xor_aa().ld_nn_a(c + 3); // car attr = 0
    }

    a.ld_a(0x93).ldh_to(0x40); // LCD on, OBJ on, BG on, tiles @ $8000

    a.xor_aa().ld_nn_a(STATE); // title screen
    a.ld_a(0xFF).ld_nn_a(LASTBTN); // nothing held at boot
    a.ld_a(1).ld_nn_a(LASTSTART);
    a.call("show_title");

    // ================= main loop =================
    a.label("loop");
    a.wait_vblank();

    a.ld_a_nn(STATE);
    a.cp(1).jr(Z_JR, "st_play");
    a.cp(2).jr(Z_JR, "st_over");

    // ---- STATE 0: TITLE — wait for a fresh Start press ----
    a.call("start_edge");
    a.jr(NZ_JR, "loop");
    a.call("begin_run");
    a.jra("loop");

    // ---- STATE 1: PLAY ----
    a.label("st_play");
    // Push live player position to OAM every frame for responsive feel.
    a.ld_a_nn(PX).ld_nn_a(OAM_PLAYER + 1);
    a.ld_a_nn(PY).ld_nn_a(OAM_PLAYER);
    a.call("input"); // edge-detected D-pad hops (with hop tick + goal check)
    // Throttle car motion to every TICKS_PER_STEP frames.
    a.ld_a_nn(TICK).inc_r(A).ld_nn_a(TICK);
    a.cp(TICKS_PER_STEP).jr(C_JR, "sp_coll"); // not yet -> still collide
    a.xor_aa().ld_nn_a(TICK);
    a.call("move_cars");
    a.label("sp_coll");
    a.call("collide");
    a.jra("loop");

    // ---- STATE 2: GAME OVER — wait for a fresh Start press ----
    a.label("st_over");
    a.call("start_edge");
    a.jr(NZ_JR, "loop");
    a.call("begin_run");
    a.jra("loop");

    // ================= start_edge: Z if Start was just pressed =================
    a.label("start_edge");
    a.ld_a(0x10).ldh_to(0x00); // select buttons
    a.ldh_from(0x00).ldh_from(0x00);
    a.and_a(0x08); // Start = bit3 (0=pressed)
    a.ld_r_r(C, A); // c = current Start bit
    a.ld_a_nn(LASTSTART);
    a.ld_r_r(B, A); // b = last
    a.ld_r_r(A, C).ld_nn_a(LASTSTART); // store current as next "last"
    a.ld_r_r(A, C).or_r(A).jr(NZ_JR, "se_no"); // current!=0 -> not pressed
    a.ld_r_r(A, B).or_r(A).jr(Z_JR, "se_no"); // last==0 (held) -> not an edge
    a.xor_aa(); // Z=1 -> edge
    a.ret();
    a.label("se_no");
    a.or_a(1); // Z=0 -> no edge
    a.ret();

    // ================= show_title =================
    a.label("show_title");
    a.call("clear_map");
    a.print(0x9800 + 4 * 32 + 7, "CROSSER");
    a.print(0x9800 + 7 * 32 + 3, "REACH THE TOP");
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.call("hide_cars");
    a.xor_aa().ld_nn_a(OAM_PLAYER + 1); // hide player (X=0)
    a.ret();

    // ================= begin_run: (re)start a fresh run =================
    a.label("begin_run");
    a.call("clear_map");
    a.ld_a(PX_START).ld_nn_a(PX);
    a.ld_a(PY_BOT).ld_nn_a(PY);
    a.ld_a_nn(PX).ld_nn_a(OAM_PLAYER + 1);
    a.ld_a_nn(PY).ld_nn_a(OAM_PLAYER);
    a.xor_aa().ld_nn_a(D0);
    a.xor_aa().ld_nn_a(D1);
    a.xor_aa().ld_nn_a(D2);
    a.xor_aa().ld_nn_a(SPEEDUP);
    a.xor_aa().ld_nn_a(TICK);
    a.ld_a(0xFF).ld_nn_a(LASTBTN); // nothing held going into the run

    // Place the two cars of each lane: shared Y, X spread half a screen apart.
    for lane in 0..5usize {
        let y = LANE_Y[lane];
        a.ld_a(y).ld_nn_a(CARS[lane * 2]);
        a.ld_a(y).ld_nn_a(CARS[lane * 2 + 1]);
        // X positions: stagger per lane so cars don't all line up.
        let x0 = 16 + (lane as u8) * 24;
        let x1 = x0.wrapping_add(84);
        let x1 = if x1 > CAR_HI { x1 - (CAR_HI - CAR_LO) } else { x1 };
        a.ld_a(x0).ld_nn_a(CARS[lane * 2] + 1);
        a.ld_a(x1).ld_nn_a(CARS[lane * 2 + 1] + 1);
    }

    a.print(SCORE_AT, "SCORE");
    a.call("drawscore");
    a.ld_a(1).ld_nn_a(STATE);
    a.ret();

    // ================= input: edge-detected D-pad hop =================
    // Reads the D-pad, edge-detects each direction against LASTBTN so one press =
    // one hop. Clamps to the screen. UP that reaches PY_TOP triggers a goal.
    a.label("input");
    a.ld_a(0x20).ldh_to(0x00); // select directions
    a.ldh_from(0x00).ldh_from(0x00);
    a.and_a(0x0F); // keep dpad nibble (0=pressed)
    a.ld_r_r(C, A); // c = current dpad bits
    a.ld_a_nn(LASTBTN);
    a.ld_r_r(B, A); // b = last dpad bits
    // store current as next frame's last
    a.ld_r_r(A, C).ld_nn_a(LASTBTN);
    // freshly-pressed = bits that are 0 now AND were 1 last frame.
    // newpressed (active-high) = (~c) AND b   -> 1 = newly pressed this frame
    a.ld_r_r(A, C).cpl(); // a = ~current (1=pressed now)
    a.and_r(B); // a = pressed-now AND was-released-last = fresh edges
    a.ld_r_r(B, A); // b = fresh-edge bits (bit0 R,1 L,2 U,3 D)

    // RIGHT (bit0)
    a.bit(0, B).jr(Z_JR, "in_l");
    a.ld_a_nn(PX).add_a(PX_STEP).ld_r_r(C, A);
    a.cp(PX_MAX + 1).jr(C_JR, "in_rok"); // <=MAX -> ok
    a.ld_a(PX_MAX).ld_r_r(C, A);
    a.label("in_rok");
    a.ld_r_r(A, C).ld_nn_a(PX);
    a.call("hop_tick");
    a.label("in_l");
    // LEFT (bit1): clamp BEFORE subtracting to avoid an 8-bit borrow/underflow.
    a.bit(1, B).jr(Z_JR, "in_u");
    a.ld_a_nn(PX).cp(PX_MIN + PX_STEP).jr(C_JR, "in_lmin"); // PX < MIN+STEP -> clamp
    a.sub_a(PX_STEP).ld_r_r(C, A); // safe: PX >= MIN+STEP
    a.jra("in_lst");
    a.label("in_lmin");
    a.ld_a(PX_MIN).ld_r_r(C, A);
    a.label("in_lst");
    a.ld_r_r(A, C).ld_nn_a(PX);
    a.call("hop_tick");
    a.label("in_u");
    // UP (bit2): move toward the top; clamp; goal if we land on PY_TOP
    a.bit(2, B).jr(Z_JR, "in_d");
    a.ld_a_nn(PY).sub_a(PY_STEP).ld_r_r(C, A);
    a.cp(PY_TOP).jr(NC_JR, "in_uok"); // >=TOP -> ok
    a.ld_a(PY_TOP).ld_r_r(C, A);
    a.label("in_uok");
    a.ld_r_r(A, C).ld_nn_a(PY);
    a.call("hop_tick");
    a.ld_a_nn(PY).cp(PY_TOP).jr(NZ_JR, "in_d"); // reached the top?
    a.call("goal");
    a.label("in_d");
    // DOWN (bit3)
    a.bit(3, B).jr(Z_JR, "in_done");
    a.ld_a_nn(PY).add_a(PY_STEP).ld_r_r(C, A);
    a.cp(PY_BOT + 1).jr(C_JR, "in_dok"); // <=BOT -> ok
    a.ld_a(PY_BOT).ld_r_r(C, A);
    a.label("in_dok");
    a.ld_r_r(A, C).ld_nn_a(PY);
    a.call("hop_tick");
    a.label("in_done");
    // refresh sprite immediately for snappy feedback
    a.ld_a_nn(PX).ld_nn_a(OAM_PLAYER + 1);
    a.ld_a_nn(PY).ld_nn_a(OAM_PLAYER);
    a.ret();

    // ================= hop_tick: short blip on a move =================
    a.label("hop_tick");
    a.tone(1500, 0xF2, 0x80);
    a.ret();

    // ================= goal: reached the top -> score++, chime, reset row ====
    a.label("goal");
    a.tone(1900, 0xF4, 0x80); // bright chime
    a.call("score_inc");
    a.ld_a(PX_START).ld_nn_a(PX); // back to centre-bottom
    a.ld_a(PY_BOT).ld_nn_a(PY);
    a.ld_a_nn(PX).ld_nn_a(OAM_PLAYER + 1);
    a.ld_a_nn(PY).ld_nn_a(OAM_PLAYER);
    // ramp difficulty: +1 added speed every crossing, capped at +3
    a.ld_a_nn(SPEEDUP).cp(3).jr(NC_JR, "goal_end");
    a.ld_a_nn(SPEEDUP).inc_r(A).ld_nn_a(SPEEDUP);
    a.label("goal_end");
    a.ret();

    // ================= move_cars: slide each lane, wrap at edges =================
    a.label("move_cars");
    for lane in 0..5usize {
        let spd = LANE_SPEED[lane];
        let dir_neg = spd >= 0x80; // moving left if high bit set
        let mag = if dir_neg { (256u16 - spd as u16) as u8 } else { spd };
        for k in 0..2usize {
            let car = CARS[lane * 2 + k];
            let wrap = a.uniq("carwrap");
            let done = a.uniq("cardone");
            let cx = car + 1; // car X lives at OAM base+1 (base+0 is Y, never touched)
            if dir_neg {
                // moving LEFT: X -= (mag + SPEEDUP); if below CAR_LO, wrap to CAR_HI
                a.ld_a_nn(SPEEDUP).add_a(mag).ld_r_r(C, A); // c = step
                a.ld_a_nn(cx).sub_r(C);
                a.cp(CAR_LO).jr(C_JR, &wrap); // X < CAR_LO -> wrap
                a.ld_nn_a(cx);
                a.jra(&done);
                a.label(&wrap);
                a.ld_a(CAR_HI).ld_nn_a(cx);
                a.label(&done);
            } else {
                // moving RIGHT: X += (mag + SPEEDUP); if above CAR_HI, wrap to CAR_LO
                a.ld_a_nn(SPEEDUP).add_a(mag).ld_r_r(C, A);
                a.ld_a_nn(cx).add_r(C);
                a.cp(CAR_HI + 1).jr(C_JR, &done); // X <= CAR_HI -> keep (a still holds X)
                a.ld_a(CAR_LO);
                a.label(&done);
                a.ld_nn_a(cx);
            }
        }
    }
    a.ret();

    // ================= collide: AABB player vs each car =================
    // Overlap if |carX-PX|<8 AND |carY-PY|<8. The (val - ref + 8) in 1..15 trick.
    a.label("collide");
    for c in CARS {
        let no = a.uniq("nohit");
        a.ld_a_nn(c + 1); // carX
        a.ld_hl(PX);
        a.sub_r(M); // carX - PX
        a.add_a(8);
        a.cp(1).jr(C_JR, &no);
        a.cp(16).jr(NC_JR, &no);
        a.ld_a_nn(c); // carY
        a.ld_hl(PY);
        a.sub_r(M); // carY - PY
        a.add_a(8);
        a.cp(1).jr(C_JR, &no);
        a.cp(16).jr(NC_JR, &no);
        a.jpa("gameover");
        a.label(&no);
    }
    a.ret();

    // ================= gameover =================
    a.label("gameover");
    a.tone(700, 0xF1, 0x80); // low hit tone
    a.call("clear_map");
    a.call("hide_cars");
    a.xor_aa().ld_nn_a(OAM_PLAYER + 1); // hide player
    a.print(0x9800 + 5 * 32 + 5, "GAME OVER");
    a.print(0x9800 + 8 * 32 + 6, "SCORE");
    a.ld_a_nn(D2).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 12);
    a.ld_a_nn(D1).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 13);
    a.ld_a_nn(D0).add_a(0x30).ld_nn_a(0x9800 + 8 * 32 + 14);
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.ld_a(2).ld_nn_a(STATE);
    a.jpa("loop");

    // ================= score_inc: decimal ripple D0/D1/D2 =================
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
    a.call("drawscore");
    a.ret();

    // ================= drawscore =================
    a.label("drawscore");
    a.ld_a_nn(D2).add_a(0x30).ld_nn_a(SCORE_AT + 6);
    a.ld_a_nn(D1).add_a(0x30).ld_nn_a(SCORE_AT + 7);
    a.ld_a_nn(D0).add_a(0x30).ld_nn_a(SCORE_AT + 8);
    a.ret();

    // ================= clear_map =================
    a.label("clear_map");
    a.memset(0x9800, 0x20, 0x0400);
    a.ret();

    // ================= hide_cars =================
    a.label("hide_cars");
    for c in CARS {
        a.xor_aa().ld_nn_a(c); // Y=0 -> hidden
        a.xor_aa().ld_nn_a(c + 1);
    }
    a.ret();

    // ================= rng: 8-bit LFSR mixed with DIV =================
    a.label("rng");
    a.ld_a_nn(RNG).add_aa().jr(NC_JR, "rng_ns").xor_a(0x1D);
    a.label("rng_ns");
    a.ld_r_r(B, A).ldh_from(0x04).xor_r(B);
    a.ld_nn_a(RNG);
    a.ret();

    // ================= tile data: 16 tiles x 16 bytes =================
    a.label("TILES");
    // 0: blank
    a.raw(&[0u8; 16]);
    // 1: player hopper — a rounded frog-like blob (color 3)
    a.raw(&solid_tile([0x3C, 0x7E, 0xFF, 0xDB, 0xFF, 0xFF, 0x7E, 0x24]));
    // 2: car/hazard — a chunky vehicle silhouette (color 3)
    a.raw(&solid_tile([0x00, 0x7E, 0xFF, 0xFF, 0xFF, 0xFF, 0x7E, 0x24]));
    // 3..15: spare (blank)
    for _ in 3..16 {
        a.raw(&[0u8; 16]);
    }

    // ================= font data (tiles $20..$5F) =================
    a.label("FONT");
    a.raw(&font_blob());

    // ================= emit ROM =================
    let rom = a.build_rom("CROSSER");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/crosser.gb", &rom).unwrap();
    println!("wrote web/crosser.gb ({} bytes code+data at $0150)", a.c.len());
}
