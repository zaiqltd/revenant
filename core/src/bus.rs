//! System bus / MMU. Owns all peripherals and drives them one M-cycle at a time.
//! Every CPU memory access goes through `tick_read`/`tick_write`, which advance
//! the whole machine by exactly one M-cycle — this is what makes timing accurate.

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::joypad::Joypad;
use crate::ppu::Ppu;
use crate::serial::Serial;
use crate::timer::Timer;

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
    dma_source: u16,
    dma_index: u16,
    dma_delay: u8,

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
            dma_source: 0,
            dma_index: 0,
            dma_delay: 0,
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
        if !self.dma_active {
            return;
        }
        if self.dma_delay > 0 {
            self.dma_delay -= 1;
            return;
        }
        if self.dma_index < 0xA0 {
            let byte = self.read(self.dma_source + self.dma_index);
            self.ppu.dma_write_oam(self.dma_index as usize, byte);
            self.dma_index += 1;
        }
        if self.dma_index >= 0xA0 {
            self.dma_active = false;
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
        self.read(addr)
    }
    pub fn tick_write(&mut self, addr: u16, v: u8) {
        self.tick();
        self.write(addr, v);
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
            0xFF40..=0xFF4B => self.ppu.read_reg(addr),
            0xFF4D => {
                let mut v = 0x7E;
                if self.double_speed {
                    v |= 0x80;
                }
                if self.key1_armed {
                    v |= 0x01;
                }
                v
            }
            0xFF4F => self.ppu.read_reg(addr),
            0xFF50 => 0xFF,
            0xFF51 => (self.hdma_src >> 8) as u8,
            0xFF52 => (self.hdma_src & 0xFF) as u8,
            0xFF53 => (self.hdma_dst >> 8) as u8,
            0xFF54 => (self.hdma_dst & 0xFF) as u8,
            0xFF55 => {
                if self.hdma_active {
                    self.hdma_len
                } else {
                    0x80 | self.hdma_len
                }
            }
            0xFF68..=0xFF6C => self.ppu.read_reg(addr),
            0xFF70 => {
                if self.cgb {
                    (self.svbk as u8) | 0xF8
                } else {
                    0xFF
                }
            }
            0xFF76 | 0xFF77 => self.apu.read(addr),
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
            0xFF4F => self.ppu.write_reg(addr, v),
            0xFF50 => {
                if v & 1 != 0 {
                    self.boot_active = false;
                }
            }
            0xFF51 => self.hdma_src = (self.hdma_src & 0x00FF) | ((v as u16) << 8),
            0xFF52 => self.hdma_src = (self.hdma_src & 0xFF00) | (v as u16 & 0xF0),
            0xFF53 => self.hdma_dst = 0x8000 | (self.hdma_dst & 0x00FF) | (((v as u16) & 0x1F) << 8),
            0xFF54 => self.hdma_dst = (self.hdma_dst & 0xFF00) | (v as u16 & 0xF0),
            0xFF55 => self.start_hdma(v),
            0xFF68..=0xFF6C => self.ppu.write_reg(addr, v),
            0xFF70 => {
                if self.cgb {
                    self.svbk = (v & 0x07) as usize;
                }
            }
            _ => {}
        }
    }

    fn start_oam_dma(&mut self, v: u8) {
        self.dma_source = (v as u16) << 8;
        self.dma_index = 0;
        self.dma_active = true;
        self.dma_delay = 1;
    }

    fn start_hdma(&mut self, v: u8) {
        let len = v & 0x7F;
        if v & 0x80 != 0 {
            // HBlank DMA
            self.hdma_len = len;
            self.hdma_active = true;
            self.last_hdma_mode0 = self.ppu.mode == 0;
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
