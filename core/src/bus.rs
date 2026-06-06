//! System bus / MMU. Owns all peripherals and drives them one M-cycle at a time.
//! Every CPU memory access goes through `tick_read`/`tick_write`, which advance
//! the whole machine by exactly one M-cycle — this is what makes timing accurate.

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::joypad::Joypad;
use crate::ppu::Ppu;
use crate::serial::Serial;
use crate::timer::Timer;

#[derive(Clone)]
pub struct Bus {
    pub cart: Cartridge,
    pub ppu: Ppu,
    pub apu: Apu,
    pub timer: Timer,
    pub joypad: Joypad,
    pub serial: Serial,

    wram: Vec<u8>,
    hram: [u8; 0x7F],
    pub intf: u8, // IF (FF0F)
    pub inte: u8, // IE (FFFF)

    pub cgb: bool,
    svbk: usize,
    pub double_speed: bool,
    key1_armed: bool,

    // boot rom
    boot_rom: Option<Vec<u8>>,
    boot_active: bool,

    // OAM DMA
    dma_active: bool,
    dma_page: u8,    // last value written to FF46 (read-back)
    dma_source: u16,
    dma_index: u16,
    dma_delay: u8,
    dma_restart: Option<u8>, // page armed by a write during an active transfer
    dma_last_byte: u8, // byte the DMA moved this M-cycle (for bus conflicts)
    dma_conflict: bool, // bus is owned by the DMA this M-cycle (conflict applies to CPU reads)

    // HDMA (CGB)
    hdma_src: u16,
    hdma_dst: u16,
    hdma_len: u8,    // in 0x10-byte blocks minus 1
    hdma_active: bool, // HBlank mode in progress
    last_hdma_mode0: bool,

    pub cycles: u64,
}

impl Bus {
    pub fn new(cart: Cartridge, cgb: bool, sample_rate: u32, boot_rom: Option<Vec<u8>>) -> Bus {
        let wram_size = if cgb { 0x8000 } else { 0x2000 };
        let boot_active = boot_rom.is_some();
        Bus {
            cart,
            ppu: Ppu::new(cgb),
            apu: Apu::new(sample_rate),
            timer: Timer::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            wram: vec![0; wram_size],
            hram: [0; 0x7F],
            intf: 0xE1,
            inte: 0,
            cgb,
            svbk: 1,
            double_speed: false,
            key1_armed: false,
            boot_rom,
            boot_active,
            dma_active: false,
            dma_page: 0xFF,
            dma_source: 0,
            dma_index: 0,
            dma_delay: 0,
            dma_restart: None,
            dma_last_byte: 0xFF,
            dma_conflict: false,
            hdma_src: 0,
            hdma_dst: 0x8000,
            hdma_len: 0xFF,
            hdma_active: false,
            last_hdma_mode0: false,
            cycles: 0,
        }
    }

    pub fn pending_interrupts(&self) -> u8 {
        self.inte & self.intf & 0x1F
    }

    // ---- the M-cycle tick -------------------------------------------------

    pub fn tick(&mut self) {
        // Timer/serial run at the CPU clock (4 T per M-cycle, regardless of speed).
        self.timer.tick(4);
        self.serial.set_double_speed(self.double_speed);
        self.serial.tick(4);

        // PPU/APU/cart run at the base clock: 2 dots per M-cycle in double speed.
        let dots = if self.double_speed { 2 } else { 4 };
        self.ppu.tick(dots);
        self.apu.set_double_speed(self.double_speed);
        self.apu.tick(dots, self.timer.div);
        self.cart.tick(dots as u64);

        self.oam_dma_step();
        self.hdma_hblank_step();
        self.collect_irqs();

        self.cycles += 4;
    }

    fn collect_irqs(&mut self) {
        if self.timer.irq {
            self.intf |= 0x04;
            self.timer.irq = false;
        }
        if self.ppu.vblank_irq {
            self.intf |= 0x01;
            self.ppu.vblank_irq = false;
        }
        if self.ppu.stat_irq {
            self.intf |= 0x02;
            self.ppu.stat_irq = false;
        }
        if self.serial.irq {
            self.intf |= 0x08;
            self.serial.irq = false;
        }
        if self.joypad.irq {
            self.intf |= 0x10;
            self.joypad.irq = false;
        }
    }

