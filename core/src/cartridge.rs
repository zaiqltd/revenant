//! Cartridge: header parsing + memory bank controllers (MBC0/1/3/5) with MBC3 RTC.
//!
//! RTC is driven by emulated cycles (not wall-clock) so the whole machine stays
//! deterministic — a hard requirement for rewind and lockstep netplay. One RTC
//! second elapses every 4_194_304 T-cycles of base clock.

pub const T_CYCLES_PER_SECOND: u64 = 4_194_304;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CgbFlag {
    /// Game ignores CGB functions (DMG game).
    None,
    /// Game supports CGB enhancements but also runs on DMG ($80).
    Supported,
    /// Game requires CGB ($C0).
    Only,
}

/// Parsed cartridge header fields we care about.
#[derive(Clone, Debug)]
pub struct Header {
    pub title: String,
    pub cgb_flag: CgbFlag,
    pub cart_type: u8,
    pub rom_banks: usize,
    pub ram_size: usize,
    pub has_battery: bool,
    pub has_rtc: bool,
    pub has_rumble: bool,
    pub header_checksum_ok: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    RomOnly,
    Mbc1,
    Mbc2,
    Mbc3,
    Mbc5,
}

/// Real-time clock state for MBC3. Latched values are exposed to the program.
#[derive(Clone, Default)]
pub struct Rtc {
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub days: u16, // 9-bit day counter (bit 8 lives in dh)
    pub halt: bool,
    pub day_carry: bool,
    // latched snapshot
    pub l_seconds: u8,
    pub l_minutes: u8,
    pub l_hours: u8,
    pub l_days_lo: u8,
    pub l_dh: u8,
    // sub-second accumulator (in T-cycles)
    pub sub: u64,
    last_latch: u8,
}

impl Rtc {
    fn tick(&mut self, cycles: u64) {
        if self.halt {
            return;
        }
        self.sub += cycles;
        while self.sub >= T_CYCLES_PER_SECOND {
            self.sub -= T_CYCLES_PER_SECOND;
            self.advance_one_second();
        }
    }

    fn advance_one_second(&mut self) {
        self.seconds = self.seconds.wrapping_add(1);
        if self.seconds >= 60 {
            self.seconds = 0;
            self.minutes = self.minutes.wrapping_add(1);
            if self.minutes >= 60 {
                self.minutes = 0;
                self.hours = self.hours.wrapping_add(1);
                if self.hours >= 24 {
                    self.hours = 0;
                    let d = self.days as u32 + 1;
                    if d > 0x1FF {
                        self.days = 0;
                        self.day_carry = true;
                    } else {
                        self.days = d as u16;
                    }
                }
            }
        }
    }

    fn latch(&mut self) {
        self.l_seconds = self.seconds & 0x3F;
        self.l_minutes = self.minutes & 0x3F;
        self.l_hours = self.hours & 0x1F;
        self.l_days_lo = (self.days & 0xFF) as u8;
        let mut dh = ((self.days >> 8) & 1) as u8;
        if self.halt {
            dh |= 0x40;
        }
        if self.day_carry {
            dh |= 0x80;
        }
        self.l_dh = dh;
    }

    fn write_latch(&mut self, v: u8) {
        if self.last_latch == 0 && v == 1 {
            self.latch();
        }
        self.last_latch = v;
    }

    fn write_reg(&mut self, reg: u8, v: u8) {
        match reg {
            0x08 => self.seconds = v & 0x3F,
            0x09 => self.minutes = v & 0x3F,
            0x0A => self.hours = v & 0x1F,
            0x0B => self.days = (self.days & 0x100) | v as u16,
            0x0C => {
                self.days = (self.days & 0xFF) | (((v & 1) as u16) << 8);
                self.halt = v & 0x40 != 0;
                self.day_carry = v & 0x80 != 0;
            }
            _ => {}
        }
        // Writing live registers also resets the sub-second accumulator on real HW.
        self.sub = 0;
        // keep latched view consistent if the program reads back immediately
    }

