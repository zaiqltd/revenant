//! Builds an ORIGINAL homebrew Game Boy game — FLAP — and writes web/flap.gb.
//!
//! A bird (sprite) sits at a fixed X column and falls under gravity. Pressing A
//! or Up gives it an upward "flap" velocity. Two pipes scroll right-to-left as
//! pairs of BG-tile columns with a vertical GAP you must fly through. When a pipe
//! leaves the left edge it recycles to the right with a fresh random gap and your
//! score goes +1. Touching a pipe, the floor, or the ceiling ends the run.
//!
//! Three states drive the loop: TITLE ("FLAP" / "PRESS START"), PLAY (the game,
//! with the score drawn across the top row), and OVER ("GAME OVER" + score +
//! "PRESS START" to retry). A short flap blip and a lower death tone play.
//!
//! Hand-assembled SM83 through the shared two-pass assembler. No copyrighted
//! content (the boot-logo region is left zero; REVENANT runs from $0100).
//!
//!   cargo run --release --example makeflap   ->  web/flap.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---------------------------------------------------------------------------
// WRAM layout ($C000.. free)
// ---------------------------------------------------------------------------
const STATE: u16 = 0xC000; // 0 = title, 1 = play, 2 = game over
const BY: u16 = 0xC001; // bird Y (OAM coord; screen y = BY-16), 16..152
const VEL: u16 = 0xC002; // bird vertical velocity, biased by VBIAS (so 0..255)
const RNG: u16 = 0xC003; // 8-bit LFSR
const TICK: u16 = 0xC004; // frame divider for game logic
const D0: u16 = 0xC005; // score ones digit
const D1: u16 = 0xC006; // score tens digit
const D2: u16 = 0xC007; // score hundreds digit
const LASTBTN: u16 = 0xC008; // previous-frame Start bit (edge detect)

// Two pipes. Each: X = tile column (signed-ish 0..21, 21 = just off right edge),
// GAP = top row of the gap (gap spans GAP..GAP+GAPH-1). SCORED guards +1 once.
const P_X: [u16; 2] = [0xC010, 0xC020];
const P_GAP: [u16; 2] = [0xC011, 0xC021];
const P_SCORED: [u16; 2] = [0xC012, 0xC022];

// OAM: sprite 0 = the bird.
const OAM_BIRD: u16 = 0xFE00;
const BIRD_X: u8 = 8 + 32; // OAM X -> screen x = 32 (fixed column)

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------
const VBIAS: u8 = 128; // velocity 128 == 0; <128 up, >128 down
const GRAVITY: u8 = 1; // velocity gain per logic tick (downward)
const FLAP_VEL: u8 = VBIAS - 4; // upward kick on a flap (gentle, controllable)
const VMAX_DOWN: u8 = VBIAS + 4; // terminal fall speed (clamp)

const BY_MIN: u8 = 16 + 2; // ceiling (screen y 2)
const BY_MAX: u8 = 16 + 136; // floor (screen y 136; bird is 8px tall)

const TICKS_PER_STEP: u8 = 3; // logic runs every 3 frames (~20 Hz) -> calm pace
const PIPE_START_X: u8 = 21; // recycle column (just past the 20-wide screen)
const GAPH: u8 = 8; // gap height in tiles (8*8 = 64px) — generous/forgiving
const GAP_MIN: u8 = 4; // gap top row; gap spans GAP_MIN..GAP_MIN+RANGE (+GAPH)

