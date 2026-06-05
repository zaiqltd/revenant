//! Audio Processing Unit: two pulse channels, a wave channel, a noise channel,
//! and the 512 Hz frame sequencer (clocked off a DIV bit). Produces stereo f32
//! samples downsampled from the 4.19 MHz base clock to the host sample rate.

const DUTY: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 1, 1, 0],
];
const NOISE_DIVISOR: [u32; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

#[derive(Clone, Default)]
struct Envelope {
    initial: u8,
    add: bool,
    period: u8,
    volume: u8,
    timer: u8,
}
impl Envelope {
    fn trigger(&mut self, next_step_is_env: bool) {
        self.volume = self.initial;
        self.timer = if self.period == 0 { 8 } else { self.period };
        // Trigger-on-envelope-step quirk: if the channel is triggered on a
        // DIV-APU step that *will* clock the envelope next (next step == 7), the
        // envelope timer is loaded with period + 1 (spec §7).
        if next_step_is_env {
            self.timer = self.timer.wrapping_add(1);
        }
    }
    fn clock(&mut self) {
        if self.period == 0 {
            return;
        }
        if self.timer > 0 {
            self.timer -= 1;
        }
        if self.timer == 0 {
            self.timer = self.period;
            if self.add && self.volume < 15 {
                self.volume += 1;
            } else if !self.add && self.volume > 0 {
                self.volume -= 1;
            }
        }
    }
}

#[derive(Clone, Default)]
struct Pulse {
    enabled: bool,
    dac: bool,
    // sweep (ch1 only)
    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_timer: u8,
    sweep_enabled: bool,
    sweep_shadow: u16,
    sweep_did_negate: bool,
    has_sweep: bool,

    duty: u8,
    duty_pos: u8,
    length: u16,
    length_enable: bool,
    freq: u16,
    timer: i32,
    env: Envelope,
}
impl Pulse {
    fn period(&self) -> i32 {
        ((2048 - self.freq as i32) * 4).max(1)
    }
    fn tick(&mut self, c: i32) {
        self.timer -= c;
        while self.timer <= 0 {
            self.timer += self.period();
            self.duty_pos = (self.duty_pos + 1) & 7;
        }
    }
    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }
    fn sweep_calc(&mut self) -> u16 {
        let delta = self.sweep_shadow >> self.sweep_shift;
        let n = if self.sweep_negate {
            self.sweep_did_negate = true;
            self.sweep_shadow.wrapping_sub(delta)
        } else {
            self.sweep_shadow + delta
        };
        n
    }
    fn clock_sweep(&mut self) {
        if !self.has_sweep {
            return;
        }
        if self.sweep_timer > 0 {
            self.sweep_timer -= 1;
        }
        if self.sweep_timer == 0 {
            self.sweep_timer = if self.sweep_period == 0 { 8 } else { self.sweep_period };
            if self.sweep_enabled && self.sweep_period > 0 {
                let new = self.sweep_calc();
                if new > 2047 {
                    self.enabled = false;
                } else if self.sweep_shift > 0 {
                    self.sweep_shadow = new;
                    self.freq = new;
                    // overflow check again
                    if self.sweep_calc() > 2047 {
                        self.enabled = false;
                    }
                }
            }
        }
    }
    fn trigger(&mut self, next_step_is_env: bool) {
        self.enabled = self.dac;
        if self.length == 0 {
            self.length = 64;
        }
        // Trigger period-timer quirk (spec §2): on CH1/CH2 trigger the low 2 bits
        // of the frequency timer are preserved, not reloaded.
        self.timer = (self.period() & !3) | (self.timer & 3);
        self.env.trigger(next_step_is_env);
        if self.has_sweep {
            self.sweep_shadow = self.freq;
            self.sweep_timer = if self.sweep_period == 0 { 8 } else { self.sweep_period };
            self.sweep_enabled = self.sweep_period > 0 || self.sweep_shift > 0;
            self.sweep_did_negate = false;
            if self.sweep_shift > 0 && self.sweep_calc() > 2047 {
                self.enabled = false;
            }
        }
    }
    fn output(&self) -> u8 {
        if !self.enabled || !self.dac {
            return 0;
        }
        if DUTY[self.duty as usize][self.duty_pos as usize] != 0 {
            self.env.volume
        } else {
            0
        }
    }
}