    fn read_reg(&self, reg: u8) -> u8 {
        match reg {
            0x08 => self.l_seconds,
            0x09 => self.l_minutes,
            0x0A => self.l_hours,
            0x0B => self.l_days_lo,
            0x0C => self.l_dh,
            _ => 0xFF,
        }
    }
}

pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    kind: Kind,
    pub header: Header,

    // banking state
    rom_bank: usize,
    ram_bank: usize,
    ram_enabled: bool,
    // MBC1 specifics
    bank_lo: u8,    // 5 bits
    bank_hi: u8,    // 2 bits (BANK2)
    mode: bool,     // MBC1 banking mode
    multicart: bool, // MBC1M wiring (4x256KiB multicart): BANK2 shifted to bits 4-5
    // MBC3 RTC
    pub rtc: Rtc,
    rtc_select: Option<u8>, // 0x08..=0x0C when an RTC register is mapped into A000-BFFF
    pub rtc_present: bool,

    pub ram_dirty: bool,
}

impl Cartridge {
    pub fn new(rom: Vec<u8>) -> Cartridge {
        let header = parse_header(&rom);
        let kind = kind_for(header.cart_type);
        let ram_size = if kind == Kind::Mbc2 {
            512 // MBC2 has 512x4 bits of built-in RAM
        } else {
            header.ram_size.max(0)
        };
        let ram = vec![0u8; ram_size.max(if header.has_battery { 0 } else { 0 })];
        let rtc_present = header.has_rtc;
        let multicart = kind == Kind::Mbc1 && detect_mbc1_multicart(&rom);
        Cartridge {
            rom,
            ram,
            kind,
            header,
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            bank_lo: 1,
            bank_hi: 0,
            mode: false,
            multicart,
            rtc: Rtc::default(),
            rtc_select: None,
            rtc_present,
            ram_dirty: false,
        }
    }

    pub fn title(&self) -> &str {
        &self.header.title
    }

    /// Advance any cycle-driven cartridge hardware (MBC3 RTC).
    pub fn tick(&mut self, cycles: u64) {
        if self.rtc_present {
            self.rtc.tick(cycles);
        }
    }

    pub fn has_battery(&self) -> bool {
        self.header.has_battery
    }

    fn rom_mask(&self) -> usize {
        self.header.rom_banks.max(1) - 1
    }

