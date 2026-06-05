//! Picture Processing Unit.
//!
//! Timing model is dot-accurate: 456 dots/line, 154 lines, Mode 2 (OAM scan, 80
//! dots) -> Mode 3 (drawing, variable) -> Mode 0 (HBlank) -> Mode 1 (VBlank).
//! STAT raises an interrupt only on the rising edge of the ORed condition line.
//!
//! Mode 3 is a TRUE pixel FIFO: a background/window fetcher state machine feeds an
//! 8-pixel BG FIFO, sprites are fetched on demand into a sprite FIFO, and one pixel
//! is shifted to the LCD per dot. Mode 3 length therefore *emerges* from the
//! fetcher (base 172 + SCX&7 + window + per-sprite penalties) rather than from a
//! precomputed formula. Every register that affects a pixel (BGP/OBP/LCDC/SCX/WX,
//! CGB palettes) is latched AT THE DOT THE PIXEL IS PRODUCED, so a mid-Mode-3
//! write changes only the pixels drawn after it (mealybug-tearoom).

pub const W: usize = 160;
pub const H: usize = 144;

#[derive(Clone, Copy)]
struct BgPixel {
    color: u8,   // 0..3
    palette: u8, // CGB BG palette 0..7 (DMG: unused)
    prio: bool,  // CGB BG attr bit7 (BG-to-OBJ priority)
}

// Snapshot of the registers whose mid-Mode-3 writes are observed with a pipeline
// delay (palettes + the LCDC bits / SCX read at the output/fetch stage).
#[derive(Clone, Copy)]
struct RegSnap {
    bgp: u8,
    obp0: u8,
    obp1: u8,
    lcdc: u8,
}

#[derive(Clone, Copy)]
struct SpPixel {
    color: u8,    // 0..3 (0 = transparent)
    palette: u8,  // DMG: 0=OBP0 1=OBP1; CGB: OBP 0..7
    behind: bool, // OAM attr bit7 (OBJ-to-BG priority)
    oam_idx: u8,  // for CGB tie-break / DMG bookkeeping
}

// A tiny ring-ish FIFO of up to 16 BG pixels.
#[derive(Clone)]
struct BgFifo {
    data: [BgPixel; 16],
    head: usize,
    len: usize,
}
impl BgFifo {
    fn new() -> Self {
        BgFifo { data: [BgPixel { color: 0, palette: 0, prio: false }; 16], head: 0, len: 0 }
    }
    fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
    fn push(&mut self, p: BgPixel) {
        let i = (self.head + self.len) & 15;
        self.data[i] = p;
        self.len += 1;
    }
    fn pop(&mut self) -> BgPixel {
        let p = self.data[self.head];
        self.head = (self.head + 1) & 15;
        self.len -= 1;
        p
    }
}

#[derive(Clone)]
pub struct Ppu {
    pub cgb: bool,

    // memory
    vram: Vec<u8>, // 0x2000 * (2 banks on CGB)
    oam: [u8; 0xA0],
    vbk: usize, // current VRAM bank (CGB)

    // registers
    pub lcdc: u8,
    pub stat: u8,
    pub scy: u8,
    pub scx: u8,
    pub ly: u8,
    pub lyc: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,
    pub wy: u8,
    pub wx: u8,

    // CGB palettes
    bcps: u8,
    bg_pal: [u8; 64],
    ocps: u8,
    obj_pal: [u8; 64],
    pub opri: u8, // object priority mode (FF6C): bit0 = DMG priority (by X)

    // timing
    pub mode: u8,
    dot_in_line: u32,
    // Physical scanline 0..=153. Distinct from the reported `ly` register, which
    // wraps 153->0 early on the last VBlank line (the LY=153 quirk).
    phys_line: u8,
    window_line: u8,
    window_y_triggered: bool,
    stat_line: bool,
    just_enabled: bool,
    // For one dot at the physical line 153 -> 0 boundary the LYC coincidence is
    // forced false, so an LYC=0 source produces a *fresh* rising edge at line 0
    // distinct from the one it already produced during the line-153 early wrap.
    lyc_suppress: bool,

    // selected sprites for current line (OAM byte indices)
    line_sprites: Vec<u8>,