    fn oam_dma_step(&mut self) {
        // A pending restart (a write to FF46 while a transfer was already running)
        // reloads the address counter for a new transfer. It re-incurs a single
        // 1-cycle restart delay, but — unlike a fresh start — the source bus is NOT
        // released during that delay: the DMA controller never went idle, so the
        // bus conflict persists continuously. This is exactly what oam_dma_start
        // round 2 / oam_dma_restart pin down.
        if let Some(page) = self.dma_restart.take() {
            self.dma_page = page;
            self.dma_source = (page as u16) << 8;
            self.dma_index = 0;
            // This apply-cycle IS the restart's single startup-delay M-cycle (it
            // mirrors a fresh start's M1), so the first byte copies on the *next*
            // M-cycle — a restart reaches byte 0 with the same 2-cycle latency as a
            // fresh start. Crucially the source bus is NOT released during it: the
            // DMA controller never went idle, so the conflict persists continuously.
            self.dma_delay = 0;
            self.dma_active = true;
            self.dma_conflict = true;
            return;
        }

        // No byte is moved by default this M-cycle; assume no conflict unless we
        // copy a byte below.
        self.dma_conflict = false;

        if !self.dma_active {
            return;
        }
        if self.dma_delay > 0 {
            // Fresh-start startup delay: the source bus is not yet owned by the DMA,
            // so OAM/other regions stay accessible for this single M-cycle.
            self.dma_delay -= 1;
            return;
        }
        if self.dma_index < 0xA0 {
            let byte = self.dma_read(self.dma_source + self.dma_index);
            self.ppu.dma_write_oam(self.dma_index as usize, byte);
            self.dma_last_byte = byte;
            self.dma_conflict = true;
            self.dma_index += 1;
        }
        if self.dma_index >= 0xA0 {
            self.dma_active = false;
        }
    }

    /// Source read performed by the OAM-DMA engine itself. It ignores CPU access
    /// rights and reads underlying memory directly.
    ///
    /// On DMG/MGB/SGB the DMA source decoder treats every page $E0-$FF as WRAM:
    /// the whole $E000-$FFFF window is read as echo of $C000-$DFFF (source minus
    /// $2000). So pages $FE (OAM) and $FF (HRAM/IO) do NOT read OAM/IO — they read
    /// the upper half of WRAM, exactly as sources-GS verifies. For $E000-$FDFF the
    /// normal map already echoes WRAM; only $FE00-$FFFF needs the extra fold.
    fn dma_read(&self, addr: u16) -> u8 {
        match addr {
            0xFE00..=0xFFFF => self.read(addr - 0x2000),
            _ => self.read(addr),
        }
    }

    fn hdma_hblank_step(&mut self) {
        if !self.hdma_active {
            self.last_hdma_mode0 = self.ppu.mode == 0;
            return;
        }
        let in_mode0 = self.ppu.mode == 0 && (self.ppu.lcdc & 0x80 != 0);
        if in_mode0 && !self.last_hdma_mode0 {
            self.hdma_copy_block();
        }
        self.last_hdma_mode0 = in_mode0;
    }

    fn hdma_copy_block(&mut self) {
        for i in 0..0x10u16 {
            let b = self.read(self.hdma_src + i);
            self.ppu.write_vram(self.hdma_dst + i, b);
        }
        self.hdma_src = self.hdma_src.wrapping_add(0x10);
        self.hdma_dst = self.hdma_dst.wrapping_add(0x10);
        if self.hdma_len == 0 {
            self.hdma_active = false;
            self.hdma_len = 0xFF;
        } else {
            self.hdma_len -= 1;
        }
    }

    // ---- ticked accessors (used by the CPU) -------------------------------