#[derive(Clone)]
struct Wave {
    enabled: bool,
    dac: bool,
    length: u16,
    length_enable: bool,
    freq: u16,
    timer: i32,
    pos: usize,
    volume_code: u8,
    ram: [u8; 16],
    sample_buffer: u8,
}
impl Default for Wave {
    fn default() -> Self {
        Wave {
            enabled: false,
            dac: false,
            length: 0,
            length_enable: false,
            freq: 0,
            timer: 0,
            pos: 0,
            volume_code: 0,
            ram: [0; 16],
            sample_buffer: 0,
        }
    }
}
impl Wave {
    fn period(&self) -> i32 {
        ((2048 - self.freq as i32) * 2).max(1)
    }
    fn tick(&mut self, c: i32) {
        self.timer -= c;
        while self.timer <= 0 {
            self.timer += self.period();
            self.pos = (self.pos + 1) & 31;
            let byte = self.ram[self.pos / 2];
            self.sample_buffer = if self.pos & 1 == 0 { byte >> 4 } else { byte & 0x0F };
        }
    }
    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }
    fn trigger(&mut self) {
        self.enabled = self.dac;
        if self.length == 0 {
            self.length = 256;
        }
        self.timer = self.period();
        self.pos = 0;
    }
    fn output(&self) -> u8 {
        if !self.enabled || !self.dac {
            return 0;
        }
        match self.volume_code {
            0 => 0,
            1 => self.sample_buffer,
            2 => self.sample_buffer >> 1,
            _ => self.sample_buffer >> 2,
        }
    }
}

#[derive(Clone, Default)]
struct Noise {
    enabled: bool,
    dac: bool,
    length: u16,
    length_enable: bool,
    timer: i32,
    lfsr: u16,
    shift: u8,
    width7: bool,
    divisor_code: u8,
    env: Envelope,
}
impl Noise {
    fn period(&self) -> i32 {
        (NOISE_DIVISOR[self.divisor_code as usize] << self.shift) as i32
    }
    fn tick(&mut self, c: i32) {
        self.timer -= c;
        while self.timer <= 0 {
            self.timer += self.period().max(1);
            // Clock shift 14 or 15 stalls the LFSR: the frequency timer keeps
            // running but the shift register receives no clocks (spec §5 /
            // gbdev wiki "Obscure Behavior"). The timer still advances so the
            // channel resumes cleanly once a valid shift is programmed.
            if self.shift < 14 {
                self.clock_lfsr();
            }
        }
    }
    fn clock_lfsr(&mut self) {
        // 15-bit LFSR: XOR the low two bits, shift right, feed the result into
        // bit 14. In 7-bit (width) mode the same bit is also forced into bit 6
        // after the shift, collapsing the period to 7 bits (gbdev wiki / spec §5).
        let bit = (self.lfsr ^ (self.lfsr >> 1)) & 1;
        self.lfsr = (self.lfsr >> 1) | (bit << 14);
        if self.width7 {
            self.lfsr = (self.lfsr & !(1 << 6)) | (bit << 6);
        }
    }
    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }
    fn trigger(&mut self, next_step_is_env: bool) {
        self.enabled = self.dac;
        if self.length == 0 {
            self.length = 64;
        }
        self.timer = self.period().max(1);
        self.lfsr = 0x7FFF;
        self.env.trigger(next_step_is_env);
    }
    fn output(&self) -> u8 {
        if !self.enabled || !self.dac {
            return 0;
        }
        if self.lfsr & 1 == 0 {
            self.env.volume
        } else {
            0
        }
    }
}

#[derive(Clone)]
pub struct Apu {
    power: bool,
    ch1: Pulse,
    ch2: Pulse,
    ch3: Wave,
    ch4: Noise,

    nr50: u8,
    nr51: u8,

    fs_step: u8,
    last_fs_bit: bool,
    double_speed: bool,

    // downsampling
    sample_rate: u32,
    sample_accum: f64,
    cycles_per_sample: f64,
    pub buffer: Vec<f32>, // interleaved stereo
    hp_l: f32,
    hp_r: f32,
}

impl Apu {
    pub fn new(sample_rate: u32) -> Apu {
        let mut ch1 = Pulse::default();
        ch1.has_sweep = true;
        Apu {
            power: false,
            ch1,
            ch2: Pulse::default(),
            ch3: Wave::default(),
            ch4: Noise::default(),
            nr50: 0,
            nr51: 0,
            fs_step: 0,
            last_fs_bit: false,
            double_speed: false,
            sample_rate,
            sample_accum: 0.0,
            cycles_per_sample: 4_194_304.0 / sample_rate as f64,
            buffer: Vec::with_capacity(8192),
            hp_l: 0.0,
            hp_r: 0.0,
        }
    }

