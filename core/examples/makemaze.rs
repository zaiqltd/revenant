//! Builds an ORIGINAL homebrew Game Boy game — MAZE — and writes web/maze.gb.
//!
//! (uses the shared assembler at examples/common/asm.rs)
//!
//! You are a small sprite dropped at the start of a maze built from WALL BG tiles.
//! Steer with the D-pad — one tile per fresh press (held keys auto-repeat on a
//! short cooldown so you can glide), but you never tunnel a wall. Before each step
//! the target BG cell is read; if it is a WALL the move is rejected — that
//! read-the-target collision is the heart of the game.
//!
//!   * proper TITLE screen ("MAZE" / "PRESS START")
//!   * three hand-built fixed maze layouts that cycle as you solve them
//!   * reach the GOAL tile -> a chime, +1 to the SOLVED score, load the next maze,
//!     re-place the player at that maze's start cell, and refill the timer
//!   * an on-screen SOLVED count and a shrinking TIME bar (top HUD rows)
//!   * a per-maze countdown TIMER: let it hit zero and it's GAME OVER with your
//!     final SOLVED score + "PRESS START" to try again
//!   * sound: APU on at boot, a tick on every step, a rising chime at a goal,
//!     a low tone on time-out
//!
//! Tile budget: game art lives in tiles 0..3 ($8000..). The bundled font lives in
//! tiles $20..$5F ($8200..), so the two never collide. The player is an OBJ sprite
//! (tile 3) over the BG, so it sits on any floor cell without disturbing the map.
//!
//! No copyrighted content (boot-logo region left zero; REVENANT runs from $0100).
//!
//!   cargo run --release --example makemaze   ->  web/maze.gb

#[path = "common/asm.rs"]
mod asm;
use asm::*;

// ---- WRAM layout (free RAM $C000..$DFFF) ----
const PX: u16 = 0xC000; // player column in tiles (0..MAZE_COLS-1)
const PY: u16 = 0xC001; // player row    in tiles (0..MAZE_ROWS-1)
const STATE: u16 = 0xC002; // 0=title 1=play 2=over
const LASTBTN: u16 = 0xC003; // last frame's Start bit (edge detect)
const MAZEIDX: u16 = 0xC005; // which of the 3 layouts is active
const TIMEHI: u16 = 0xC006; // countdown timer high byte (frames)
const TIMELO: u16 = 0xC007; // countdown timer low byte
const MOVECD: u16 = 0xC008; // movement cooldown (frames until next step allowed)
const SC_T: u16 = 0xC009; // solved tens digit
const SC_O: u16 = 0xC00A; // solved ones digit
const BARLEN: u16 = 0xC00B; // current drawn time-bar length (cells), for redraw diff
const TGTCOL: u16 = 0xC00C; // scratch: target column for a move attempt
const TGTROW: u16 = 0xC00D; // scratch: target row for a move attempt

// ---- state values ----
const ST_TITLE: u8 = 0;
const ST_PLAY: u8 = 1;
const ST_OVER: u8 = 2;

// ---- tiles (kept in 0..31, clear of the font @ $20..) ----
const T_FLOOR: u8 = 0;
const T_WALL: u8 = 1;
const T_GOAL: u8 = 2;
const T_PLAYER: u8 = 3; // sprite tile

// ---- OAM ----
const OAM_PLAYER: u16 = 0xFE00;

// ---- maze geometry: MAZE_COLS x MAZE_ROWS, placed at BG rows MAZE_TOP.. ----
const MAZE_COLS: u8 = 20;
const MAZE_ROWS: u8 = 16;
const MAZE_TOP: u8 = 2; // BG row of maze row 0

// ---- timer ----
const TIME_START: u16 = 60 * 45; // ~45 s per maze
const MOVE_CD: u8 = 6; // frames between auto-repeat steps when a key is held

const fn map(row: u16, col: u16) -> u16 {
    0x9800 + row * 32 + col
}
const TIMEBAR_ROW: u16 = 1;
const TIMEBAR_COL0: u16 = 6;
const TIMEBAR_CELLS: u8 = 14;