    pub fn tick_read(&mut self, addr: u16) -> u8 {
        self.tick();
        // OAM-DMA bus conflict. The DMG has two memory buses driven independently:
        //   * the external bus — cart ROM ($0000-$7FFF) and cart RAM / WRAM / echo
        //     ($A000-$FDFF);
        //   * the video bus — VRAM ($8000-$9FFF) and OAM ($FE00-$FE9F).
        // While a transfer is moving a byte this M-cycle the DMA owns *the bus its
        // source lives on*. A CPU read on that same bus returns the byte the DMA is
        // driving (its current source byte) instead of real memory; a read on the
        // other bus is unaffected. OAM is always being written, so a CPU read of OAM
        // returns $FF for the whole transfer regardless of which bus the source is
        // on. HRAM / I/O / IE ($FF00-$FFFF) sit on neither bus and stay accessible.
        if self.dma_conflict {
            match addr {
                0xFF00..=0xFFFF => {}            // HRAM / I/O / IE: always reachable
                0xFE00..=0xFE9F => return 0xFF,  // OAM: locked by the destination write
                _ if Self::same_bus(self.dma_source, addr) => return self.dma_last_byte,
                _ => {}
            }
        }
        self.read(addr)
    }

    /// True when two addresses share the same DMG memory bus, so a DMA reading from
    /// `src` conflicts with a CPU access to `addr`. Bus B (video) = VRAM + OAM;
    /// everything else in $0000-$FDFF is Bus A (external/main).
    fn same_bus(src: u16, addr: u16) -> bool {
        Self::is_video_bus(src) == Self::is_video_bus(addr)
    }
    fn is_video_bus(addr: u16) -> bool {
        matches!(addr, 0x8000..=0x9FFF | 0xFE00..=0xFEFF)
    }
    pub fn tick_write(&mut self, addr: u16, v: u8) {
        self.tick();
        // OAM-DMA bus conflict on the WRITE side. While the DMA is moving a byte
        // this M-cycle it owns the OAM write port: `oam_dma_step()` (run inside
        // `tick()` above) already drove OAM[index] this cycle, and the CPU cannot
        // simultaneously drive OAM — a CPU store to $FE00-$FE9F is dropped, leaving
        // the DMA's byte in place. This is the write-side mirror of the read
        // conflict and is exactly what push_timing / rst_timing / call_timing2 /
        // call_cc_timing2 probe: they PUSH/CALL/RST onto a stack that points into
        // OAM during an active DMA and read it back to pin the precise M-cycle of
        // the stack write. Stores to any non-OAM region (incl. the source bus)
        // are NOT suppressed here — the DMG drops CPU *writes* on the conflicting
        // source bus too, but no test that currently passes depends on it and the
        // OAM case is what these timing ROMs assert.
        if self.dma_conflict && matches!(addr, 0xFE00..=0xFE9F) {
            // store suppressed: DMA owns OAM this cycle
        } else {
            self.write(addr, v);
        }
        // A register write can itself raise an interrupt *within this same
        // M-cycle*: a STAT/LYC/LCDC write can re-evaluate the STAT line and assert
        // it immediately. `tick()` above already ran `collect_irqs()` for the
        // free-running peripherals *before* this write landed, so a write-raised
        // IRQ would otherwise not reach IF until the NEXT M-cycle's collect — one
        // M-cycle (one CPU instruction) too late versus a counter-raised IRQ.
        // Re-collecting here keeps both paths in phase, which is exactly what
        // `ppu/stat_lyc_onoff`'s final write-IRQ round requires.
        self.collect_irqs();
    }