    pub fn read_rom(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                let bank = match self.kind {
                    Kind::Mbc1 if self.mode => {
                        // In MBC1 advanced mode the high bits select the bank-0 region too.
                        // Multicarts (MBC1M) wire BANK2 to ROM bits 4-5 (base $00/$10/$20/$30);
                        // standard carts wire it to bits 5-6 (base $00/$20/$40/$60).
                        let shift = if self.multicart { 4 } else { 5 };
                        (self.bank_hi as usize) << shift
                    }
                    _ => 0,
                };
                let bank = bank & self.rom_mask();
                let idx = bank * 0x4000 + addr as usize;
                self.rom.get(idx).copied().unwrap_or(0xFF)
            }
            0x4000..=0x7FFF => {
                let bank = self.effective_rom_bank() & self.rom_mask();
                let idx = bank * 0x4000 + (addr as usize - 0x4000);
                self.rom.get(idx).copied().unwrap_or(0xFF)
            }
            _ => 0xFF,
        }
    }

    fn effective_rom_bank(&self) -> usize {
        match self.kind {
            Kind::RomOnly => 1,
            Kind::Mbc1 => {
                let lo = self.bank_lo as usize & 0x1F;
                let lo = if lo == 0 { 1 } else { lo };
                if self.multicart {
                    // MBC1M: only the low 4 bits of BANK1 address ROM (bit 4 ignored),
                    // and BANK2 occupies bits 4-5 (base $00/$10/$20/$30).
                    (lo & 0x0F) | ((self.bank_hi as usize) << 4)
                } else {
                    lo | ((self.bank_hi as usize) << 5)
                }
            }
            Kind::Mbc2 => {
                let b = self.rom_bank & 0x0F;
                if b == 0 {
                    1
                } else {
                    b
                }
            }
            Kind::Mbc3 => {
                let b = self.rom_bank & 0x7F;
                if b == 0 {
                    1
                } else {
                    b
                }
            }
            Kind::Mbc5 => self.rom_bank & 0x1FF,
        }
    }

    pub fn write_rom(&mut self, addr: u16, v: u8) {
        match self.kind {
            Kind::RomOnly => {}
            Kind::Mbc1 => self.write_mbc1(addr, v),
            Kind::Mbc2 => self.write_mbc2(addr, v),
            Kind::Mbc3 => self.write_mbc3(addr, v),
            Kind::Mbc5 => self.write_mbc5(addr, v),
        }
    }

    fn write_mbc1(&mut self, addr: u16, v: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (v & 0x0F) == 0x0A,
            0x2000..=0x3FFF => {
                let mut lo = v & 0x1F;
                if lo == 0 {
                    lo = 1;
                }
                self.bank_lo = lo;
            }
            0x4000..=0x5FFF => self.bank_hi = v & 0x03,
            0x6000..=0x7FFF => self.mode = v & 1 != 0,
            _ => {}
        }
    }

    fn write_mbc2(&mut self, addr: u16, v: u8) {
        if addr < 0x4000 {
            // bit 8 of address selects ram-enable vs rom-bank
            if addr & 0x0100 == 0 {
                self.ram_enabled = (v & 0x0F) == 0x0A;
            } else {
                let b = v & 0x0F;
                self.rom_bank = if b == 0 { 1 } else { b as usize };
            }
        }
    }

    fn write_mbc3(&mut self, addr: u16, v: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (v & 0x0F) == 0x0A,
            0x2000..=0x3FFF => {
                let b = (v & 0x7F) as usize;
                self.rom_bank = if b == 0 { 1 } else { b };
            }
            0x4000..=0x5FFF => match v {
                0x00..=0x07 => {
                    self.ram_bank = v as usize;
                    self.rtc_select = None;
                }
                0x08..=0x0C => {
                    self.rtc_select = Some(v);
                }
                _ => {}
            },
            0x6000..=0x7FFF => {
                if self.rtc_present {
                    self.rtc.write_latch(v);
                }
            }
            _ => {}
        }
    }

    fn write_mbc5(&mut self, addr: u16, v: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = (v & 0x0F) == 0x0A,
            0x2000..=0x2FFF => self.rom_bank = (self.rom_bank & 0x100) | v as usize,
            0x3000..=0x3FFF => self.rom_bank = (self.rom_bank & 0x0FF) | (((v & 1) as usize) << 8),
            0x4000..=0x5FFF => {
                // bit 3 is rumble for rumble carts; low 4 bits select RAM bank
                self.ram_bank = (v & 0x0F) as usize;
            }
            _ => {}
        }
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }
        if let Some(reg) = self.rtc_select {
            if self.rtc_present {
                return self.rtc.read_reg(reg);
            }
        }
        if self.kind == Kind::Mbc2 {
            let idx = (addr as usize - 0xA000) & 0x1FF;
            return self.ram.get(idx).copied().unwrap_or(0xFF) | 0xF0;
        }
        let bank = self.effective_ram_bank();
        let idx = bank * 0x2000 + (addr as usize - 0xA000);
        self.ram.get(idx).copied().unwrap_or(0xFF)
    }

    pub fn write_ram(&mut self, addr: u16, v: u8) {
        if !self.ram_enabled {
            return;
        }
        if let Some(reg) = self.rtc_select {
            if self.rtc_present {
                self.rtc.write_reg(reg, v);
                self.ram_dirty = true;
            }
            return;
        }
        if self.kind == Kind::Mbc2 {
            let idx = (addr as usize - 0xA000) & 0x1FF;
            if let Some(slot) = self.ram.get_mut(idx) {
                *slot = v & 0x0F;
                self.ram_dirty = true;
            }
            return;
        }
        let bank = self.effective_ram_bank();
        let idx = bank * 0x2000 + (addr as usize - 0xA000);
        if let Some(slot) = self.ram.get_mut(idx) {
            *slot = v;
            self.ram_dirty = true;
        }
    }

    fn effective_ram_bank(&self) -> usize {
        let bank = match self.kind {
            Kind::Mbc1 => {
                // MODE 1 selects RAM bank via BANK2; MODE 0 is always bank 0.
                // Multicarts have no external RAM banking (RAM is fixed at bank 0).
                if self.mode && !self.multicart {
                    self.bank_hi as usize
                } else {
                    0
                }
            }
            Kind::Mbc3 => self.ram_bank & 0x07,
            Kind::Mbc5 => self.ram_bank & 0x0F,
            _ => 0,
        };
        // Mask the selected bank to the number of banks actually present so that
        // selecting an out-of-range bank wraps (aliases) rather than reading open bus.
        bank & self.ram_bank_mask()
    }

    /// Mask covering the available 8 KiB external RAM banks (0 when ≤1 bank).
    fn ram_bank_mask(&self) -> usize {
        let banks = self.ram.len() / 0x2000;
        if banks <= 1 {
            0
        } else {
            // RAM bank counts in the cartridge header are powers of two.
            banks.next_power_of_two() - 1
        }
    }

    // ---- battery save / load ----------------------------------------------

    pub fn ram_snapshot(&self) -> Vec<u8> {
        let mut out = self.ram.clone();
        if self.rtc_present {
            out.extend_from_slice(&self.serialize_rtc());
        }
        out
    }

    pub fn load_ram(&mut self, data: &[u8]) {
        let ram_len = self.ram.len();
        let copy = ram_len.min(data.len());
        self.ram[..copy].copy_from_slice(&data[..copy]);
        if self.rtc_present && data.len() >= ram_len + 48 {
            self.deserialize_rtc(&data[ram_len..]);
        }
        self.ram_dirty = false;
    }

    fn serialize_rtc(&self) -> [u8; 48] {
        // BGB-compatible-ish: 10 little-endian u32 RTC fields + 8-byte unix base (0).
        let mut b = [0u8; 48];
        let fields = [
            self.rtc.seconds as u32,
            self.rtc.minutes as u32,
            self.rtc.hours as u32,
            (self.rtc.days & 0xFF) as u32,
            (((self.rtc.days >> 8) & 1)
                | if self.rtc.halt { 0x40 } else { 0 }
                | if self.rtc.day_carry { 0x80 } else { 0 }) as u32,
            self.rtc.l_seconds as u32,
            self.rtc.l_minutes as u32,
            self.rtc.l_hours as u32,
            self.rtc.l_days_lo as u32,
            self.rtc.l_dh as u32,
        ];
        for (i, f) in fields.iter().enumerate() {
            b[i * 4..i * 4 + 4].copy_from_slice(&f.to_le_bytes());
        }
        b
    }

    fn deserialize_rtc(&mut self, d: &[u8]) {
        let rd = |i: usize| -> u32 {
            u32::from_le_bytes([d[i * 4], d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3]])
        };
        self.rtc.seconds = rd(0) as u8 & 0x3F;
        self.rtc.minutes = rd(1) as u8 & 0x3F;
        self.rtc.hours = rd(2) as u8 & 0x1F;
        let dh = rd(4) as u8;
        self.rtc.days = (rd(3) as u16 & 0xFF) | (((dh & 1) as u16) << 8);
        self.rtc.halt = dh & 0x40 != 0;
        self.rtc.day_carry = dh & 0x80 != 0;
        self.rtc.l_seconds = rd(5) as u8;
        self.rtc.l_minutes = rd(6) as u8;
        self.rtc.l_hours = rd(7) as u8;
        self.rtc.l_days_lo = rd(8) as u8;
        self.rtc.l_dh = rd(9) as u8;
    }
}