    // ---- Mode-3 pixel pipeline state ----
    fetch: Fetcher,
    bg_fifo: BgFifo,
    sp_fifo: [SpPixel; 8], // sprite FIFO (fixed 8, transparent = color 0)
    x_out: i32,            // pixels shipped to the LCD this line (0..160)
    to_discard: u8,        // SCX&7 fine-scroll pixels still to drop from BG FIFO
    win_active: bool,      // window currently being fetched
    win_x: u8,             // window fetcher tile X counter
    window_drawn_line: bool,
    first_fetch: bool,     // the first tile fetch of the line is a discarded warm-up
    // sprite fetch in progress
    sp_fetch_active: bool,
    sp_fetch_idx: usize,        // index into line_sprites
    sp_fetch_step: u8,          // 0..=5 sub-steps of an object fetch
    sprite_done: [bool; 10],    // which selected sprites have already been fetched

    // Mid-Mode-3 register writes reach the pixel pipeline a few dots after the CPU
    // issues them (the CPU write lands at an M-cycle boundary, but the rendered
    // pixel that consumes it is a fixed pipeline distance downstream). We model this
    // by snapshotting the latch-relevant registers each dot and consuming the value
    // from REG_LATCH_DELAY dots ago when a pixel is produced. Outside Mode 3 (and in
    // ROMs that never poke mid-line, e.g. acid2) the delayed value equals the live
    // one, so this is invisible there.
    reg_hist: [RegSnap; Self::REG_HIST],
    reg_head: usize,

    pub vblank_irq: bool,
    pub stat_irq: bool,
    pub frame_ready: bool,

    // output: RGBA8888
    pub fb: Vec<u8>,
}

#[derive(Clone)]
struct Fetcher {
    step: u8,     // 0=GetTile 1=GetLow 2=GetHigh 3=Push
    substep: u8,  // 0/1 within a 2-dot step
    x: u8,        // background tile X counter (0..)
    tile_num: u8,
    attr: u8,     // CGB tile attribute
    lo: u8,
    hi: u8,
}
impl Fetcher {
    fn new() -> Self {
        Fetcher { step: 0, substep: 0, x: 0, tile_num: 0, attr: 0, lo: 0, hi: 0 }
    }
    fn reset(&mut self) {
        self.step = 0;
        self.substep = 0;
    }
}

const DMG_SHADES: [[u8; 3]; 4] = [
    [0xE0, 0xF8, 0xD0],
    [0x88, 0xC0, 0x70],
    [0x34, 0x68, 0x56],
    [0x08, 0x18, 0x20],
];

const TRANSPARENT_SP: SpPixel = SpPixel { color: 0, palette: 0, behind: false, oam_idx: 0xFF };

impl Ppu {
    pub fn new(cgb: bool) -> Ppu {
        Ppu {
            cgb,
            vram: vec![0; 0x2000 * if cgb { 2 } else { 1 }],
            oam: [0; 0xA0],
            vbk: 0,
            lcdc: 0x91,
            stat: 0x85,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            bcps: 0,
            bg_pal: [0xFF; 64],
            ocps: 0,
            obj_pal: [0xFF; 64],
            opri: 0,
            mode: 2,
            dot_in_line: 0,
            phys_line: 0,
            window_line: 0,
            window_y_triggered: false,
            stat_line: false,
            just_enabled: false,
            lyc_suppress: false,
            line_sprites: Vec::with_capacity(10),
            fetch: Fetcher::new(),
            bg_fifo: BgFifo::new(),
            sp_fifo: [TRANSPARENT_SP; 8],
            x_out: 0,
            to_discard: 0,
            win_active: false,
            win_x: 0,
            window_drawn_line: false,
            first_fetch: true,
            sp_fetch_active: false,
            sp_fetch_idx: 0,
            sp_fetch_step: 0,
            sprite_done: [false; 10],
            reg_hist: [RegSnap { bgp: 0xFC, obp0: 0xFF, obp1: 0xFF, lcdc: 0x91 }; Self::REG_HIST],
            reg_head: 0,
            vblank_irq: false,
            stat_irq: false,
            frame_ready: false,
            fb: vec![0xFF; W * H * 4],
        }
    }

    // Toggle for the DMG spurious-STAT-interrupt write bug (see write_reg).
    const DMG_STAT_WRITE_BUG: bool = true;