// The bird's tile row at its current Y (screen y / 8). Used for tile-grid
// collision against pipe columns.
fn main() {
    let mut a = Asm::new();

    // ================= one-time setup =================
    a.label("main");
    a.di();
    a.ld_sp(0xFFFE);
    a.xor_aa().ldh_to(0x40); // LCDC = 0 (LCD off so VRAM is safe to write)

    a.apu_on();
    a.memcpy_lbl("TILES", 0x8000, 16 * 16); // game art tiles into $8000..$80FF
    a.load_font(); // font into tiles $20..$5F
    a.memset(0x9800, 0, 0x0400); // blank BG map

    a.ld_a(0xE4).ldh_to(0x47); // BGP
    a.ld_a(0xE4).ldh_to(0x48); // OBP0

    a.ldh_from(0x04).or_a(1).ld_nn_a(RNG); // seed LFSR from DIV (nonzero)

    // Static bird OAM fields (X/tile/attr never change; only Y moves).
    a.ld_a(BIRD_X).ld_nn_a(OAM_BIRD + 1);
    a.ld_a(1).ld_nn_a(OAM_BIRD + 2); // tile 1 = bird
    a.xor_aa().ld_nn_a(OAM_BIRD + 3); // attr 0

    a.ld_a(0x93).ldh_to(0x40); // LCD on, OBJ on, BG on, tiles @ $8000

    a.xor_aa().ld_nn_a(STATE); // start on the title screen
    a.xor_aa().ld_nn_a(LASTBTN);
    a.call("show_title");

    // ================= main loop =================
    a.label("loop");
    a.ld_sp(0xFFFE); // reset stack each frame so a mid-step `jp loop` can't leak
    a.wait_vblank();

    // Branch on STATE.
    a.ld_a_nn(STATE);
    a.cp(1).jr(Z_JR, "st_play");
    a.cp(2).jr(Z_JR, "st_over");
    // ---- STATE 0: TITLE — wait for a fresh Start press ----
    a.call("start_edge");
    a.jr(NZ_JR, "begin_play"); // A!=0 -> Start was newly pressed
    a.jra("loop");

    a.label("st_over");
    a.call("start_edge");
    a.jr(NZ_JR, "begin_play");
    a.jra("loop");

    a.label("st_play");
    a.call("play_step");
    a.jra("loop");

    // ---- transition into a fresh run ----
    a.label("begin_play");
    a.call("init_run");
    a.jra("loop");

    // ================= sync_start: latch the current Start bit into LASTBTN =
    // Called at every state transition so a retry/begin requires a FRESH press
    // (otherwise a Start still held from the previous state would re-trigger).
    a.label("sync_start");
    a.ld_a(0x10).ldh_to(0x00);
    a.ldh_from(0x00).ldh_from(0x00);
    a.cpl().and_a(0x08);
    a.ld_nn_a(LASTBTN);
    a.ret();

    // ================= start_edge: A=1 iff Start went 0->1 this frame =====
    a.label("start_edge");
    a.ld_a(0x10).ldh_to(0x00); // select buttons
    a.ldh_from(0x00).ldh_from(0x00); // settle the matrix
    a.cpl(); // now bit3 = 1 when Start pressed
    a.and_a(0x08); // isolate Start
    a.ld_r_r(B, A); // b = this frame's Start (0 or 8)
    a.ld_a_nn(LASTBTN); // a = last frame's Start
    a.ld_r_r(C, A);
    a.ld_r_r(A, B).ld_nn_a(LASTBTN); // save current for next frame
    // edge = current AND NOT last  -> (b) & ~(c)
    a.ld_r_r(A, C).cpl().and_r(B); // a = b & ~c  (8 if rising edge, else 0)
    a.ret();

    // ================= init_run: reset everything, enter STATE=play ======
    a.label("init_run");
    a.ld_a(16 + 64).ld_nn_a(BY); // bird mid-screen
    a.ld_a(VBIAS).ld_nn_a(VEL); // zero velocity
    a.xor_aa().ld_nn_a(TICK);
    a.xor_aa().ld_nn_a(D0).ld_nn_a(D1).ld_nn_a(D2);

    // Two pipes staggered across the right half so they arrive spaced out.
    a.ld_a(PIPE_START_X).ld_nn_a(P_X[0]);
    a.ld_a(5).ld_nn_a(P_GAP[0]);
    a.xor_aa().ld_nn_a(P_SCORED[0]);
    a.ld_a(PIPE_START_X + 11).ld_nn_a(P_X[1]); // 11 columns behind pipe 0
    a.ld_a(6).ld_nn_a(P_GAP[1]);
    a.xor_aa().ld_nn_a(P_SCORED[1]);

    a.ld_a(1).ld_nn_a(STATE);
    a.call("sync_start"); // require a fresh Start press to interact next time
    a.call("draw_world"); // paint the first frame of the field
    a.ret();

    // ================= play_step: one frame of gameplay =================
    a.label("play_step");
    // Push the live bird Y to OAM every frame (smooth even between logic ticks).
    a.ld_a_nn(BY).ld_nn_a(OAM_BIRD);

    // Flap is edge-free (hold to keep flapping is fine) — read A or Up.
    a.call("read_flap"); // A=nonzero if flap requested
    a.jr(Z_JR, "ps_nogo");
    a.ld_a(FLAP_VEL).ld_nn_a(VEL); // set upward velocity
    a.tone(1700, 0xF3, 0x80); // flap blip
    a.label("ps_nogo");

    // Throttle the heavy logic (gravity, scroll, collide, redraw).
    a.ld_a_nn(TICK).inc_r(A).ld_nn_a(TICK);
    a.cp(TICKS_PER_STEP).jr(C_JR, "ps_end");
    a.xor_aa().ld_nn_a(TICK);

    // ---- gravity: VEL += GRAVITY, clamp to terminal fall ----
    a.ld_a_nn(VEL).add_a(GRAVITY);
    a.cp(VMAX_DOWN + 1).jr(C_JR, "ps_vok");
    a.ld_a(VMAX_DOWN);
    a.label("ps_vok");
    a.ld_nn_a(VEL);

    // ---- apply velocity: BY += (VEL - VBIAS) ----
    // signed add: BY = BY + VEL - VBIAS.  Do BY += VEL, then BY -= VBIAS.
    a.ld_a_nn(BY).add_r_vel_setup(); // (custom below) -> uses helper
    // (helper emits: load VEL into C, add, sub VBIAS) — implemented inline here:

    // ---- ceiling / floor: clamp and kill ----
    a.ld_a_nn(BY).cp(BY_MIN).jr(NC_JR, "ps_notceil");
    a.jpa("die");
    a.label("ps_notceil");
    a.ld_a_nn(BY).cp(BY_MAX + 1).jr(C_JR, "ps_notfloor");
    a.jpa("die");
    a.label("ps_notfloor");

    // ---- scroll pipes left; recycle + score when they pass the edge ----
    a.call("scroll_pipes");

    // ---- collide bird vs pipe columns ----
    a.call("collide_pipes");

    // ---- redraw the field ----
    a.call("draw_world");

    a.label("ps_end");
    a.ret();

    // ================= read_flap: A != 0 if A-button or Up held ==========
    a.label("read_flap");
    // buttons: A is bit0 after selecting 0x10
    a.ld_a(0x10).ldh_to(0x00);
    a.ldh_from(0x00).ldh_from(0x00);
    a.cpl().and_a(0x01); // A-button pressed -> 1
    a.ld_r_r(B, A);
    // d-pad: Up is bit2 after selecting 0x20
    a.ld_a(0x20).ldh_to(0x00);
    a.ldh_from(0x00).ldh_from(0x00);
    a.cpl().and_a(0x04); // Up pressed -> 4
    a.or_r(B); // combine; nonzero if either
    a.ret();

    // ================= scroll_pipes: move each pipe 1 column left =========
    a.label("scroll_pipes");
    for i in 0..2 {
        let recyc = a.uniq("recyc");
        let done = a.uniq("scdone");
        let gpok = a.uniq("gprng_ok");
        a.ld_a_nn(P_X[i]);
        a.cp(0).jr(Z_JR, &recyc); // X==0 -> off the left edge, recycle
        a.dec_r(A).ld_nn_a(P_X[i]);
        a.jra(&done);
        a.label(&recyc);
        // recycle to the far right with a new random gap
        a.ld_a(PIPE_START_X).ld_nn_a(P_X[i]);
        a.call("rng");
        // gap top in GAP_MIN..GAP_MIN+5 so the whole gap (GAPH tall) stays on the
        // 18-row screen and is reachable from the bird's mid-screen flight band.
        a.and_a(0x07); // 0..7
        a.cp(6).jr(C_JR, &gpok); // clamp 6,7 -> 5 (range 0..5)
        a.ld_a(5);
        a.label(&gpok);
        a.add_a(GAP_MIN).ld_nn_a(P_GAP[i]); // top 4..9, gap rows top..top+7 (12..16 bottom)
        a.xor_aa().ld_nn_a(P_SCORED[i]); // can score again
        a.label(&done);
    }
    a.ret();

    // ================= collide_pipes: bird tile-grid vs pipe column ======
    // Bird occupies screen column 4 (x 32..39 -> tile col 4). A pipe at tile
    // column P_X is solid in every row EXCEPT GAP..GAP+GAPH-1. Collision when
    // P_X == 4 (or 5, since the bird is 8px and pipes are 2 tiles wide) AND the
    // bird's row is outside the gap. Also award the score as a pipe clears.
    a.label("collide_pipes");
    // Bird tile row = (BY - 16) / 8 — computed once, kept in D for both pipes.
    a.ld_a_nn(BY).sub_a(16);
    a.raw(&[0xCB, 0x3F, 0xCB, 0x3F, 0xCB, 0x3F]); // srl a x3 -> a = bird row (0..17)
    a.ld_r_r(D, A); // d = bird row
    for i in 0..2 {
        let no = a.uniq("cpno"); // done with this pipe
        let overlap = a.uniq("cpov"); // pipe column overlaps the bird
        let maybe_score = a.uniq("cpsc"); // pipe is behind the bird

        // The bird's fixed column is tile col 4 (screen x 32). Pipes are 2 wide
        // (columns X and X+1). Column overlap when X==3 or X==4 (so col X+1==4 or
        // col X==4). Score once the pipe's far edge has passed (X < 3).
        a.ld_a_nn(P_X[i]).cp(3).jr(Z_JR, &overlap); // X==3 -> col X+1 hits bird col 4
        a.ld_a_nn(P_X[i]).cp(4).jr(Z_JR, &overlap); // X==4 -> col X hits bird col 4
        a.ld_a_nn(P_X[i]).cp(3).jr(C_JR, &maybe_score); // X<3 -> behind bird -> score
        a.jra(&no); // X>4 -> still ahead, nothing to do

        // ---- pipe is behind the bird: award the point once ----
        a.label(&maybe_score);
        a.ld_a_nn(P_SCORED[i]).or_a(0).jr(NZ_JR, &no); // already scored
        a.ld_a(1).ld_nn_a(P_SCORED[i]);
        a.call("score_inc");
        a.jra(&no);

        // ---- column overlaps: collide unless the bird row sits in the gap ----
        a.label(&overlap);
        // gap rows: P_GAP .. P_GAP+GAPH-1.  Hit if D < P_GAP or D >= P_GAP+GAPH.
        a.ld_r_r(A, D); // a = bird row
        a.ld_hl(P_GAP[i]);
        a.cp_r(M).jp(CF, "die"); // bird row < gap top -> hit top pipe
        a.ld_a_nn(P_GAP[i]).add_a(GAPH); // a = gap bottom (exclusive)
        a.ld_r_r(C, A);
        a.ld_r_r(A, D).cp_r(C).jp(NCF, "die"); // bird row >= gap bottom -> hit bottom
        // else: safely inside the gap.
        a.label(&no);
    }
    a.ret();

    // ================= die: death tone, STATE=over, show screen ==========
    // Reached via `jp die` from the middle of play_step (its stack frame is
    // abandoned — the per-frame SP reset at `loop` cleans it up). Show the
    // game-over screen and go straight back to the top of the main loop so the
    // play_step that triggered the death does NOT continue into draw_world.
    a.label("die");
    a.tone(700, 0xF2, 0x80); // low death tone
    a.ld_a(2).ld_nn_a(STATE);
    a.call("sync_start"); // require a fresh Start press to retry
    a.call("show_over");
    a.jpa("loop");

    // ================= draw_world: blank field, draw pipes + score =======
    // Clears the visible 20x18 area then stamps the two pipe columns and the
    // score across the top row. Cheap enough at ~20 Hz.
    a.label("draw_world");
    // Blank rows 0..17, cols 0..19 (leave the off-screen columns alone).
    a.memset(0x9800, 0, 0x0400); // wipe whole map (simplest; 1KiB)
    // pipes
    for i in 0..2 {
        a.call_draw_pipe(i);
    }
    a.call("drawscore");
    a.ret();

    // ================= drawscore: D2 D1 D0 at the top-left ===============
    // Digit glyphs are tiles 6..15 (tile 6='0' ... tile 15='9').
    a.label("drawscore");
    a.ld_a_nn(D2).add_a(6).ld_nn_a(0x9800); // col 0
    a.ld_a_nn(D1).add_a(6).ld_nn_a(0x9801);
    a.ld_a_nn(D0).add_a(6).ld_nn_a(0x9802);
    a.ret();

    // ================= score_inc: ripple D0/D1/D2 (cap 999) ==============
    a.label("score_inc");
    a.ld_a_nn(D0).inc_r(A).cp(10).jr(C_JR, "si_d0");
    a.xor_aa().ld_nn_a(D0);
    a.ld_a_nn(D1).inc_r(A).cp(10).jr(C_JR, "si_d1");
    a.xor_aa().ld_nn_a(D1);
    a.ld_a_nn(D2).inc_r(A).cp(10).jr(C_JR, "si_d2");
    a.ld_a(9).ld_nn_a(D2);
    a.jra("si_done");
    a.label("si_d2");
    a.ld_nn_a(D2);
    a.jra("si_done");
    a.label("si_d1");
    a.ld_nn_a(D1);
    a.jra("si_done");
    a.label("si_d0");
    a.ld_nn_a(D0);
    a.label("si_done");
    a.ret();

    // ================= show_title / show_over: static screens ============
    a.label("show_title");
    a.memset(0x9800, 0, 0x0400);
    a.print(0x9800 + 5 * 32 + 8, "FLAP");
    a.print(0x9800 + 8 * 32 + 4, "FLY THROUGH GAPS");
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.print(0x9800 + 15 * 32 + 2, "A OR UP TO FLAP");
    a.ret();

    a.label("show_over");
    a.memset(0x9800, 0, 0x0400);
    a.print(0x9800 + 5 * 32 + 5, "GAME OVER");
    a.print(0x9800 + 8 * 32 + 6, "SCORE:");
    a.ld_a_nn(D2).add_a(6).ld_nn_a(0x9800 + 8 * 32 + 13);
    a.ld_a_nn(D1).add_a(6).ld_nn_a(0x9800 + 8 * 32 + 14);
    a.ld_a_nn(D0).add_a(6).ld_nn_a(0x9800 + 8 * 32 + 15);
    a.print(0x9800 + 12 * 32 + 4, "PRESS START");
    a.ret();

    // ================= rng: 8-bit LFSR mixed with DIV ===================
    a.label("rng");
    a.ld_a_nn(RNG).add_aa().jr(NC_JR, "rng_ns").xor_a(0x1D);
    a.label("rng_ns");
    a.ld_r_r(B, A).ldh_from(0x04).xor_r(B);
    a.ld_nn_a(RNG);
    a.ret();

    // ================= tile data: 16 tiles x 16 bytes ===================
    a.label("TILES");
    a.raw(&[0u8; 16]); // 0: blank
    // 1: bird — a round body with a beak/eye
    a.raw(&solid_tile([0x3C, 0x7E, 0xFF, 0xDF, 0xFF, 0xFF, 0x7E, 0x3C]));
    // 2: pipe body — solid block with a vertical seam
    a.raw(&solid_tile([0xFF, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7, 0xFF]));
    // 3,4,5: spare
    a.raw(&[0u8; 16]);
    a.raw(&[0u8; 16]);
    a.raw(&[0u8; 16]);
    // 6..15: digits '0'..'9'
    for g in DIGITS {
        a.raw(&solid_tile(g));
    }

    // ================= font data ========================================
    a.label("FONT");
    a.raw(&font_blob());

    // ================= emit ROM =========================================
    let rom = a.build_rom("FLAP");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/flap.gb", &rom).unwrap();
    println!("wrote web/flap.gb ({} bytes code+data at $0150)", a.c.len());
}