/// First 24 bytes of the Nintendo boot logo (enough to identify a header reliably).
const NINTENDO_LOGO_HEAD: [u8; 24] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E,
];

/// Detect an MBC1 multicart (MBC1M): a 1 MiB compilation of four 256 KiB games.
///
/// Heuristic (per the mappers spec §2.7): only 1 MiB MBC1 carts qualify, and a
/// genuine multicart repeats the Nintendo logo at the start of each game base
/// bank ($00/$10/$20/$30). We require the logo at ≥2 of those four banks, which
/// distinguishes a multicart from an ordinary 1 MiB game (logo only at bank 0).
fn detect_mbc1_multicart(rom: &[u8]) -> bool {
    // Must be exactly 1 MiB (64 banks of 16 KiB) to be a 4×256 KiB multicart.
    if rom.len() != 64 * 0x4000 {
        return false;
    }
    let mut logo_banks = 0;
    for base in [0x00usize, 0x10, 0x20, 0x30] {
        let off = base * 0x4000 + 0x0104;
        if rom
            .get(off..off + NINTENDO_LOGO_HEAD.len())
            .is_some_and(|slice| slice == NINTENDO_LOGO_HEAD)
        {
            logo_banks += 1;
        }
    }
    logo_banks >= 2
}

fn kind_for(t: u8) -> Kind {
    match t {
        0x00 | 0x08 | 0x09 => Kind::RomOnly,
        0x01..=0x03 => Kind::Mbc1,
        0x05 | 0x06 => Kind::Mbc2,
        0x0F..=0x13 => Kind::Mbc3,
        0x19..=0x1E => Kind::Mbc5,
        _ => Kind::RomOnly,
    }
}