    // Pipeline delay (in dots) between a CPU register write and the pixel that
    // observes it. Empirically calibrated against the mealybug-tearoom references.
    const REG_LATCH_DELAY: usize = 5;
    const REG_HIST: usize = 8; // ring length (> REG_LATCH_DELAY)

    fn lcd_on(&self) -> bool {
        self.lcdc & 0x80 != 0
    }

    // ---- memory access ----------------------------------------------------

    pub fn read_vram(&self, addr: u16) -> u8 {
        let off = (addr as usize - 0x8000) + self.vbk * 0x2000;
        self.vram[off]
    }
    pub fn write_vram(&mut self, addr: u16, v: u8) {
        let off = (addr as usize - 0x8000) + self.vbk * 0x2000;
        self.vram[off] = v;
    }
    fn vram_bank(&self, bank: usize, addr: u16) -> u8 {
        self.vram[(addr as usize - 0x8000) + bank * 0x2000]
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        self.oam[addr as usize - 0xFE00]
    }
    pub fn write_oam(&mut self, addr: u16, v: u8) {
        self.oam[addr as usize - 0xFE00] = v;
    }
    /// Direct OAM write used by OAM DMA (bypasses mode restrictions).
    pub fn dma_write_oam(&mut self, idx: usize, v: u8) {
        if idx < 0xA0 {
            self.oam[idx] = v;
        }
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => self.stat | 0x80,
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            0xFF4F => (self.vbk as u8) | 0xFE,
            0xFF68 => self.bcps | 0x40,
            0xFF69 => self.bg_pal[(self.bcps & 0x3F) as usize],
            0xFF6A => self.ocps | 0x40,
            0xFF6B => self.obj_pal[(self.ocps & 0x3F) as usize],
            0xFF6C => self.opri | 0xFE,
            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, v: u8) {
        match addr {
            0xFF40 => {
                let was_on = self.lcd_on();
                self.lcdc = v;
                if was_on && !self.lcd_on() {
                    // Turning the LCD off resets the timing + LY. STAT mode bits
                    // read 0 while the LCD is off; the coincidence flag (bit 2) is
                    // left frozen at whatever it was at the moment of turn-off.
                    self.ly = 0;
                    self.phys_line = 0;
                    self.dot_in_line = 0;
                    self.mode = 0;
                    self.window_line = 0;
                    self.window_y_triggered = false;
                    self.lyc_suppress = false;
                    self.stat &= !0x03; // mode bits -> 0
                    // The internal STAT interrupt line is NOT cleared on disable:
                    // its LYC component stays latched from the frozen coincidence,
                    // so a later enable only edges if the coincidence *changes*
                    // (mooneye stat_lyc_onoff). Re-level it without firing an IRQ.
                    self.prime_stat_line();
                    self.blank_frame();
                } else if !was_on && self.lcd_on() {
                    self.dot_in_line = 0;
                    self.phys_line = 0;
                    self.ly = 0;
                    // The first line after LCD-on reports Mode 0, not Mode 2 (the
                    // LCD-on quirk: line 0 goes Mode0 -> Mode3 with no Mode 2).
                    self.mode = 0;
                    self.just_enabled = true;
                    self.lyc_suppress = false;
                    self.check_lyc();
                    // Re-evaluate the STAT line: an LY==LYC coincidence that becomes
                    // true at enable is a genuine rising edge and requests a STAT IRQ
                    // (mooneye stat_lyc_onoff). If the coincidence was already true
                    // before disable (its frozen value), there is no edge.
                    self.update_stat_line();
                }
            }
            0xFF41 => {
                // bits 0-2 are read-only; keep bit7. Writes to the enable bits
                // can create a fresh rising edge of the STAT line, so re-evaluate.
                if Self::DMG_STAT_WRITE_BUG && self.lcd_on() && !self.cgb {
                    // DMG STAT write bug: for one write, the enable bits behave as
                    // if all set ($FF), so any currently-true condition (mode 0/1/2
                    // or LY==LYC) forces the line high and can fire a spurious IRQ.
                    let saved = self.stat;
                    self.stat |= 0x78;
                    self.update_stat_line();
                    self.stat = (saved & 0x87) | (v & 0x78);
                }
                self.stat = (self.stat & 0x87) | (v & 0x78);
                if self.lcd_on() {
                    self.update_stat_line();
                }
            }
            0xFF42 => self.scy = v,
            0xFF43 => self.scx = v,
            0xFF44 => {} // LY read-only
            0xFF45 => {
                self.lyc = v;
                if self.lcd_on() {
                    self.check_lyc();
                    self.update_stat_line();
                }
            }
            0xFF47 => self.bgp = v,
            0xFF48 => self.obp0 = v,
            0xFF49 => self.obp1 = v,
            0xFF4A => self.wy = v,
            0xFF4B => self.wx = v,
            0xFF4F => {
                if self.cgb {
                    self.vbk = (v & 1) as usize;
                }
            }
            0xFF68 => self.bcps = v & 0xBF,
            0xFF69 => {
                let i = (self.bcps & 0x3F) as usize;
                self.bg_pal[i] = v;
                if self.bcps & 0x80 != 0 {
                    self.bcps = (self.bcps & 0x80) | ((i as u8 + 1) & 0x3F);
                }
            }
            0xFF6A => self.ocps = v & 0xBF,
            0xFF6B => {
                let i = (self.ocps & 0x3F) as usize;
                self.obj_pal[i] = v;
                if self.ocps & 0x80 != 0 {
                    self.ocps = (self.ocps & 0x80) | ((i as u8 + 1) & 0x3F);
                }
            }
            0xFF6C => self.opri = v & 1,
            _ => {}
        }
    }