// ---------------------------------------------------------------------------
// Small emit helpers kept as a trait so the main flow reads cleanly.
// ---------------------------------------------------------------------------
trait FlapAsm {
    fn add_r_vel_setup(&mut self) -> &mut Self;
    fn call_draw_pipe(&mut self, i: usize) -> &mut Self;
}
impl FlapAsm for Asm {
    /// With A = current BY, compute BY = BY + (VEL - VBIAS) and store it, clamped
    /// only by the ceiling/floor checks that follow. A holds the signed delta add.
    fn add_r_vel_setup(&mut self) -> &mut Self {
        // a currently = BY. delta = VEL - VBIAS (signed). new = BY + VEL - VBIAS.
        self.ld_r_r(B, A); // b = BY
        self.ld_a_nn(VEL); // a = VEL
        self.add_r(B); // a = VEL + BY
        self.sub_a(VBIAS); // a = BY + VEL - VBIAS
        self.ld_nn_a(BY)
    }

    /// Draw pipe `i`: if its column is on-screen (X 0..19), stamp pipe tiles in
    /// every visible row except the gap, in BOTH this column and the one to its
    /// right (pipes are 2 tiles wide for presence).
    fn call_draw_pipe(&mut self, i: usize) -> &mut Self {
        let off = self.uniq("pdoff");
        let loop_l = self.uniq("pdloop");
        let skipgap = self.uniq("pdgap");
        let cont = self.uniq("pdcont");
        // X must be <= 19 to be visible (and < PIPE_START_X handled by <=19).
        self.ld_a_nn(P_X[i]).cp(20).jr(NC_JR, &off); // X>=20 -> off-screen, skip
        // Compute base map addr = 0x9800 + X.  HL = 0x9800 + X.
        self.ld_a_nn(P_X[i]).ld_r_r(E, A).ld_r_n(D, 0); // de = X
        self.ld_hl(0x9800).add_hl_de(); // hl = 0x9800 + X

        // Loop rows 0..17 (B = row counter), advancing HL by 32 each row.
        self.ld_r_n(B, 18);
        self.ld_r_n(C, 0); // c = current row index
        self.label(&loop_l);
        // if row (C) within gap -> leave blank, else write pipe tile (2).
        // in-gap when  GAP <= C < GAP+GAPH.
        self.ld_a_nn(P_GAP[i]).ld_r_r(E, A); // e = gap top
        self.ld_r_r(A, C).cp_r(E).jr(C_JR, &cont); // C < gap top -> solid
        self.ld_a_nn(P_GAP[i]).add_a(GAPH).ld_r_r(E, A); // e = gap bottom
        self.ld_r_r(A, C).cp_r(E).jr(C_JR, &skipgap); // C < gap bottom -> in gap
        self.label(&cont);
        // write pipe tile at (HL) and (HL+1)
        self.ld_a(2).ld_hl_a(); // (HL) = pipe
        self.inc_hl();
        self.ld_a(2).ld_hl_a(); // (HL+1) = pipe (2-wide)
        self.dec_hl();
        self.jra(&format!("{skipgap}_adv"));
        self.label(&skipgap);
        // in gap: nothing drawn (map already blanked by draw_world)
        self.label(&format!("{skipgap}_adv"));
        // advance HL by 32 (next row), inc row counter, loop
        self.push_bc();
        self.ld_de(32);
        self.add_hl_de();
        self.pop_bc();
        self.inc_r(C);
        self.dec_r(B).jr(NZ_JR, &loop_l);
        self.label(&off);
        self
    }
}

// 5x7-ish digit glyphs for the score readout (color 3 via solid_tile).
const DIGITS: [[u8; 8]; 10] = [
    [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00], // 0
    [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00], // 1
    [0x3C, 0x66, 0x06, 0x0C, 0x30, 0x60, 0x7E, 0x00], // 2
    [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00], // 3
    [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00], // 4
    [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00], // 5
    [0x3C, 0x66, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00], // 6
    [0x7E, 0x66, 0x0C, 0x18, 0x18, 0x18, 0x18, 0x00], // 7
    [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00], // 8
    [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x66, 0x3C, 0x00], // 9
];
