//! REVENANT — a cycle-accurate Game Boy / Game Boy Color emulator core.
//!
//! Public surface is deliberately small and deterministic: feed it a ROM and
//! inputs, call `run_frame`, read back a framebuffer + audio. Identical inputs
//! produce byte-identical frames, which is what makes rewind and netplay work.

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod disasm;
pub mod joypad;
pub mod ppu;
pub mod serial;
pub mod state;
pub mod timer;

use bus::Bus;
use cartridge::Cartridge;
use cpu::Cpu;
use joypad::Button;

pub const SCREEN_W: usize = ppu::W;
pub const SCREEN_H: usize = ppu::H;
pub const CYCLES_PER_FRAME: u64 = 70224;

pub struct GameBoy {
    pub cpu: Cpu,
    pub bus: Bus,
    pub cgb: bool,
    rom: Vec<u8>,
    boot_rom: Option<Vec<u8>>,
    sample_rate: u32,
    // ---- rewind (Boss I) ----
    // A ring of per-frame machine snapshots. This is *only* possible because the
    // core is byte-deterministic: a restored state replays frame-perfect. The ROM
    // is shared via Rc, so each snapshot copies only the mutable state (~100-150 KB),
    // not the cartridge ROM.
    history: std::collections::VecDeque<(Cpu, Bus)>,
    recording: bool,
}

/// ~10 seconds of rewind at 59.7275 Hz.
const REWIND_CAP: usize = 600;

impl GameBoy {
    pub fn new(rom: Vec<u8>, boot_rom: Option<Vec<u8>>, sample_rate: u32) -> GameBoy {
        let header_checksum = rom.get(0x014D).copied().unwrap_or(0);
        let cart = Cartridge::new(rom.clone());
        let cgb = cart.header.cgb_flag != cartridge::CgbFlag::None;
        let has_boot = boot_rom.is_some();
        let mut bus = Bus::new(cart, cgb, sample_rate, boot_rom.clone());
        let mut cpu = Cpu::new();
        if !has_boot {
            cpu.set_post_boot(cgb, header_checksum);
            // Approximate post-boot DIV (the value the boot ROM leaves behind).
            bus.timer.set_div_counter(if cgb { 0x1EA0 } else { 0xABCC });
        }
        GameBoy {
            cpu,
            bus,
            cgb,
            rom,
            boot_rom,
            sample_rate,
            history: std::collections::VecDeque::new(),
            recording: false,
        }
    }

    // ---- rewind --------------------------------------------------------------

    /// Turn the per-frame snapshot ring on/off. Disabling clears the buffer.
    pub fn set_recording(&mut self, on: bool) {
        self.recording = on;
        if !on {
            self.history.clear();
        }
    }

    pub fn rewind_len(&self) -> usize {
        self.history.len()
    }

    fn capture(&mut self) {
        if self.history.len() >= REWIND_CAP {
            self.history.pop_front();
        }
        self.history.push_back((self.cpu.clone(), self.bus.clone()));
    }

    /// Restore the machine to the state one frame earlier (byte-exact). Returns
    /// false if the rewind buffer is empty.
    pub fn rewind_frame(&mut self) -> bool {
        match self.history.pop_back() {
            Some((c, b)) => {
                self.cpu = c;
                self.bus = b;
                true
            }
            None => false,
        }
    }

    pub fn reset(&mut self) {
        let battery = if self.bus.cart.has_battery() {
            Some(self.bus.cart.ram_snapshot())
        } else {
            None
        };
        let mut fresh = GameBoy::new(self.rom.clone(), self.boot_rom.clone(), self.sample_rate);
        if let Some(b) = battery {
            fresh.bus.cart.load_ram(&b);
        }
        self.cpu = fresh.cpu;
        self.bus = fresh.bus;
        self.history.clear();
    }

    /// Run exactly one frame (until the PPU finishes a frame, or a frame's worth
    /// of cycles elapses when the LCD is off).
    pub fn run_frame(&mut self) {
        if self.recording {
            self.capture(); // snapshot the state at the start of this frame
        }
        let start = self.bus.cycles;
        let cap = if self.bus.double_speed {
            CYCLES_PER_FRAME * 2
        } else {
            CYCLES_PER_FRAME
        };
        self.bus.ppu.frame_ready = false;
        loop {
            self.cpu.step(&mut self.bus);
            if self.bus.ppu.frame_ready {
                self.bus.ppu.frame_ready = false;
                break;
            }
            if self.bus.cycles.wrapping_sub(start) >= cap {
                break;
            }
        }
    }

    /// Execute a single CPU instruction (used by the debugger / test harness).
    pub fn step_instruction(&mut self) {
        self.cpu.step(&mut self.bus);
    }

    /// Run until at least `cycles` T-cycles have elapsed.
    pub fn run_cycles(&mut self, cycles: u64) {
        let target = self.bus.cycles + cycles;
        while self.bus.cycles < target {
            self.cpu.step(&mut self.bus);
        }
    }

    pub fn framebuffer(&self) -> &[u8] {
        &self.bus.ppu.fb
    }

    pub fn take_audio(&mut self) -> Vec<f32> {
        self.bus.apu.take_samples()
    }

    pub fn set_button(&mut self, b: Button, pressed: bool) {
        self.bus.joypad.set_button(b, pressed);
    }

    pub fn set_buttons(&mut self, bits: u8) {
        self.bus.joypad.set_buttons(bits);
    }

    pub fn title(&self) -> &str {
        self.bus.cart.title()
    }

    pub fn has_battery(&self) -> bool {
        self.bus.cart.has_battery()
    }

    pub fn ram_is_dirty(&self) -> bool {
        self.bus.cart.ram_dirty
    }

    pub fn save_ram(&mut self) -> Vec<u8> {
        self.bus.cart.ram_dirty = false;
        self.bus.cart.ram_snapshot()
    }

    pub fn load_ram(&mut self, data: &[u8]) {
        self.bus.cart.load_ram(data);
    }

    // ---- link cable (netplay) ---------------------------------------------

    /// Provide the byte the link partner is presenting (0xFF when unplugged).
    pub fn set_link_incoming(&mut self, byte: u8) {
        self.bus.serial.incoming = byte;
    }

    /// If we completed a transfer this step, the byte we shifted out.
    pub fn take_link_sent(&mut self) -> Option<u8> {
        self.bus.serial.sent.take()
    }

    pub fn link_receive_external(&mut self, byte: u8) -> u8 {
        self.bus.serial.receive_external(byte)
    }

    // ---- test / debug -----------------------------------------------------

    pub fn serial_output(&self) -> &[u8] {
        &self.bus.serial.out
    }
}