    fn blank_frame(&mut self) {
        for px in self.fb.chunks_mut(4) {
            px[0] = 0xE0;
            px[1] = 0xF8;
            px[2] = 0xD0;
            px[3] = 0xFF;
        }
        self.frame_ready = true;
    }

    // ---- timing -----------------------------------------------------------

    pub fn tick(&mut self, dots: u32) {
        if !self.lcd_on() {
            return;
        }
        for _ in 0..dots {
            self.dot();
        }
    }

    // The reported LY register wraps to 0 early on the last VBlank line: on DMG
    // LY reads 153 for only ~1 M-cycle at the top of physical line 153, then 0.
    const LY153_EARLY_WRAP_DOT: u32 = 4;

    fn dot(&mut self) {
        // The one-dot LYC suppression (153->0 boundary) only lasts a single dot;
        // release it and re-evaluate the coincidence so the line-0 edge can form.
        if self.lyc_suppress {
            self.lyc_suppress = false;
            self.check_lyc();
        }

        self.dot_in_line += 1;
        if self.dot_in_line >= 456 {
            self.dot_in_line = 0;
            self.advance_line();
        }

        // Apply the LY=153 -> 0 early-wrap quirk: on physical line 153, LY reports
        // 153 for the first few dots, then 0 for the remainder.
        if self.phys_line == 153 && self.dot_in_line == Self::LY153_EARLY_WRAP_DOT && self.ly == 153 {
            self.ly = 0;
            self.check_lyc();
        }

        if self.phys_line < 144 {
            if self.dot_in_line == 80 && self.mode != 3 {
                self.mode = 3;
                self.oam_scan();
                self.start_mode3();
            }
            if self.mode == 3 {
                self.pipeline_dot();
                if self.x_out >= W as i32 {
                    self.mode = 0;
                }
            }
        }
        self.update_stat_line();
    }

    fn advance_line(&mut self) {
        let was_153 = self.phys_line == 153;
        self.phys_line = self.phys_line.wrapping_add(1);
        if self.phys_line > 153 {
            self.phys_line = 0;
            self.window_line = 0;
            self.window_y_triggered = false;
        }
        // 153 -> 0 leaves the reported LY at 0 (it already wrapped mid-line-153);
        // force the coincidence false for this dot so LYC=0 re-edges at line 0.
        self.lyc_suppress = was_153 && self.phys_line == 0;
        self.ly = self.phys_line;
        self.check_lyc();

        if self.phys_line == 144 {
            self.mode = 1;
            self.vblank_irq = true;
            self.frame_ready = true;
        } else if self.phys_line < 144 {
            self.mode = 2;
            // Latch the window Y trigger for this line (stays armed for the frame).
            if self.lcd_on() && self.ly == self.wy {
                self.window_y_triggered = true;
            }
        }
    }

    fn check_lyc(&mut self) {
        if self.ly == self.lyc && !self.lyc_suppress {
            self.stat |= 0x04;
        } else {
            self.stat &= !0x04;
        }
    }

