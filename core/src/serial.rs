//! Serial link (FF01 SB / FF02 SC). Drives byte-level transfers used both by the
//! Blargg test harness (which prints results to serial) and by link-cable netplay.

#[derive(Clone)]
pub struct Serial {
    pub sb: u8,
    pub sc: u8,
    /// T-cycles remaining in the current transfer (internal clock only).
    counter: i32,
    transferring: bool,
    pub irq: bool,
    /// Bytes shifted out, captured for tests / debugging.
    pub out: Vec<u8>,
    /// Byte presented by the link partner (0xFF when unplugged).
    pub incoming: u8,
    /// When a transfer completes, the byte we sent (for netplay forwarding).
    pub sent: Option<u8>,
    double_speed: bool,
}

const BITS: i32 = 8;
const T_PER_BIT: i32 = 512; // 8192 Hz bit clock at base speed

impl Serial {
    pub fn new() -> Serial {
        Serial {
            sb: 0x00,
            sc: 0x00,
            counter: 0,
            transferring: false,
            irq: false,
            out: Vec::new(),
            incoming: 0xFF,
            sent: None,
            double_speed: false,
        }
    }

    pub fn set_double_speed(&mut self, ds: bool) {
        self.double_speed = ds;
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF01 => self.sb,
            0xFF02 => self.sc | 0x7E,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, v: u8) {
        match addr {
            0xFF01 => self.sb = v,
            0xFF02 => {
                self.sc = v & 0x83;
                if v & 0x80 != 0 && v & 0x01 != 0 {
                    // Start transfer with internal clock.
                    self.transferring = true;
                    self.counter = T_PER_BIT * BITS;
                }
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, t: u32) {
        if !self.transferring {
            return;
        }
        let step = if self.double_speed { (t as i32) * 2 } else { t as i32 };
        self.counter -= step;
        if self.counter <= 0 {
            self.complete();
        }
    }

    fn complete(&mut self) {
        let sent = self.sb;
        self.out.push(sent);
        self.sent = Some(sent);
        self.sb = self.incoming;
        self.incoming = 0xFF;
        self.sc &= !0x80;
        self.transferring = false;
        self.irq = true;
    }

    /// Feed a byte from an external clock partner (link cable). Returns the byte
    /// we present back. Used by netplay when the *other* console clocks.
    pub fn receive_external(&mut self, byte: u8) -> u8 {
        let ours = self.sb;
        if self.sc & 0x80 != 0 && self.sc & 0x01 == 0 {
            // We were waiting on an external clock: exchange now.
            self.sb = byte;
            self.sc &= !0x80;
            self.irq = true;
            self.out.push(ours);
        }
        ours
    }
}