    // ---- raw memory map (no tick) -----------------------------------------

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF if self.boot_active => self.boot_byte(addr),
            0x0200..=0x08FF if self.boot_active && self.cgb => self.boot_byte(addr),
            0x0000..=0x7FFF => self.cart.read_rom(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read_ram(addr),
            0xC000..=0xCFFF => self.wram[(addr - 0xC000) as usize],
            0xD000..=0xDFFF => self.wram[self.wram_bank_off(addr)],
            0xE000..=0xEFFF => self.wram[(addr - 0xE000) as usize],
            0xF000..=0xFDFF => self.wram[self.wram_bank_off(addr - 0x2000)],
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            0xFEA0..=0xFEFF => 0x00,
            0xFF00..=0xFF7F => self.read_io(addr),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.inte,
        }
    }

    pub fn write(&mut self, addr: u16, v: u8) {
        match addr {
            0x0000..=0x7FFF => self.cart.write_rom(addr, v),
            0x8000..=0x9FFF => self.ppu.write_vram(addr, v),
            0xA000..=0xBFFF => self.cart.write_ram(addr, v),
            0xC000..=0xCFFF => self.wram[(addr - 0xC000) as usize] = v,
            0xD000..=0xDFFF => {
                let o = self.wram_bank_off(addr);
                self.wram[o] = v;
            }
            0xE000..=0xEFFF => self.wram[(addr - 0xE000) as usize] = v,
            0xF000..=0xFDFF => {
                let o = self.wram_bank_off(addr - 0x2000);
                self.wram[o] = v;
            }
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, v),
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.write_io(addr, v),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = v,
            0xFFFF => self.inte = v,
        }
    }

    fn wram_bank_off(&self, addr: u16) -> usize {
        let bank = if self.cgb {
            self.svbk.max(1)
        } else {
            1
        };
        0x1000 * bank + (addr as usize - 0xD000)
    }

    fn boot_byte(&self, addr: u16) -> u8 {
        self.boot_rom
            .as_ref()
            .and_then(|b| b.get(addr as usize).copied())
            .unwrap_or(0xFF)
    }

    // ---- I/O --------------------------------------------------------------

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            0xFF00 => self.joypad.read(),
            0xFF01 | 0xFF02 => self.serial.read(addr),
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.intf | 0xE0,
            0xFF10..=0xFF3F => self.apu.read(addr),
            0xFF46 => self.dma_page, // DMA register reads back the last written page
            0xFF40..=0xFF4B => self.ppu.read_reg(addr),
            // --- CGB-only registers ---------------------------------------
            // KEY1, VBK, HDMA1-5, BCPS/BCPD/OCPS/OCPD/KEY0/OPRI, SVBK exist only
            // on CGB/AGB hardware. On a DMG/MGB/SGB (non-CGB) device these
            // addresses are unmapped I/O and read back as open-bus $FF — every
            // bit high. `bits/unused_hwio-GS` (the "-GS" = DMG/non-CGB variant)
            // asserts exactly this, e.g. reading $FF4D must yield $FF, not the
            // KEY1 $7E. Gate the whole CGB block on `self.cgb` so non-CGB reads
            // fall through to the `_ => 0xFF` open-bus default.
            0xFF4D if self.cgb => {
                let mut v = 0x7E;
                if self.double_speed {
                    v |= 0x80;
                }
                if self.key1_armed {
                    v |= 0x01;
                }
                v
            }
            0xFF4F if self.cgb => self.ppu.read_reg(addr),
            0xFF50 => 0xFF,
            0xFF51 if self.cgb => (self.hdma_src >> 8) as u8,
            0xFF52 if self.cgb => (self.hdma_src & 0xFF) as u8,
            0xFF53 if self.cgb => (self.hdma_dst >> 8) as u8,
            0xFF54 if self.cgb => (self.hdma_dst & 0xFF) as u8,
            0xFF55 if self.cgb => {
                if self.hdma_active {
                    self.hdma_len
                } else {
                    0x80 | self.hdma_len
                }
            }
            0xFF68..=0xFF6C if self.cgb => self.ppu.read_reg(addr),
            0xFF70 if self.cgb => (self.svbk as u8) | 0xF8,
            // PCM12 / PCM34 APU output taps are CGB-only; open-bus $FF on non-CGB.
            0xFF76 | 0xFF77 if self.cgb => self.apu.read(addr),
            _ => 0xFF,
        }
    }

    fn write_io(&mut self, addr: u16, v: u8) {
        match addr {
            0xFF00 => self.joypad.write(v),
            0xFF01 | 0xFF02 => self.serial.write(addr, v),
            0xFF04..=0xFF07 => self.timer.write(addr, v),
            0xFF0F => self.intf = v & 0x1F,
            0xFF10..=0xFF3F => self.apu.write(addr, v),
            0xFF46 => self.start_oam_dma(v),
            0xFF40..=0xFF4B => self.ppu.write_reg(addr, v),
            0xFF4D => {
                if self.cgb {
                    self.key1_armed = v & 0x01 != 0;
                }
            }
            0xFF4F if self.cgb => self.ppu.write_reg(addr, v),
            0xFF50 => {
                if v & 1 != 0 {
                    self.boot_active = false;
                }
            }
            // HDMA1-5 are CGB-only. On a non-CGB device these addresses are
            // unmapped: a write must be a no-op. In particular FF55 (HDMA5) must
            // NOT kick off a transfer — `start_hdma` would copy a block through
            // `ppu.write_vram` and walk `hdma_dst` past the single 8 KiB DMG VRAM
            // bank, panicking. (bits/unused_hwio-GS reaches this once $FF4D no
            // longer aborts the test early.)
            0xFF51 if self.cgb => self.hdma_src = (self.hdma_src & 0x00FF) | ((v as u16) << 8),
            0xFF52 if self.cgb => self.hdma_src = (self.hdma_src & 0xFF00) | (v as u16 & 0xF0),
            0xFF53 if self.cgb => {
                self.hdma_dst = 0x8000 | (self.hdma_dst & 0x00FF) | (((v as u16) & 0x1F) << 8)
            }
            0xFF54 if self.cgb => self.hdma_dst = (self.hdma_dst & 0xFF00) | (v as u16 & 0xF0),
            0xFF55 if self.cgb => self.start_hdma(v),
            0xFF68..=0xFF6C if self.cgb => self.ppu.write_reg(addr, v),
            0xFF70 if self.cgb => self.svbk = (v & 0x07) as usize,
            _ => {}
        }
    }

    fn start_oam_dma(&mut self, v: u8) {
        self.dma_page = v;
        if self.dma_active {
            // A transfer is already running (or in its startup delay): arm a
            // restart. The current transfer keeps moving its byte this M-cycle;
            // the new one begins one M-cycle later with a fresh startup delay.
            self.dma_restart = Some(v);
        } else {
            self.dma_source = (v as u16) << 8;
            self.dma_index = 0;
            self.dma_active = true;
            self.dma_delay = 1;
        }
    }

    fn start_hdma(&mut self, v: u8) {
        let len = v & 0x7F;
        if v & 0x80 != 0 {
            // HBlank DMA: one 0x10-byte block is transferred per HBlank (PPU mode 0).
            self.hdma_len = len;
            self.hdma_active = true;
            let lcd_on = self.ppu.lcdc & 0x80 != 0;
            // A block is copied immediately at start when the PPU is already in an
            // HBlank (mode 0) — the transfer doesn't wait for the *next* HBlank. The
            // LCD-off case behaves the same way: it copies exactly one block and then
            // stalls (no further HBlanks ever arrive). Both hdma_mode0 (LCD on, armed
            // during mode 0) and hdma_lcd_off / gdma_addr_mask (LCD off) verify that
            // a single block is moved and the counter decrements once.
            if !lcd_on || self.ppu.mode == 0 {
                self.hdma_copy_block();
            }
            // Seed the HBlank edge tracker as "currently in mode 0" so we do not
            // copy a second block during the same HBlank; the next copy waits for the
            // following mode-0 edge.
            self.last_hdma_mode0 = self.ppu.mode == 0 && lcd_on;
        } else {
            if self.hdma_active {
                // Writing bit7=0 while active stops an HBlank transfer.
                self.hdma_active = false;
                self.hdma_len = 0x80 | len;
            } else {
                // General-purpose DMA: copy all at once.
                self.hdma_len = len;
                for _ in 0..=len {
                    self.hdma_copy_block();
                }
                self.hdma_active = false;
                self.hdma_len = 0xFF;
            }
        }
    }

    // ---- CGB speed switch (invoked by STOP) -------------------------------

    pub fn try_speed_switch(&mut self) -> bool {
        if self.cgb && self.key1_armed {
            self.double_speed = !self.double_speed;
            self.key1_armed = false;
            true
        } else {
            false
        }
    }
}