    pub fn set_double_speed(&mut self, ds: bool) {
        self.double_speed = ds;
    }

    pub fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    /// Advance channels by `cycles` base-clock T-cycles, watching `div` for the
    /// frame-sequencer falling edge.
    pub fn tick(&mut self, cycles: u32, div: u16) {
        // Frame sequencer: falling edge of DIV bit 12 (or 13 in double speed).
        let bit_pos = if self.double_speed { 13 } else { 12 };
        let fs_bit = (div >> bit_pos) & 1 != 0;
        if self.last_fs_bit && !fs_bit {
            self.step_frame_sequencer();
        }
        self.last_fs_bit = fs_bit;

        if self.power {
            let c = cycles as i32;
            self.ch1.tick(c);
            self.ch2.tick(c);
            self.ch3.tick(c);
            self.ch4.tick(c);
        }

        // sample generation
        self.sample_accum += cycles as f64;
        while self.sample_accum >= self.cycles_per_sample {
            self.sample_accum -= self.cycles_per_sample;
            self.emit_sample();
        }
    }

    fn step_frame_sequencer(&mut self) {
        match self.fs_step {
            0 | 4 => self.clock_length(),
            2 | 6 => {
                self.clock_length();
                self.ch1.clock_sweep();
            }
            7 => {
                self.ch1.env.clock();
                self.ch2.env.clock();
                self.ch4.env.clock();
            }
            _ => {}
        }
        self.fs_step = (self.fs_step + 1) & 7;
    }

    fn clock_length(&mut self) {
        self.ch1.clock_length();
        self.ch2.clock_length();
        self.ch3.clock_length();
        self.ch4.clock_length();
    }

    /// True when the *next* DIV-APU step will clock the length counters
    /// (steps 0,2,4,6). `fs_step` holds the next step to execute.
    fn next_clocks_length(&self) -> bool {
        self.fs_step & 1 == 0
    }

    /// The "first half" of a length period — the next DIV-APU step does NOT
    /// clock length. This is the window in which the extra-length-clock quirk
    /// (spec §6) fires on an NRx4 write.
    fn in_first_half(&self) -> bool {
        !self.next_clocks_length()
    }

    /// True when the *next* DIV-APU step will clock the envelope (step 7).
    fn next_clocks_env(&self) -> bool {
        self.fs_step == 7
    }

    fn emit_sample(&mut self) {
        let c1 = dac(self.ch1.output());
        let c2 = dac(self.ch2.output());
        let c3 = dac(self.ch3.output());
        let c4 = dac(self.ch4.output());

        let mut l = 0.0f32;
        let mut r = 0.0f32;
        let pan = self.nr51;
        if pan & 0x10 != 0 { l += c1 }
        if pan & 0x20 != 0 { l += c2 }
        if pan & 0x40 != 0 { l += c3 }
        if pan & 0x80 != 0 { l += c4 }
        if pan & 0x01 != 0 { r += c1 }
        if pan & 0x02 != 0 { r += c2 }
        if pan & 0x04 != 0 { r += c3 }
        if pan & 0x08 != 0 { r += c4 }

        let vol_l = ((self.nr50 >> 4) & 0x07) as f32 + 1.0;
        let vol_r = (self.nr50 & 0x07) as f32 + 1.0;
        l = l / 4.0 * vol_l / 8.0;
        r = r / 4.0 * vol_r / 8.0;

        // simple DC-blocking high-pass for a clean signal
        let out_l = l - self.hp_l;
        let out_r = r - self.hp_r;
        self.hp_l += out_l * 0.0008;
        self.hp_r += out_r * 0.0008;

        self.buffer.push(out_l);
        self.buffer.push(out_r);
    }