    /// Compute the current ORed STAT interrupt line from mode + LYC + enables.
    fn stat_line_now(&self) -> bool {
        let m = self.mode;
        // The Mode-2 (OAM) STAT source is also asserted at the very start of line
        // 144, even though the PPU mode bits read VBlank (mode 1) there. This is
        // the "OAM interrupt at LY=144" quirk (mooneye vblank_stat_intr).
        let mode2_cond = m == 2 || (self.phys_line == 144 && self.dot_in_line == 0);
        (self.stat & 0x08 != 0 && m == 0)
            || (self.stat & 0x10 != 0 && m == 1)
            || (self.stat & 0x20 != 0 && mode2_cond)
            || (self.stat & 0x40 != 0 && (self.stat & 0x04 != 0))
    }

    fn update_stat_line(&mut self) {
        self.stat = (self.stat & 0xFC) | (self.mode & 0x03);
        let line = self.stat_line_now();
        if line && !self.stat_line {
            self.stat_irq = true;
        }
        self.stat_line = line;
    }

    /// Latch the STAT line to its current level WITHOUT requesting an interrupt.
    /// Used when the LCD is enabled: the line may come up high (e.g. LY==LYC), but
    /// that initial level must not be treated as a fresh rising edge.
    fn prime_stat_line(&mut self) {
        self.stat = (self.stat & 0xFC) | (self.mode & 0x03);
        self.stat_line = self.stat_line_now();
    }

    // ---- OAM scan ---------------------------------------------------------

    fn oam_scan(&mut self) {
        self.line_sprites.clear();
        let height: i32 = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        let ly = self.ly as i32;
        for i in 0..40usize {
            let y = self.oam[i * 4] as i32 - 16;
            if ly >= y && ly < y + height {
                self.line_sprites.push(i as u8);
                if self.line_sprites.len() == 10 {
                    break;
                }
            }
        }
    }

    // ---- Mode-3 pixel pipeline --------------------------------------------

    fn start_mode3(&mut self) {
        self.fetch.reset();
        self.fetch.x = 0;
        self.bg_fifo.clear();
        self.sp_fifo = [TRANSPARENT_SP; 8];
        self.x_out = 0;
        self.to_discard = self.scx & 7;
        self.win_active = false;
        self.win_x = 0;
        self.window_drawn_line = false;
        self.first_fetch = true;
        self.sp_fetch_active = false;
        self.sp_fetch_idx = 0;
        self.sp_fetch_step = 0;
        self.sprite_done = [false; 10];
        // Prime the register history with the values live at Mode-3 start so the
        // first pixels latch the correct (pre-write) values.
        let snap = RegSnap { bgp: self.bgp, obp0: self.obp0, obp1: self.obp1, lcdc: self.lcdc };
        self.reg_hist = [snap; Self::REG_HIST];
        self.reg_head = 0;
    }

    /// Push the current latch-relevant registers into the delay ring (once per dot).
    fn snapshot_regs(&mut self) {
        self.reg_head = (self.reg_head + 1) % Self::REG_HIST;
        self.reg_hist[self.reg_head] = RegSnap {
            bgp: self.bgp,
            obp0: self.obp0,
            obp1: self.obp1,
            lcdc: self.lcdc,
        };
    }

    /// The register snapshot as observed by a pixel produced this dot (REG_LATCH_DELAY
    /// dots after the corresponding CPU write).
    fn delayed_regs(&self) -> RegSnap {
        let d = std::env::var("RD").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(Self::REG_LATCH_DELAY);
        let idx = (self.reg_head + Self::REG_HIST - d) % Self::REG_HIST;
        self.reg_hist[idx]
    }

