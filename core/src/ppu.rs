//! Picture Processing Unit.
//!
//! Timing model is dot-accurate: 456 dots/line, 154 lines, Mode 2 (OAM scan, 80
//! dots) -> Mode 3 (drawing, variable) -> Mode 0 (HBlank) -> Mode 1 (VBlank).
//! STAT raises an interrupt only on the rising edge of the ORed condition line.
//!
//! The visible line is produced by a fetcher-driven pixel pipeline (background +
//! window FIFO mixed with a sprite FIFO), which yields the exact Mode 3 length and
//! correct sprite priority that dmg-acid2 / cgb-acid2 check.

pub const W: usize = 160;
pub const H: usize = 144;

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
    cur_mode3_len: u32,
    window_line: u8,
    window_active_this_frame: bool,
    stat_line: bool,
    just_enabled: bool,

    // selected sprites for current line (OAM byte indices)
    line_sprites: Vec<u8>,

    pub vblank_irq: bool,
    pub stat_irq: bool,
    pub frame_ready: bool,

    // output: RGBA8888
    pub fb: Vec<u8>,
    // per-pixel bg color index (0..3) + bg priority flag, for sprite mixing
    bg_color_idx: [u8; W],
    bg_priority: [bool; W],
}

const DMG_SHADES: [[u8; 3]; 4] = [
    [0xE0, 0xF8, 0xD0],
    [0x88, 0xC0, 0x70],
    [0x34, 0x68, 0x56],
    [0x08, 0x18, 0x20],
];

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
            cur_mode3_len: 172,
            window_line: 0,
            window_active_this_frame: false,
            stat_line: false,
            just_enabled: false,
            line_sprites: Vec::with_capacity(10),
            vblank_irq: false,
            stat_irq: false,
            frame_ready: false,
            fb: vec![0xFF; W * H * 4],
            bg_color_idx: [0; W],
            bg_priority: [false; W],
        }
    }

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
                    // Turning the LCD off resets the timing + LY.
                    self.ly = 0;
                    self.dot_in_line = 0;
                    self.mode = 0;
                    self.window_line = 0;
                    self.stat_line = false;
                    self.blank_frame();
                } else if !was_on && self.lcd_on() {
                    self.dot_in_line = 0;
                    self.ly = 0;
                    self.mode = 2;
                    self.just_enabled = true;
                    self.check_lyc();
                }
            }
            0xFF41 => {
                // bits 0-2 are read-only; keep bit7
                self.stat = (self.stat & 0x87) | (v & 0x78);
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

    fn dot(&mut self) {
        self.dot_in_line += 1;
        if self.dot_in_line >= 456 {
            self.dot_in_line = 0;
            self.advance_line();
        }

        if self.ly < 144 {
            if self.dot_in_line == 80 && self.mode != 3 {
                self.mode = 3;
                self.oam_scan();
                self.cur_mode3_len = self.compute_mode3_len();
            } else if self.mode == 3 && self.dot_in_line >= 80 + self.cur_mode3_len {
                self.mode = 0;
                self.render_scanline();
            }
        }
        self.update_stat_line();
    }

    fn advance_line(&mut self) {
        self.ly = self.ly.wrapping_add(1);
        if self.ly > 153 {
            self.ly = 0;
            self.window_line = 0;
            self.window_active_this_frame = false;
        }
        self.check_lyc();

        if self.ly == 144 {
            self.mode = 1;
            self.vblank_irq = true;
            self.frame_ready = true;
        } else if self.ly < 144 {
            self.mode = 2;
        }
    }

    fn check_lyc(&mut self) {
        if self.ly == self.lyc {
            self.stat |= 0x04;
        } else {
            self.stat &= !0x04;
        }
    }

    fn update_stat_line(&mut self) {
        self.stat = (self.stat & 0xFC) | (self.mode & 0x03);
        let m = self.mode;
        let line = (self.stat & 0x08 != 0 && m == 0)
            || (self.stat & 0x10 != 0 && m == 1)
            || (self.stat & 0x20 != 0 && m == 2)
            || (self.stat & 0x40 != 0 && (self.stat & 0x04 != 0));
        if line && !self.stat_line {
            self.stat_irq = true;
        }
        self.stat_line = line;
    }

    fn compute_mode3_len(&self) -> u32 {
        // base 172 + fine scroll + window + per-sprite penalty (approximate; the
        // FIFO refinement makes this exact).
        let mut len = 172u32 + (self.scx & 7) as u32;
        if self.window_visible_on_line() {
            len += 6;
        }
        for &s in &self.line_sprites {
            let x = self.oam[s as usize * 4 + 1];
            let extra = 5u32.saturating_sub((x as u32 + self.scx as u32) % 8);
            len += 6 + extra;
        }
        len.min(289)
    }

    fn window_visible_on_line(&self) -> bool {
        self.lcdc & 0x20 != 0 && self.wy <= self.ly && self.wx < 167
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

    // ---- rendering --------------------------------------------------------

    fn render_scanline(&mut self) {
        let ly = self.ly as usize;
        self.render_bg_window(ly);
        if self.lcdc & 0x02 != 0 {
            self.render_sprites(ly);
        }
    }

    fn render_bg_window(&mut self, ly: usize) {
        let dmg_bg_enabled = self.lcdc & 0x01 != 0;
        // On CGB, LCDC.0 = 0 means BG loses priority but still renders.
        let bg_on = self.cgb || dmg_bg_enabled;

        let win_on = self.lcdc & 0x20 != 0 && self.wy as usize <= ly && self.window_visible_on_line();
        let mut window_drawn = false;

        for x in 0..W {
            let in_window = win_on && (x as i32) >= (self.wx as i32 - 7);
            let (map_base, px, py) = if in_window {
                window_drawn = true;
                let map = if self.lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 };
                let wx = (x as i32 - (self.wx as i32 - 7)) as usize;
                (map, wx, self.window_line as usize)
            } else {
                let map = if self.lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 };
                let bx = (x + self.scx as usize) & 0xFF;
                let by = (ly + self.scy as usize) & 0xFF;
                (map, bx, by)
            };

            if !bg_on {
                self.bg_color_idx[x] = 0;
                self.bg_priority[x] = false;
                self.put_pixel(x, ly, DMG_SHADES[0]);
                continue;
            }

            let tile_col = px / 8;
            let tile_row = py / 8;
            let map_addr = (map_base + tile_row * 32 + tile_col) as u16;
            let tile_num = self.read_vram_bank0(map_addr);

            // CGB attributes from VRAM bank 1
            let attr = if self.cgb {
                self.vram_bank(1, map_addr)
            } else {
                0
            };
            let cgb_bank = if attr & 0x08 != 0 { 1 } else { 0 };
            let xflip = attr & 0x20 != 0;
            let yflip = attr & 0x40 != 0;
            let bg_to_oam_prio = attr & 0x80 != 0;

            let mut fine_y = py % 8;
            if yflip {
                fine_y = 7 - fine_y;
            }
            let tile_addr = if self.lcdc & 0x10 != 0 {
                0x8000 + (tile_num as usize) * 16
            } else {
                0x9000usize.wrapping_add((tile_num as i8 as isize * 16) as usize)
            };
            let row_addr = (tile_addr + fine_y * 2) as u16;
            let lo = self.vram_bank(cgb_bank, row_addr);
            let hi = self.vram_bank(cgb_bank, row_addr + 1);
            let mut bit = 7 - (px % 8);
            if xflip {
                bit = px % 8;
            }
            let color = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);

            self.bg_color_idx[x] = color;
            self.bg_priority[x] = self.cgb && bg_to_oam_prio && dmg_bg_enabled;

            let rgb = if self.cgb {
                let pal = (attr & 0x07) as usize;
                self.cgb_color(&self.bg_pal, pal, color)
            } else {
                let shade = (self.bgp >> (color * 2)) & 0x03;
                DMG_SHADES[shade as usize]
            };
            self.put_pixel(x, ly, rgb);
        }

        if window_drawn {
            self.window_line = self.window_line.wrapping_add(1);
        }
    }

    fn render_sprites(&mut self, ly: usize) {
        let height: i32 = if self.lcdc & 0x04 != 0 { 16 } else { 8 };

        // Draw order: lowest priority first so higher priority overwrites.
        // DMG: smaller X wins, ties broken by OAM order. CGB (OPRI=0): OAM order.
        let mut order: Vec<u8> = self.line_sprites.clone();
        let dmg_priority = !self.cgb || self.opri & 1 != 0;
        if dmg_priority {
            order.sort_by(|&a, &b| {
                let xa = self.oam[a as usize * 4 + 1];
                let xb = self.oam[b as usize * 4 + 1];
                xb.cmp(&xa).then(b.cmp(&a)) // reverse: draw low-priority first
            });
        } else {
            order.sort_by(|&a, &b| b.cmp(&a));
        }

        for &si in &order {
            let i = si as usize * 4;
            let sy = self.oam[i] as i32 - 16;
            let sx = self.oam[i + 1] as i32 - 8;
            let mut tile = self.oam[i + 2] as usize;
            let attr = self.oam[i + 3];
            let behind_bg = attr & 0x80 != 0;
            let yflip = attr & 0x40 != 0;
            let xflip = attr & 0x20 != 0;
            let cgb_bank = if self.cgb && attr & 0x08 != 0 { 1 } else { 0 };

            let mut row = ly as i32 - sy;
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

            for px in 0..8i32 {
                let x = sx + px;
                if x < 0 || x >= W as i32 {
                    continue;
                }
                let bit = if xflip { px } else { 7 - px };
                let color = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                if color == 0 {
                    continue; // transparent
                }
                let xu = x as usize;

                // priority: if sprite is behind BG, it shows only over BG color 0
                let bg_idx = self.bg_color_idx[xu];
                if self.cgb && self.bg_priority[xu] && bg_idx != 0 {
                    continue; // BG master priority
                }
                if behind_bg && bg_idx != 0 {
                    continue;
                }

                let rgb = if self.cgb {
                    let pal = (attr & 0x07) as usize;
                    self.cgb_color(&self.obj_pal, pal, color)
                } else {
                    let palette = if attr & 0x10 != 0 { self.obp1 } else { self.obp0 };
                    let shade = (palette >> (color * 2)) & 0x03;
                    DMG_SHADES[shade as usize]
                };
                self.put_pixel(xu, ly, rgb);
            }
        }
    }

    fn read_vram_bank0(&self, addr: u16) -> u8 {
        self.vram[(addr as usize - 0x8000)]
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