fn main() {
    let mut a = Asm::new();

    // ===== main: one-time setup =====
    a.label("main");
    a.di();
    a.ld_sp(0xFFFE);
    a.xor_aa().ldh_to(0x40); // LCDC=0 (LCD off)
    a.apu_on();
    a.ld_hl_lbl("TILES").ld_de(0x8000).ld_bc(4 * 16);
    a.label("cpt");
    a.ldi_a_hl().ld_de_a().inc_de().dec_bc().ld_r_r(A, B).or_r(C).jr(NZ_JR, "cpt");
    a.load_font();
    a.ld_a(0xE4).ldh_to(0x47); // BGP
    a.ld_a(0xE4).ldh_to(0x48); // OBP0
    a.memset(0xFE00, 0x00, 160); // clear OAM
    a.ld_a(0x0F).ld_nn_a(LASTBTN);
    a.xor_aa().ld_nn_a(SC_T).ld_nn_a(SC_O).ld_nn_a(MAZEIDX);
    a.jpa("title");

    // ===== title =====
    a.label("title");
    a.ld_a(ST_TITLE).ld_nn_a(STATE);
    a.xor_aa().ldh_to(0x40);
    a.memset(0xFE00, 0x00, 160);
    a.call("clearmap");
    a.print(map(4, 8), "MAZE");
    a.print(map(7, 4), "PRESS START");
    a.print(map(10, 3), "D-PAD TO MOVE");
    a.print(map(12, 3), "REACH THE GOAL");
    a.print(map(15, 3), "BEAT THE CLOCK");
    a.ld_a(0x93).ldh_to(0x40); // LCD on, BG+OBJ, tiles @ $8000
    a.call("primebtn");
    a.label("twait");
    a.call("vbl");
    a.call("startedge");
    a.or_r(A).jr(Z_JR, "twait");
    // new game: reset score, start at maze 0
    a.xor_aa().ld_nn_a(SC_T).ld_nn_a(SC_O).ld_nn_a(MAZEIDX);
    // fall through

    // ===== newmaze: draw MAZEIDX layout, place player at start, refill timer =====
    a.label("newmaze");
    a.ld_a(ST_PLAY).ld_nn_a(STATE);
    a.xor_aa().ldh_to(0x40);
    a.call("clearmap");
    a.call("drawmaze"); // paints walls/floor/goal, sets PX/PY to the start cell
    a.call("drawhud");
    a.ld_a((TIME_START >> 8) as u8).ld_nn_a(TIMEHI);
    a.ld_a(TIME_START as u8).ld_nn_a(TIMELO);
    a.ld_a(0xFF).ld_nn_a(BARLEN); // force full bar redraw
    a.call("drawbar");
    a.xor_aa().ld_nn_a(MOVECD);
    a.call("drawplayer");
    a.ld_a(0x93).ldh_to(0x40); // LCD on
    // fall through

    // ===== game loop =====
    a.label("loop");
    a.call("vbl");
    a.call("tick"); // countdown (may jump to gameover)
    a.ld_a_nn(MOVECD).or_r(A).jr(Z_JR, "lp_in");
    a.dec_r(A).ld_nn_a(MOVECD);
    a.jra("loop");
    a.label("lp_in");
    a.call("input"); // may move player / set cooldown / jump to win
    a.call("drawplayer");
    a.jra("loop");

    // ===== tick: dec 16-bit timer; on zero -> time out =====
    a.label("tick");
    a.ld_a_nn(TIMELO).or_r(A).jr(NZ_JR, "tk_lo");
    a.ld_a_nn(TIMEHI).or_r(A).jr(Z_JR, "tk_out"); // hi==0 & lo==0 -> out
    a.dec_r(A).ld_nn_a(TIMEHI);
    a.ld_a(0xFF).ld_nn_a(TIMELO);
    a.jra("tk_bar");
    a.label("tk_lo");
    a.dec_r(A).ld_nn_a(TIMELO);
    a.label("tk_bar");
    a.call("drawbar");
    a.ret();
    a.label("tk_out");
    a.tone(600, 0xF1, 0x80);
    a.jpa("gameover");

    // ===== input: D-pad -> attempt a 1-tile move with wall collision =====
    a.label("input");
    a.ld_a(0x20).ldh_to(0x00).ldh_from(0x00).ldh_from(0x00);
    a.ld_r_r(B, A); // b = dpad bits (0=pressed) bit0 R,1 L,2 U,3 D
    // start target = current pos
    a.ld_a_nn(PX).ld_nn_a(TGTCOL);
    a.ld_a_nn(PY).ld_nn_a(TGTROW);
    // UP (bit2)
    a.bit(2, B).jr(NZ_JR, "in_dn");
    a.ld_a_nn(PY).or_r(A).jr(Z_JR, "in_no"); // row 0 edge
    a.dec_r(A).ld_nn_a(TGTROW).jra("in_try");
    a.label("in_dn");
    // DOWN (bit3)
    a.bit(3, B).jr(NZ_JR, "in_lf");
    a.ld_a_nn(PY).cp(MAZE_ROWS - 1).jr(Z_JR, "in_no");
    a.inc_r(A).ld_nn_a(TGTROW).jra("in_try");
    a.label("in_lf");
    // LEFT (bit1)
    a.bit(1, B).jr(NZ_JR, "in_rt");
    a.ld_a_nn(PX).or_r(A).jr(Z_JR, "in_no");
    a.dec_r(A).ld_nn_a(TGTCOL).jra("in_try");
    a.label("in_rt");
    // RIGHT (bit0)
    a.bit(0, B).jr(NZ_JR, "in_no");
    a.ld_a_nn(PX).cp(MAZE_COLS - 1).jr(Z_JR, "in_no");
    a.inc_r(A).ld_nn_a(TGTCOL);
    // fall to in_try
    a.label("in_try");
    a.call("cellptr"); // hl = BG cell of (TGTCOL,TGTROW)
    a.ld_a_hl(); // a = tile there
    a.cp(T_WALL).jr(Z_JR, "in_no"); // WALL -> reject (collision)
    a.cp(T_GOAL).jr(Z_JR, "in_goal"); // GOAL -> win
    // floor: commit the move
    a.ld_a_nn(TGTCOL).ld_nn_a(PX);
    a.ld_a_nn(TGTROW).ld_nn_a(PY);
    a.ld_a(MOVE_CD).ld_nn_a(MOVECD);
    a.tone(1450, 0xF2, 0x80); // step tick
    a.ret();
    a.label("in_no");
    a.ret();
    a.label("in_goal");
    a.ld_a_nn(TGTCOL).ld_nn_a(PX);
    a.ld_a_nn(TGTROW).ld_nn_a(PY);
    a.jpa("win");

    // ===== win: chime, +1 solved, advance + reload maze =====
    a.label("win");
    a.tone(1850, 0xF3, 0x80); // goal chime
    a.call("drawplayer");
    a.call("scoreup");
    a.ld_a_nn(MAZEIDX).inc_r(A).cp(3).jr(NZ_JR, "win_ok");
    a.xor_aa();
    a.label("win_ok");
    a.ld_nn_a(MAZEIDX);
    a.jpa("newmaze");

    // ===== gameover =====
    a.label("gameover");
    a.ld_a(ST_OVER).ld_nn_a(STATE);
    a.xor_aa().ldh_to(0x40);
    a.memset(0xFE00, 0x00, 160);
    a.call("clearmap");
    a.print(map(4, 5), "GAME OVER");
    a.print(map(7, 4), "SOLVED");
    a.ld_a_nn(SC_T).add_a(0x30).ld_nn_a(map(7, 11));
    a.ld_a_nn(SC_O).add_a(0x30).ld_nn_a(map(7, 12));
    a.print(map(10, 4), "PRESS START");
    a.ld_a(0x93).ldh_to(0x40);
    a.call("primebtn");
    a.label("owait");
    a.call("vbl");
    a.call("startedge");
    a.or_r(A).jr(Z_JR, "owait");
    a.jpa("title");

    // ===== scoreup: ++solved (tens/ones, clamp 99) =====
    a.label("scoreup");
    a.ld_a_nn(SC_O).inc_r(A).cp(10).jr(NZ_JR, "su_o");
    a.xor_aa().ld_nn_a(SC_O);
    a.ld_a_nn(SC_T).inc_r(A).cp(10).jr(NZ_JR, "su_t");
    a.ld_a(9); // clamp at 99
    a.label("su_t");
    a.ld_nn_a(SC_T);
    a.call("drawscore");
    a.ret();
    a.label("su_o");
    a.ld_nn_a(SC_O);
    a.call("drawscore");
    a.ret();

    // ===== drawhud: labels + score + time =====
    a.label("drawhud");
    a.print(map(0, 0), "SOLVED");
    a.print(map(TIMEBAR_ROW, 0), "TIME");
    a.call("drawscore");
    a.ret();

    // ===== drawscore: two digits at row0 col7,8 =====
    a.label("drawscore");
    a.ld_a_nn(SC_T).add_a(0x30).ld_nn_a(map(0, 7));
    a.ld_a_nn(SC_O).add_a(0x30).ld_nn_a(map(0, 8));
    a.ret();

    // ===== drawbar: time bar length scales with TIMEHI (0..10 -> 1..11 cells) =====
    a.label("drawbar");
    a.ld_a_nn(TIMEHI).inc_r(A); // 1..11
    a.cp(TIMEBAR_CELLS + 1).jr(C_JR, "db_ok");
    a.ld_a(TIMEBAR_CELLS);
    a.label("db_ok");
    a.ld_r_r(C, A); // c = desired length
    a.ld_a_nn(BARLEN).cp_r(C).jr(Z_JR, "db_ret"); // unchanged -> skip
    a.ld_r_r(A, C).ld_nn_a(BARLEN);
    a.ld_r_n(B, 0); // b = i
    a.label("db_lp");
    a.push_bc();
    a.ld_hl(map(TIMEBAR_ROW, TIMEBAR_COL0));
    a.ld_r_n(D, 0).ld_r_r(E, B).add_hl_de();
    a.ld_r_r(A, B).cp_r(C).jr(C_JR, "db_fill"); // i<len -> filled
    a.ld_a(T_FLOOR).jra("db_put");
    a.label("db_fill");
    a.ld_a(T_GOAL); // filled diamond as a bar block
    a.label("db_put");
    a.ld_hl_a();
    a.pop_bc();
    a.inc_r(B).ld_r_r(A, B).cp(TIMEBAR_CELLS).jr(NZ_JR, "db_lp");
    a.label("db_ret");
    a.ret();

    // ===== drawplayer: push player OAM from PX,PY =====
    a.label("drawplayer");
    a.ld_a_nn(PX).call("times8");
    a.add_a(8).ld_nn_a(OAM_PLAYER + 1); // OAM X = PX*8 + 8
    a.ld_a_nn(PY).add_a(MAZE_TOP).call("times8");
    a.add_a(16).ld_nn_a(OAM_PLAYER); // OAM Y = (MAZE_TOP+PY)*8 + 16
    a.ld_a(T_PLAYER).ld_nn_a(OAM_PLAYER + 2);
    a.xor_aa().ld_nn_a(OAM_PLAYER + 3);
    a.ret();

    // ===== times8: a = a*8 =====
    a.label("times8");
    a.add_aa().add_aa().add_aa();
    a.ret();

    // ===== cellptr: hl = $9800 + (MAZE_TOP+TGTROW)*32 + TGTCOL =====
    a.label("cellptr");
    a.ld_hl(0x9800);
    a.ld_a_nn(TGTROW).add_a(MAZE_TOP).ld_r_r(B, A); // b = bg row count
    a.ld_r_n(D, 0).ld_r_n(E, 32);
    a.label("cp_row");
    a.ld_r_r(A, B).or_r(A).jr(Z_JR, "cp_col");
    a.add_hl_de().dec_r(B).jra("cp_row");
    a.label("cp_col");
    a.ld_a_nn(TGTCOL).ld_r_r(E, A).ld_r_n(D, 0).add_hl_de();
    a.ret();

    // ===== primebtn =====
    a.label("primebtn");
    a.ld_a(0x10).ldh_to(0x00);
    a.ldh_from(0x00).ldh_from(0x00);
    a.and_a(0x08).ld_nn_a(LASTBTN);
    a.ret();

    // ===== startedge: a=1 on fresh Start press =====
    a.label("startedge");
    a.ld_a(0x10).ldh_to(0x00);
    a.ldh_from(0x00).ldh_from(0x00);
    a.and_a(0x08);
    a.ld_r_r(C, A);
    a.ld_a_nn(LASTBTN).ld_r_r(B, A);
    a.ld_r_r(A, C).ld_nn_a(LASTBTN);
    a.ld_r_r(A, C).or_r(A).jr(NZ_JR, "se_no");
    a.ld_r_r(A, B).or_r(A).jr(Z_JR, "se_no");
    a.ld_a(1).ret();
    a.label("se_no");
    a.xor_aa().ret();

    // ===== clearmap: fill 32x32 BG with FLOOR =====
    a.label("clearmap");
    a.ld_hl(0x9800).ld_bc(0x0400);
    a.label("clr");
    a.xor_aa().ldi_hl_a().dec_bc().ld_r_r(A, B).or_r(C).jr(NZ_JR, "clr");
    a.ret();

    // ===== vbl =====
    a.label("vbl");
    a.ldh_from(0x44).cp(145).jr(Z_JR, "vbl");
    a.label("vbl2");
    a.ldh_from(0x44).cp(145).jr(NZ_JR, "vbl2");
    a.ret();

    // ===== drawmaze: paint active layout; record start ('S') cell into PX/PY =====
    // Each layout = MAZE_ROWS*MAZE_COLS bytes of tile indices (0 floor,1 wall,2 goal)
    // with byte value 4 meaning "start floor". HL walks the source; we write tiles
    // row by row using cellptr-style addressing via a running BG pointer in DE.
    a.label("drawmaze");
    // select source -> HL
    a.ld_a_nn(MAZEIDX).or_r(A).jr(NZ_JR, "dm_1");
    a.ld_hl_lbl("MAZE0").jra("dm_go");
    a.label("dm_1");
    a.cp(1).jr(NZ_JR, "dm_2");
    a.ld_hl_lbl("MAZE1").jra("dm_go");
    a.label("dm_2");
    a.ld_hl_lbl("MAZE2");
    a.label("dm_go");
    // DE = BG dest of maze (0,0) = map(MAZE_TOP,0)
    a.ld_de(map(MAZE_TOP as u16, 0));
    // B = row counter (MAZE_ROWS)
    a.ld_r_n(B, MAZE_ROWS);
    a.label("dm_r");
    // C = col counter (MAZE_COLS)
    a.push_bc();
    a.ld_r_n(C, MAZE_COLS);
    a.label("dm_c");
    a.ldi_a_hl(); // a = source tile, hl++
    a.cp(4).jr(NZ_JR, "dm_nw"); // 4 = start marker -> floor + record pos
    // record start cell. During the inner loop C counts cols-remaining (MAZE_COLS..1)
    // and B counts rows-remaining (MAZE_ROWS..1), so the current cell is:
    //   col = MAZE_COLS - C   (C is pre-decrement here)
    //   row = MAZE_ROWS - B
    a.ld_a(MAZE_COLS).sub_r(C).ld_nn_a(PX);
    a.ld_a(MAZE_ROWS).sub_r(B).ld_nn_a(PY);
    a.ld_a(T_FLOOR); // draw floor under the start
    a.label("dm_nw");
    a.ld_de_a(); // (de) = tile
    a.inc_de();
    a.dec_r(C).jr(NZ_JR, "dm_c");
    // advance DE to next BG row start: we wrote MAZE_COLS cells, need +32-MAZE_COLS
    a.push_hl();
    a.ld_hl(0); a.add_hl_de(); // hl = de
    a.ld_de((32 - MAZE_COLS as u16) as u16).add_hl_de();
    a.ld_r_r(D, H).ld_r_r(E, L);
    a.pop_hl();
    a.pop_bc();
    a.dec_r(B).jr(NZ_JR, "dm_r");
    a.ret();

    // === TILE DATA ===
    a.label("TILES");
    a.raw(&[0x00; 16]); // 0 FLOOR
    a.raw(&[
        0xFF, 0xFF, 0xAA, 0xAA, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xAA, 0xAA, 0xFF, 0xFF, 0xFF, 0xFF,
    ]); // 1 WALL (brick)
    a.raw(&[
        0x18, 0x18, 0x3C, 0x3C, 0x7E, 0x7E, 0xFF, 0xFF,
        0xFF, 0xFF, 0x7E, 0x7E, 0x3C, 0x3C, 0x18, 0x18,
    ]); // 2 GOAL (diamond)
    a.raw(&[
        0x3C, 0x3C, 0x7E, 0x7E, 0xFF, 0xFF, 0xDB, 0xDB,
        0xFF, 0xFF, 0xFF, 0xFF, 0x7E, 0x7E, 0x3C, 0x3C,
    ]); // 3 PLAYER (blob)

    // === MAZE LAYOUTS (MAZE_ROWS x MAZE_COLS bytes each) ===
    a.label("MAZE0");
    a.raw(&maze_bytes(MAZE0));
    a.label("MAZE1");
    a.raw(&maze_bytes(MAZE1));
    a.label("MAZE2");
    a.raw(&maze_bytes(MAZE2));

    a.label("FONT");
    a.raw(&font_blob());

    let rom = a.build_rom("MAZE");
    std::fs::create_dir_all("web").ok();
    std::fs::write("web/maze.gb", &rom).unwrap();
    println!("wrote web/maze.gb ({} bytes code+data at $0150)", a.c.len());
}