    /// One dot of the Mode-3 pipeline. Advances the fetcher and (if not paused)
    /// ships one pixel to the LCD.
    fn pipeline_dot(&mut self) {
        self.snapshot_regs();
        // If an object fetch is in progress, it owns this dot. The BG fetcher keeps
        // running during the fetch; no pixel is shipped.
        if self.sp_fetch_active {
            self.sprite_fetch_step();
            self.bg_fetch_step();
            return;
        }

        // Advance the BG/window fetcher first so the FIFO is up to date.
        self.bg_fetch_step();

        // A window trigger resets the fetcher; check before shipping a pixel.
        self.maybe_trigger_window();

        // Nothing to ship until the BG FIFO has a pixel.
        if self.bg_fifo.len == 0 {
            return;
        }

        // Burn off the SCX fine-scroll discard before any sprite or output.
        if self.to_discard > 0 {
            self.bg_fifo.pop();
            self.to_discard -= 1;
            return;
        }

        // If a selected object's left edge is at the current output column, pause
        // BG output and begin its fetch this dot (no pixel shipped).
        self.maybe_start_sprite_fetch();
        if self.sp_fetch_active {
            return;
        }

        // Ship one pixel: pop the BG FIFO and the matching sprite FIFO slot.
        let bg = self.bg_fifo.pop();
        let s = self.sp_fifo[0];
        for i in 0..7 {
            self.sp_fifo[i] = self.sp_fifo[i + 1];
        }
        self.sp_fifo[7] = TRANSPARENT_SP;
        let x = self.x_out as usize;
        self.mix_and_ship(x, bg, s);
        self.x_out += 1;
    }

    /// Advance the background/window fetcher one dot.
    fn bg_fetch_step(&mut self) {
        match self.fetch.step {
            0 => {
                // Get Tile (2 dots): on the 2nd dot, latch tile number + attr.
                if self.fetch.substep == 1 {
                    self.fetch_tile();
                    self.fetch.step = 1;
                    self.fetch.substep = 0;
                } else {
                    self.fetch.substep = 1;
                }
            }
            1 => {
                if self.fetch.substep == 1 {
                    self.fetch_tile_data(false);
                    self.fetch.step = 2;
                    self.fetch.substep = 0;
                } else {
                    self.fetch.substep = 1;
                }
            }
            2 => {
                if self.fetch.substep == 1 {
                    self.fetch_tile_data(true);
                    self.fetch.substep = 0;
                    self.fetch.step = 3;
                } else {
                    self.fetch.substep = 1;
                }
            }
            _ => {
                // Push (retried each dot until BG FIFO is empty).
                if self.bg_fifo.len == 0 {
                    if self.first_fetch {
                        // The first fetch of the line (and of the window) is a
                        // discarded warm-up: re-fetch the same tile. This is the
                        // 12-dot Mode-3 startup overhead (and the +6 window penalty).
                        self.first_fetch = false;
                        self.fetch.step = 0;
                        self.fetch.substep = 0;
                        return;
                    }
                    self.do_push();
                }
            }
        }
    }

    fn do_push(&mut self) {
        self.push_bg_row();
        self.fetch.step = 0;
        self.fetch.substep = 0;
        self.fetch.x = self.fetch.x.wrapping_add(1);
        if self.win_active {
            self.win_x = self.win_x.wrapping_add(1);
        }
    }