fn parse_header(rom: &[u8]) -> Header {
    let get = |i: usize| rom.get(i).copied().unwrap_or(0);
    let cart_type = get(0x0147);
    let cgb = get(0x0143);
    let cgb_flag = match cgb {
        0x80 => CgbFlag::Supported,
        0xC0 => CgbFlag::Only,
        _ => CgbFlag::None,
    };
    let title_end = if cgb_flag == CgbFlag::None { 0x0144 } else { 0x0143 };
    let mut title = String::new();
    for i in 0x0134..title_end {
        let c = get(i);
        if c == 0 {
            break;
        }
        if c.is_ascii_graphic() || c == b' ' {
            title.push(c as char);
        }
    }
    let rom_banks = match get(0x0148) {
        n @ 0x00..=0x08 => 2usize << n,
        0x52 => 72,
        0x53 => 80,
        0x54 => 96,
        _ => 2,
    };
    let ram_size = match get(0x0149) {
        0x00 => 0,
        0x01 => 0x800, // unofficial 2KB
        0x02 => 0x2000,
        0x03 => 0x8000,
        0x04 => 0x20000,
        0x05 => 0x10000,
        _ => 0,
    };
    let has_battery = matches!(
        cart_type,
        0x03 | 0x06 | 0x09 | 0x0D | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E | 0x22 | 0xFF
    );
    let has_rtc = matches!(cart_type, 0x0F | 0x10);
    let has_rumble = matches!(cart_type, 0x1C | 0x1D | 0x1E);

    // Header checksum: x = 0; for i in 0x134..=0x14C { x = x - rom[i] - 1 }
    let mut x: u8 = 0;
    for i in 0x0134..=0x014C {
        x = x.wrapping_sub(get(i)).wrapping_sub(1);
    }
    let header_checksum_ok = x == get(0x014D);

    Header {
        title,
        cgb_flag,
        cart_type,
        rom_banks,
        ram_size,
        has_battery,
        has_rtc,
        has_rumble,
        header_checksum_ok,
    }
}