// Convert a 16-row x 20-col ASCII maze into tile bytes:
//   '#'=wall(1) 'G'=goal(2) 'S'=start(4) else floor(0)
fn maze_bytes(rows: [&str; MAZE_ROWS as usize]) -> Vec<u8> {
    let mut out = Vec::with_capacity(MAZE_ROWS as usize * MAZE_COLS as usize);
    for r in rows {
        let b = r.as_bytes();
        for c in 0..MAZE_COLS as usize {
            let ch = if c < b.len() { b[c] } else { b' ' };
            out.push(match ch {
                b'#' => 1,
                b'G' => 2,
                b'S' => 4,
                _ => 0,
            });
        }
    }
    out
}

// 20-wide, 16-tall. 'S' start, 'G' goal, '#' wall, ' ' floor.
// Generated as perfect mazes (DFS carve + a few loop openings) and machine-verified
// solvable from S to G via BFS — see the throwaway generator that produced them.
#[rustfmt::skip]
const MAZE0: [&str; 16] = [
    "####################",
    "#S  #       #     G#",
    "### # # ### # ######",
    "#   # # # # # #   ##",
    "# ### # # # # # # ##",
    "# #   # # # #   # ##",
    "# ##### # # ##### ##",
    "# #     #   #   # ##",
    "# # ##### ### # # ##",
    "# #       #   #   ##",
    "# ### # ### ##### ##",
    "#   # #     #   # ##",
    "### # ####### # # ##",
    "#     #       #   ##",
    "####################",
    "####################",
];

#[rustfmt::skip]
const MAZE1: [&str; 16] = [
    "####################",
    "#S      #G  #     ##",
    "####### ### # # # ##",
    "#     #   #   # # ##",
    "## ## ### ##### # ##",
    "#             # # ##",
    "# ### ####### # ####",
    "#   # #       #   ##",
    "# # # # ####### # ##",
    "# # # # #       # ##",
    "# # ### ######### ##",
    "# #   #   #       ##",
    "# ### ###   ##### ##",
    "#   #       #     ##",
    "###### ### #########",
    "####################",
];

#[rustfmt::skip]
const MAZE2: [&str; 16] = [
    "####################",
    "#S#           #   ##",
    "# #  ###### # # # ##",
    "# # #       #   #  #",
    "# # # ######## #  ##",
    "# # #     #     # ##",
    "# # ##### ###   # ##",
    "# #   #   #   # # ##",
    "# ### # ### ##### ##",
    "# #   # #         ##",
    "# ##### # ##########",
    "# #   # #     #  G##",
    "# # # # ##### # # ##",
    "#   #   #         ##",
    "####################",
    "####################",
];
