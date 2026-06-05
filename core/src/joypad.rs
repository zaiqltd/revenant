//! Joypad register (FF00). Inputs are active-low; the program selects either the
//! direction group or the action group via bits 4/5.

#[derive(Clone, Copy)]
pub enum Button {
    Right = 0,
    Left = 1,
    Up = 2,
    Down = 3,
    A = 4,
    B = 5,
    Select = 6,
    Start = 7,
}

#[derive(Clone)]
pub struct Joypad {
    /// bit set = pressed. Layout: [Right,Left,Up,Down,A,B,Select,Start].
    state: u8,
    select_buttons: bool,   // P15 (bit5) low
    select_direction: bool, // P14 (bit4) low
    pub irq: bool,
}

impl Joypad {
    pub fn new() -> Joypad {
        Joypad {
            state: 0,
            select_buttons: false,
            select_direction: false,
            irq: false,
        }
    }

    pub fn set_buttons(&mut self, bits: u8) {
        let before = self.line_low();
        self.state = bits;
        let after = self.line_low();
        // Any input line going high->low (newly pressed & selected) raises IRQ.
        if (before & !after) != 0 {
            self.irq = true;
        }
    }

    pub fn set_button(&mut self, b: Button, pressed: bool) {
        let mask = 1u8 << (b as u8);
        let mut s = self.state;
        if pressed {
            s |= mask;
        } else {
            s &= !mask;
        }
        self.set_buttons(s);
    }

    /// Returns the 4 currently-driven input lines as active-low bits (1 = low/pressed).
    fn line_low(&self) -> u8 {
        let mut low = 0u8;
        if self.select_direction {
            low |= self.state & 0x0F; // Right,Left,Up,Down -> bits 0..3
        }
        if self.select_buttons {
            low |= (self.state >> 4) & 0x0F; // A,B,Select,Start -> bits 0..3
        }
        low
    }

    pub fn read(&self) -> u8 {
        let mut v = 0xCF; // bits 6,7 always 1; lines default high
        if self.select_buttons {
            v &= !0x20;
        }
        if self.select_direction {
            v &= !0x10;
        }
        // Pressed (selected) lines read as 0.
        v &= !self.line_low();
        v
    }

    pub fn write(&mut self, v: u8) {
        self.select_direction = v & 0x10 == 0;
        self.select_buttons = v & 0x20 == 0;
    }
}