    fn fetch_tile(&mut self) {
        let (map_base, tile_x, tile_y) = if self.win_active {
            let map = if self.lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 };
            (map, self.win_x as usize & 0x1F, self.window_line as usize)
        } else {
            let map = if self.lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 };
            let tx = (((self.scx as usize) / 8) + self.fetch.x as usize) & 0x1F;
            let ty = (self.ly as usize + self.scy as usize) & 0xFF;
            (map, tx, ty)
        };
        let map_addr = (map_base + (tile_y / 8) * 32 + tile_x) as u16;
        self.fetch.tile_num = self.read_vram_bank0(map_addr);
        self.fetch.attr = if self.cgb { self.vram_bank(1, map_addr) } else { 0 };
    }

    fn fetch_tile_data(&mut self, high: bool) {
        let attr = self.fetch.attr;
        let yflip = self.cgb && attr & 0x40 != 0;
        let cgb_bank = if self.cgb && attr & 0x08 != 0 { 1 } else { 0 };
        let py = if self.win_active {
            self.window_line as usize
        } else {
            (self.ly as usize + self.scy as usize) & 0xFF
        };
        let mut fine_y = py % 8;
        if yflip {
            fine_y = 7 - fine_y;
        }
        let tile_addr = if self.lcdc & 0x10 != 0 {
            0x8000 + (self.fetch.tile_num as usize) * 16
        } else {
            0x9000usize.wrapping_add((self.fetch.tile_num as i8 as isize * 16) as usize)
        };
        let row_addr = (tile_addr + fine_y * 2) as u16;
        if high {
            self.fetch.hi = self.vram_bank(cgb_bank, row_addr + 1);
        } else {
            self.fetch.lo = self.vram_bank(cgb_bank, row_addr);
        }
    }

    fn push_bg_row(&mut self) {
        let attr = self.fetch.attr;
        let xflip = self.cgb && attr & 0x20 != 0;
        let palette = (attr & 0x07) as u8;
        let prio = self.cgb && attr & 0x80 != 0;
        let lo = self.fetch.lo;
        let hi = self.fetch.hi;
        for i in 0..8 {
            let bit = if xflip { i } else { 7 - i };
            let color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
            self.bg_fifo.push(BgPixel { color, palette, prio });
        }
    }

    // ---- window -----------------------------------------------------------

    fn maybe_trigger_window(&mut self) {
        if self.win_active {
            return;
        }
        if self.lcdc & 0x20 == 0 || !self.window_y_triggered {
            return;
        }
        // Window left column = WX - 7. The output column about to be produced is
        // x_out; the window triggers when x_out reaches WX-7. WX<7 triggers at 0.
        let trigger_x = self.wx as i32 - 7;
        if self.x_out >= trigger_x {
            // Activate the window: reset BG fetcher to the window tilemap. The
            // first window tile fetch is a discarded warm-up (the +6 penalty).
            self.win_active = true;
            self.win_x = 0;
            self.fetch.reset();
            self.fetch.x = 0;
            self.bg_fifo.clear();
            self.first_fetch = true;
            self.window_drawn_line = true;
        }
    }

    // ---- sprites ----------------------------------------------------------

    fn maybe_start_sprite_fetch(&mut self) {
        if self.lcdc & 0x02 == 0 {
            return; // objects globally disabled (DMG + CGB master OBJ enable)
        }
        // Find the first not-yet-fetched sprite whose left edge equals the current
        // output column. Off-screen-left sprites (X<8 -> sx<0) are triggered at
        // column 0.
        let cur_x = self.x_out;
        for idx in 0..self.line_sprites.len() {
            if self.sprite_done[idx] {
                continue;
            }
            let si = self.line_sprites[idx] as usize;
            let sx = self.oam[si * 4 + 1] as i32 - 8;
            if sx == cur_x || (sx < cur_x && cur_x == 0) {
                self.sp_fetch_active = true;
                self.sp_fetch_idx = idx;
                self.sp_fetch_step = 0;
                return;
            }
        }
    }

    fn sprite_fetch_step(&mut self) {
        // Object fetch takes ~6 dots; we model it as 6 sub-steps and merge on the
        // last. (Penalty granularity is sufficient for the timing tests because the
        // BG fetcher keeps stepping during the fetch.)
        self.sp_fetch_step += 1;
        if self.sp_fetch_step >= 6 {
            self.merge_sprite(self.sp_fetch_idx);
            self.sprite_done[self.sp_fetch_idx] = true;
            self.sp_fetch_active = false;
        }
    }

    fn merge_sprite(&mut self, idx: usize) {
        let si = self.line_sprites[idx] as usize;
        let i = si * 4;
        let sy = self.oam[i] as i32 - 16;
        let sx = self.oam[i + 1] as i32 - 8;
        let mut tile = self.oam[i + 2] as usize;
        let attr = self.oam[i + 3];
        let behind = attr & 0x80 != 0;
        let yflip = attr & 0x40 != 0;
        let xflip = attr & 0x20 != 0;
        let cgb_bank = if self.cgb && attr & 0x08 != 0 { 1 } else { 0 };
        let height: i32 = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        let palette = if self.cgb { (attr & 0x07) as u8 } else { (attr >> 4) & 1 };

        let mut row = self.ly as i32 - sy;
        if height == 16 {
            tile &= 0xFE;
            if yflip {
                row = 15 - row;
            }
        } else if yflip {
            row = 7 - row;
        }
        let tile_addr = 0x8000 + tile * 16 + (row as usize) * 2;
        let lo = self.vram_bank(cgb_bank, tile_addr as u16);
        let hi = self.vram_bank(cgb_bank, (tile_addr + 1) as u16);

        for p in 0..8i32 {
            let screen_x = sx + p;
            // FIFO slot 0 corresponds to the current output column (x_out).
            let slot = screen_x - self.x_out;
            if !(0..8).contains(&slot) {
                continue;
            }
            let bit = if xflip { p } else { 7 - p };
            let color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
            if color == 0 {
                continue; // transparent, never overwrites
            }
            let slot = slot as usize;
            let existing = self.sp_fifo[slot];
            let replace = if existing.color == 0 {
                true
            } else if self.cgb && self.opri & 1 == 0 {
                // CGB priority: lower OAM index wins.
                (si as u8) < existing.oam_idx
            } else {
                // DMG priority: lower X wins, tie -> lower OAM index. Sprites are
                // fetched left->right by trigger order (which is X-then-index for
                // the merge), so the *first* non-transparent pixel should remain.
                false
            };
            if replace {
                self.sp_fifo[slot] = SpPixel { color, palette, behind, oam_idx: si as u8 };
            }
        }
    }

    // ---- mixing -----------------------------------------------------------

    fn mix_and_ship(&mut self, x: usize, bg: BgPixel, sp: SpPixel) {
        let y = self.ly as usize;
        // Registers consumed at the output stage are observed with the pipeline
        // delay (mealybug mid-Mode-3 writes).
        let r = self.delayed_regs();
        if self.cgb {
            // Master priority bit (LCDC.0): if 0, objects always over BG.
            let master = r.lcdc & 0x01 != 0;
            let bg_has_priority = master && (bg.prio || sp.behind) && bg.color != 0;
            let obj_visible = r.lcdc & 0x02 != 0;
            if obj_visible && sp.color != 0 && !bg_has_priority {
                let rgb = self.cgb_color(&self.obj_pal, sp.palette as usize, sp.color);
                self.put_pixel(x, y, rgb);
            } else {
                let rgb = self.cgb_color(&self.bg_pal, bg.palette as usize, bg.color);
                self.put_pixel(x, y, rgb);
            }
        } else {
            let dmg_bg_enabled = r.lcdc & 0x01 != 0;
            let bg_color = if dmg_bg_enabled { bg.color } else { 0 };
            let draw_obj = r.lcdc & 0x02 != 0
                && sp.color != 0
                && !(sp.behind && bg_color != 0);
            if draw_obj {
                let palette = if sp.palette != 0 { r.obp1 } else { r.obp0 };
                let shade = (palette >> (sp.color * 2)) & 0x03;
                self.put_pixel(x, y, DMG_SHADES[shade as usize]);
            } else {
                let shade = (r.bgp >> (bg_color * 2)) & 0x03;
                self.put_pixel(x, y, DMG_SHADES[shade as usize]);
            }
        }
        if x + 1 == W {
            // last pixel of the line shipped; advance window line counter once if
            // the window was rendered this line.
            if self.window_drawn_line {
                self.window_line = self.window_line.wrapping_add(1);
            }
        }
    }

    fn read_vram_bank0(&self, addr: u16) -> u8 {
        self.vram[addr as usize - 0x8000]
    }

    fn cgb_color(&self, pal_ram: &[u8; 64], pal: usize, color: u8) -> [u8; 3] {
        let idx = pal * 8 + color as usize * 2;
        let lo = pal_ram[idx] as u16;
        let hi = pal_ram[idx + 1] as u16;
        let rgb555 = lo | (hi << 8);
        let r = (rgb555 & 0x1F) as u8;
        let g = ((rgb555 >> 5) & 0x1F) as u8;
        let b = ((rgb555 >> 10) & 0x1F) as u8;
        // 5-bit -> 8-bit with the common CGB color-correction-free expansion
        [(r << 3) | (r >> 2), (g << 3) | (g >> 2), (b << 3) | (b >> 2)]
    }

    fn put_pixel(&mut self, x: usize, y: usize, rgb: [u8; 3]) {
        let off = (y * W + x) * 4;
        self.fb[off] = rgb[0];
        self.fb[off + 1] = rgb[1];
        self.fb[off + 2] = rgb[2];
        self.fb[off + 3] = 0xFF;
    }

    // ---- debug helpers ----------------------------------------------------

    pub fn vram_raw(&self) -> &[u8] {
        &self.vram
    }
    pub fn oam_raw(&self) -> &[u8] {
        &self.oam
    }
    pub fn bg_palette_raw(&self) -> &[u8] {
        &self.bg_pal
    }
    pub fn obj_palette_raw(&self) -> &[u8] {
        &self.obj_pal
    }
}
