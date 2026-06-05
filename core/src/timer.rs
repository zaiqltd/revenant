//! DIV / TIMA / TMA / TAC timer with cycle-accurate falling-edge detection and
//! the TIMA-overflow reload window that the Mooneye timer tests exercise.

#[derive(Clone)]
pub struct Timer {
    /// Full 16-bit internal divider counter. DIV (FF04) = high byte.
    pub div: u16,
    pub tima: u8,
    pub tma: u8,
    pub tac: u8,
    last_and: bool,
    /// Counts down T-cycles from 4 after a TIMA overflow until the TMA reload.
    overflow_counter: u8,
    just_reloaded: bool,
    pub irq: bool,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            div: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            last_and: false,
            overflow_counter: 0,
            just_reloaded: false,
            irq: false,
        }
    }

    /// Used when skipping the boot ROM to seed the post-boot DIV value.
    pub fn set_div_counter(&mut self, v: u16) {
        self.div = v;
        self.last_and = self.tac_and();
    }

    fn selected_bit_pos(&self) -> u8 {
        match self.tac & 0x03 {
            0 => 9,
            1 => 3,
            2 => 5,
            _ => 7,
        }
    }

    fn tac_and(&self) -> bool {
        let bit = (self.div >> self.selected_bit_pos()) & 1 != 0;
        bit && (self.tac & 0x04 != 0)
    }

    /// Advance the timer by `t` T-cycles, one cycle at a time so edges are exact.
    pub fn tick(&mut self, t: u32) {
        for _ in 0..t {
            self.tick_one();
        }
    }

    fn tick_one(&mut self) {
        self.just_reloaded = false;
        self.div = self.div.wrapping_add(1);
        self.detect_edge();

        if self.overflow_counter > 0 {
            self.overflow_counter -= 1;
            if self.overflow_counter == 0 {
                self.tima = self.tma;
                self.irq = true;
                self.just_reloaded = true;
            }
        }
    }

    fn detect_edge(&mut self) {
        let cur = self.tac_and();
        if self.last_and && !cur {
            self.inc_tima();
        }
        self.last_and = cur;
    }

    fn inc_tima(&mut self) {
        let (v, ovf) = self.tima.overflowing_add(1);
        if ovf {
            self.tima = 0;
            self.overflow_counter = 4; // reload one M-cycle later
        } else {
            self.tima = v;
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => (self.div >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | 0xF8,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, v: u8) {
        match addr {
            0xFF04 => {
                // Writing DIV resets the whole counter, which can produce a
                // falling edge on the selected bit and tick TIMA.
                self.div = 0;
                let cur = self.tac_and();
                if self.last_and && !cur {
                    self.inc_tima();
                }
                self.last_and = cur;
            }
            0xFF05 => {
                if self.just_reloaded {
                    // Write on the exact reload cycle is ignored (TMA wins).
                } else {
                    self.tima = v;
                    // Writing during the overflow window cancels the reload + IRQ.
                    self.overflow_counter = 0;
                }
            }
            0xFF06 => {
                self.tma = v;
                if self.just_reloaded {
                    self.tima = v;
                }
            }
            0xFF07 => {
                let old = self.tac_and();
                self.tac = v & 0x07;
                let new = self.tac_and();
                // Changing TAC can also create a falling edge (glitch).
                if old && !new {
                    self.inc_tima();
                }
                self.last_and = new;
            }
            _ => {}
        }
    }
}