    // ---- register access --------------------------------------------------

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF10 => 0x80 | (self.ch1.sweep_period << 4) | (if self.ch1.sweep_negate { 0x08 } else { 0 }) | self.ch1.sweep_shift,
            0xFF11 => (self.ch1.duty << 6) | 0x3F,
            0xFF12 => env_read(&self.ch1.env),
            0xFF13 => 0xFF,
            0xFF14 => 0xBF | (if self.ch1.length_enable { 0x40 } else { 0 }),
            0xFF16 => (self.ch2.duty << 6) | 0x3F,
            0xFF17 => env_read(&self.ch2.env),
            0xFF18 => 0xFF,
            0xFF19 => 0xBF | (if self.ch2.length_enable { 0x40 } else { 0 }),
            0xFF1A => if self.ch3.dac { 0xFF } else { 0x7F },
            0xFF1B => 0xFF,
            0xFF1C => 0x9F | (self.ch3.volume_code << 5),
            0xFF1D => 0xFF,
            0xFF1E => 0xBF | (if self.ch3.length_enable { 0x40 } else { 0 }),
            0xFF20 => 0xFF,
            0xFF21 => env_read(&self.ch4.env),
            0xFF22 => (self.ch4.shift << 4) | (if self.ch4.width7 { 0x08 } else { 0 }) | self.ch4.divisor_code,
            0xFF23 => 0xBF | (if self.ch4.length_enable { 0x40 } else { 0 }),
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => {
                let mut v = if self.power { 0x80 } else { 0 };
                v |= 0x70;
                if self.ch1.enabled { v |= 0x01 }
                if self.ch2.enabled { v |= 0x02 }
                if self.ch3.enabled { v |= 0x04 }
                if self.ch4.enabled { v |= 0x08 }
                v
            }
            0xFF30..=0xFF3F => self.ch3.ram[(addr - 0xFF30) as usize],
            0xFF76 => dac8(self.ch1.output()) | (dac8(self.ch2.output()) << 4),
            0xFF77 => dac8(self.ch3.output()) | (dac8(self.ch4.output()) << 4),
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, v: u8) {
        // While powered off, only NR52 and (on DMG) length writes are allowed.
        if !self.power && addr != 0xFF26 && !(0xFF30..=0xFF3F).contains(&addr) {
            // allow length-load registers even when off (DMG behavior)
            if !matches!(addr, 0xFF11 | 0xFF16 | 0xFF1B | 0xFF20) {
                return;
            }
        }
        match addr {
            0xFF10 => {
                self.ch1.sweep_period = (v >> 4) & 0x07;
                let neg = v & 0x08 != 0;
                if self.ch1.sweep_negate && !neg && self.ch1.sweep_did_negate {
                    self.ch1.enabled = false;
                }
                self.ch1.sweep_negate = neg;
                self.ch1.sweep_shift = v & 0x07;
            }
            0xFF11 => {
                // While powered off (DMG), only the length load is writable; the
                // duty bits keep their value (blargg 01-registers #6).
                if self.power {
                    self.ch1.duty = v >> 6;
                }
                self.ch1.length = 64 - (v & 0x3F) as u16;
            }
            0xFF12 => {
                env_write(&mut self.ch1.env, v);
                self.ch1.dac = v & 0xF8 != 0;
                if !self.ch1.dac {
                    self.ch1.enabled = false;
                }
            }
            0xFF13 => self.ch1.freq = (self.ch1.freq & 0x700) | v as u16,
            0xFF14 => {
                self.ch1.freq = (self.ch1.freq & 0xFF) | (((v & 0x07) as u16) << 8);
                let first_half = self.in_first_half();
                let next_env = self.next_clocks_env();
                let prev_en = self.ch1.length_enable;
                let trigger = v & 0x80 != 0;
                self.ch1.length_enable = v & 0x40 != 0;
                // Extra length clock on length-enable rising edge in the first half.
                if first_half && !prev_en && self.ch1.length_enable && self.ch1.length > 0 {
                    self.ch1.length -= 1;
                    if self.ch1.length == 0 && !trigger {
                        self.ch1.enabled = false;
                    }
                }
                if trigger {
                    let was_zero = self.ch1.length == 0;
                    self.ch1.trigger(next_env);
                    // 63/255 collision: reloaded-from-0 length in the first half
                    // with length-enable set loads max-1.
                    if was_zero && first_half && self.ch1.length_enable && self.ch1.length > 0 {
                        self.ch1.length -= 1;
                    }
                }
            }
            0xFF16 => {
                if self.power {
                    self.ch2.duty = v >> 6;
                }
                self.ch2.length = 64 - (v & 0x3F) as u16;
            }
            0xFF17 => {
                env_write(&mut self.ch2.env, v);
                self.ch2.dac = v & 0xF8 != 0;
                if !self.ch2.dac {
                    self.ch2.enabled = false;
                }
            }
            0xFF18 => self.ch2.freq = (self.ch2.freq & 0x700) | v as u16,
            0xFF19 => {
                self.ch2.freq = (self.ch2.freq & 0xFF) | (((v & 0x07) as u16) << 8);
                let first_half = self.in_first_half();
                let next_env = self.next_clocks_env();
                let prev_en = self.ch2.length_enable;
                let trigger = v & 0x80 != 0;
                self.ch2.length_enable = v & 0x40 != 0;
                if first_half && !prev_en && self.ch2.length_enable && self.ch2.length > 0 {
                    self.ch2.length -= 1;
                    if self.ch2.length == 0 && !trigger {
                        self.ch2.enabled = false;
                    }
                }
                if trigger {
                    let was_zero = self.ch2.length == 0;
                    self.ch2.trigger(next_env);
                    if was_zero && first_half && self.ch2.length_enable && self.ch2.length > 0 {
                        self.ch2.length -= 1;
                    }
                }
            }
            0xFF1A => {
                self.ch3.dac = v & 0x80 != 0;
                if !self.ch3.dac {
                    self.ch3.enabled = false;
                }
            }
            0xFF1B => self.ch3.length = 256 - v as u16,
            0xFF1C => self.ch3.volume_code = (v >> 5) & 0x03,
            0xFF1D => self.ch3.freq = (self.ch3.freq & 0x700) | v as u16,
            0xFF1E => {
                self.ch3.freq = (self.ch3.freq & 0xFF) | (((v & 0x07) as u16) << 8);
                let first_half = self.in_first_half();
                let prev_en = self.ch3.length_enable;
                let trigger = v & 0x80 != 0;
                self.ch3.length_enable = v & 0x40 != 0;
                if first_half && !prev_en && self.ch3.length_enable && self.ch3.length > 0 {
                    self.ch3.length -= 1;
                    if self.ch3.length == 0 && !trigger {
                        self.ch3.enabled = false;
                    }
                }
                if trigger {
                    let was_zero = self.ch3.length == 0;
                    self.ch3.trigger();
                    if was_zero && first_half && self.ch3.length_enable && self.ch3.length > 0 {
                        self.ch3.length -= 1;
                    }
                }
            }
            0xFF20 => self.ch4.length = 64 - (v & 0x3F) as u16,
            0xFF21 => {
                env_write(&mut self.ch4.env, v);
                self.ch4.dac = v & 0xF8 != 0;
                if !self.ch4.dac {
                    self.ch4.enabled = false;
                }
            }
            0xFF22 => {
                self.ch4.shift = (v >> 4) & 0x0F;
                self.ch4.width7 = v & 0x08 != 0;
                self.ch4.divisor_code = v & 0x07;
            }
            0xFF23 => {
                let first_half = self.in_first_half();
                let next_env = self.next_clocks_env();
                let prev_en = self.ch4.length_enable;
                let trigger = v & 0x80 != 0;
                self.ch4.length_enable = v & 0x40 != 0;
                if first_half && !prev_en && self.ch4.length_enable && self.ch4.length > 0 {
                    self.ch4.length -= 1;
                    if self.ch4.length == 0 && !trigger {
                        self.ch4.enabled = false;
                    }
                }
                if trigger {
                    let was_zero = self.ch4.length == 0;
                    self.ch4.trigger(next_env);
                    if was_zero && first_half && self.ch4.length_enable && self.ch4.length > 0 {
                        self.ch4.length -= 1;
                    }
                }
            }
            0xFF24 => self.nr50 = v,
            0xFF25 => self.nr51 = v,
            0xFF26 => {
                let on = v & 0x80 != 0;
                if !on && self.power {
                    self.power_off();
                } else if on && !self.power {
                    self.power = true;
                    self.fs_step = 0;
                }
            }
            0xFF30..=0xFF3F => self.ch3.ram[(addr - 0xFF30) as usize] = v,
            _ => {}
        }
    }

    fn power_off(&mut self) {
        let ram = self.ch3.ram;
        let ch1 = Pulse { has_sweep: true, ..Pulse::default() };
        self.ch1 = ch1;
        self.ch2 = Pulse::default();
        let mut ch3 = Wave::default();
        ch3.ram = ram; // wave RAM is preserved across power
        self.ch3 = ch3;
        self.ch4 = Noise::default();
        self.nr50 = 0;
        self.nr51 = 0;
        self.power = false;
    }
}

fn dac(input: u8) -> f32 {
    // 0..15 digital -> -1.0..1.0 analog (DAC), 0 -> +1, 15 -> -1 is the real
    // polarity, but for mixing magnitude we use a centered conversion.
    (input as f32) / 7.5 - 1.0
}
fn dac8(input: u8) -> u8 {
    input & 0x0F
}
fn env_read(e: &Envelope) -> u8 {
    (e.initial << 4) | (if e.add { 0x08 } else { 0 }) | e.period
}
fn env_write(e: &mut Envelope, v: u8) {
    e.initial = v >> 4;
    e.add = v & 0x08 != 0;
    e.period = v & 0x07;
}
