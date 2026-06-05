//! Sharp SM83 / LR35902 CPU. M-cycle accurate: every memory access ticks the bus
//! one machine cycle, so peripheral timing falls out naturally. Implements the
//! full 256 + 256 (CB) opcode set, exact flags, EI delay, HALT + HALT bug, STOP,
//! and interrupt dispatch (including the cancellation quirk).

use crate::bus::Bus;

pub const FLAG_Z: u8 = 0x80;
pub const FLAG_N: u8 = 0x40;
pub const FLAG_H: u8 = 0x20;
pub const FLAG_C: u8 = 0x10;

#[derive(Clone)]
pub struct Cpu {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
    pub ime: bool,
    pub halted: bool,
    halt_bug: bool,
    ei_pending: bool,
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0,
            pc: 0,
            ime: false,
            halted: false,
            halt_bug: false,
            ei_pending: false,
        }
    }

    pub fn set_post_boot(&mut self, cgb: bool, header_checksum: u8) {
        if cgb {
            self.a = 0x11;
            self.f = 0x80;
            self.b = 0x00;
            self.c = 0x00;
            self.d = 0xFF;
            self.e = 0x56;
            self.h = 0x00;
            self.l = 0x0D;
        } else {
            self.a = 0x01;
            // DMG boot leaves H/C clear iff the header checksum byte is 0.
            self.f = if header_checksum == 0 { 0x80 } else { 0xB0 };
            self.b = 0x00;
            self.c = 0x13;
            self.d = 0x00;
            self.e = 0xD8;
            self.h = 0x01;
            self.l = 0x4D;
        }
        self.sp = 0xFFFE;
        self.pc = 0x0100;
        self.ime = false;
    }

    // ---- 16-bit register pairs --------------------------------------------
    fn af(&self) -> u16 {
        ((self.a as u16) << 8) | self.f as u16
    }
    fn bc(&self) -> u16 {
        ((self.b as u16) << 8) | self.c as u16
    }
    fn de(&self) -> u16 {
        ((self.d as u16) << 8) | self.e as u16
    }
    fn hl(&self) -> u16 {
        ((self.h as u16) << 8) | self.l as u16
    }
    fn set_af(&mut self, v: u16) {
        self.a = (v >> 8) as u8;
        self.f = (v as u8) & 0xF0;
    }
    fn set_bc(&mut self, v: u16) {
        self.b = (v >> 8) as u8;
        self.c = v as u8;
    }
    fn set_de(&mut self, v: u16) {
        self.d = (v >> 8) as u8;
        self.e = v as u8;
    }
    fn set_hl(&mut self, v: u16) {
        self.h = (v >> 8) as u8;
        self.l = v as u8;
    }

    fn flag(&self, m: u8) -> bool {
        self.f & m != 0
    }
    fn set_flag(&mut self, m: u8, on: bool) {
        if on {
            self.f |= m;
        } else {
            self.f &= !m;
        }
        self.f &= 0xF0;
    }

    // ---- ticked memory primitives -----------------------------------------
    fn fetch8(&mut self, bus: &mut Bus) -> u8 {
        let v = bus.tick_read(self.pc);
        if self.halt_bug {
            self.halt_bug = false;
        } else {
            self.pc = self.pc.wrapping_add(1);
        }
        v
    }
    fn fetch16(&mut self, bus: &mut Bus) -> u16 {
        let lo = self.fetch8(bus) as u16;
        let hi = self.fetch8(bus) as u16;
        (hi << 8) | lo
    }
    fn read8(&mut self, bus: &mut Bus, addr: u16) -> u8 {
        bus.tick_read(addr)
    }
    fn write8(&mut self, bus: &mut Bus, addr: u16, v: u8) {
        bus.tick_write(addr, v);
    }
    fn internal(&mut self, bus: &mut Bus) {
        bus.tick();
    }

    fn push16(&mut self, bus: &mut Bus, v: u16) {
        self.internal(bus);
        self.sp = self.sp.wrapping_sub(1);
        self.write8(bus, self.sp, (v >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        self.write8(bus, self.sp, v as u8);
    }
    fn pop16(&mut self, bus: &mut Bus) -> u16 {
        let lo = self.read8(bus, self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        let hi = self.read8(bus, self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        (hi << 8) | lo
    }

    // ---- r[] operand access (index 6 = (HL), ticks) -----------------------
    fn read_r(&mut self, bus: &mut Bus, idx: u8) -> u8 {
        match idx {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => {
                let a = self.hl();
                self.read8(bus, a)
            }
            _ => self.a,
        }
    }
    fn write_r(&mut self, bus: &mut Bus, idx: u8, v: u8) {
        match idx {
            0 => self.b = v,
            1 => self.c = v,
            2 => self.d = v,
            3 => self.e = v,
            4 => self.h = v,
            5 => self.l = v,
            6 => {
                let a = self.hl();
                self.write8(bus, a, v);
            }
            _ => self.a = v,
        }
    }

    fn cond(&self, y: u8) -> bool {
        match y {
            0 => !self.flag(FLAG_Z),
            1 => self.flag(FLAG_Z),
            2 => !self.flag(FLAG_C),
            _ => self.flag(FLAG_C),
        }
    }

    // ---- step -------------------------------------------------------------
    pub fn step(&mut self, bus: &mut Bus) {
        let pending = bus.pending_interrupts();

        if self.halted {
            if pending != 0 {
                self.halted = false;
            } else {
                bus.tick();
                return;
            }
        }

        if self.ime && pending != 0 {
            self.service_interrupt(bus);
            return;
        }

        // EI takes effect after the interrupt check but before the next opcode.
        if self.ei_pending {
            self.ime = true;
            self.ei_pending = false;
        }

        let op = self.fetch8(bus);
        if op == 0xCB {
            let cb = self.fetch8(bus);
            self.execute_cb(bus, cb);
        } else {
            self.execute(bus, op);
        }
    }

    fn service_interrupt(&mut self, bus: &mut Bus) {
        self.ime = false;
        self.internal(bus);
        self.internal(bus);
        let pc = self.pc;
        self.sp = self.sp.wrapping_sub(1);
        self.write8(bus, self.sp, (pc >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        self.write8(bus, self.sp, pc as u8);
        // The vector is decided here; if IE was cleared in the meantime, the
        // dispatch is cancelled and we jump to 0x0000.
        let pending = bus.pending_interrupts();
        let vector = if pending == 0 {
            0x0000
        } else {
            let bit = pending.trailing_zeros() as u8;
            bus.intf &= !(1 << bit);
            0x0040 + (bit as u16) * 8
        };
        self.pc = vector;
        self.internal(bus);
    }

    fn execute(&mut self, bus: &mut Bus, op: u8) {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let p = y >> 1;
        let q = y & 1;

        match x {
            0 => self.exec_x0(bus, y, z, p, q),
            1 => {
                if y == 6 && z == 6 {
                    self.op_halt(bus);
                } else {
                    let v = self.read_r(bus, z);
                    self.write_r(bus, y, v);
                }
            }
            2 => {
                let v = self.read_r(bus, z);
                self.alu(y, v);
            }
            _ => self.exec_x3(bus, y, z, p, q),
        }
    }

    fn exec_x0(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) {
        match z {
            0 => match y {
                0 => {} // NOP
                1 => {
                    // LD (a16),SP
                    let addr = self.fetch16(bus);
                    self.write8(bus, addr, self.sp as u8);
                    self.write8(bus, addr.wrapping_add(1), (self.sp >> 8) as u8);
                }
                2 => self.op_stop(bus),
                3 => {
                    // JR e
                    let e = self.fetch8(bus) as i8 as i16;
                    self.internal(bus);
                    self.pc = (self.pc as i16).wrapping_add(e) as u16;
                }
                _ => {
                    // JR cc, e
                    let e = self.fetch8(bus) as i8 as i16;
                    if self.cond(y - 4) {
                        self.internal(bus);
                        self.pc = (self.pc as i16).wrapping_add(e) as u16;
                    }
                }
            },
            1 => {
                if q == 0 {
                    // LD rp,nn
                    let v = self.fetch16(bus);
                    self.set_rp(p, v);
                } else {
                    // ADD HL,rp
                    self.internal(bus);
                    let hl = self.hl();
                    let rp = self.get_rp(p);
                    let r = hl.wrapping_add(rp);
                    self.set_flag(FLAG_N, false);
                    self.set_flag(FLAG_H, (hl & 0x0FFF) + (rp & 0x0FFF) > 0x0FFF);
                    self.set_flag(FLAG_C, (hl as u32) + (rp as u32) > 0xFFFF);
                    self.set_hl(r);
                }
            }
            2 => {
                if q == 0 {
                    let v = self.a;
                    match p {
                        0 => {
                            let a = self.bc();
                            self.write8(bus, a, v);
                        }
                        1 => {
                            let a = self.de();
                            self.write8(bus, a, v);
                        }
                        2 => {
                            let a = self.hl();
                            self.write8(bus, a, v);
                            self.set_hl(a.wrapping_add(1));
                        }
                        _ => {
                            let a = self.hl();
                            self.write8(bus, a, v);
                            self.set_hl(a.wrapping_sub(1));
                        }
                    }
                } else {
                    let v = match p {
                        0 => {
                            let a = self.bc();
                            self.read8(bus, a)
                        }
                        1 => {
                            let a = self.de();
                            self.read8(bus, a)
                        }
                        2 => {
                            let a = self.hl();
                            let r = self.read8(bus, a);
                            self.set_hl(a.wrapping_add(1));
                            r
                        }
                        _ => {
                            let a = self.hl();
                            let r = self.read8(bus, a);
                            self.set_hl(a.wrapping_sub(1));
                            r
                        }
                    };
                    self.a = v;
                }
            }
            3 => {
                self.internal(bus);
                if q == 0 {
                    self.set_rp(p, self.get_rp(p).wrapping_add(1));
                } else {
                    self.set_rp(p, self.get_rp(p).wrapping_sub(1));
                }
            }
            4 => {
                // INC r
                let v = self.read_r(bus, y);
                let r = v.wrapping_add(1);
                self.set_flag(FLAG_Z, r == 0);
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, v & 0x0F == 0x0F);
                self.write_r(bus, y, r);
            }
            5 => {
                // DEC r
                let v = self.read_r(bus, y);
                let r = v.wrapping_sub(1);
                self.set_flag(FLAG_Z, r == 0);
                self.set_flag(FLAG_N, true);
                self.set_flag(FLAG_H, v & 0x0F == 0);
                self.write_r(bus, y, r);
            }
            6 => {
                // LD r,n
                let n = self.fetch8(bus);
                self.write_r(bus, y, n);
            }
            _ => self.exec_misc_rot(y),
        }
    }

    fn exec_misc_rot(&mut self, y: u8) {
        match y {
            0 => {
                // RLCA
                let c = self.a >> 7;
                self.a = (self.a << 1) | c;
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            1 => {
                // RRCA
                let c = self.a & 1;
                self.a = (self.a >> 1) | (c << 7);
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            2 => {
                // RLA
                let old = self.flag(FLAG_C) as u8;
                let c = self.a >> 7;
                self.a = (self.a << 1) | old;
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            3 => {
                // RRA
                let old = self.flag(FLAG_C) as u8;
                let c = self.a & 1;
                self.a = (self.a >> 1) | (old << 7);
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            4 => self.daa(),
            5 => {
                // CPL
                self.a = !self.a;
                self.set_flag(FLAG_N, true);
                self.set_flag(FLAG_H, true);
            }
            6 => {
                // SCF
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, false);
                self.set_flag(FLAG_C, true);
            }
            _ => {
                // CCF
                let c = self.flag(FLAG_C);
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, false);
                self.set_flag(FLAG_C, !c);
            }
        }
    }

    fn daa(&mut self) {
        let mut a = self.a as u16;
        if !self.flag(FLAG_N) {
            if self.flag(FLAG_H) || (a & 0x0F) > 0x09 {
                a += 0x06;
            }
            if self.flag(FLAG_C) || a > 0x9F {
                a += 0x60;
                self.set_flag(FLAG_C, true);
            }
        } else {
            if self.flag(FLAG_H) {
                a = a.wrapping_sub(0x06) & 0xFF;
            }
            if self.flag(FLAG_C) {
                a = a.wrapping_sub(0x60);
            }
        }
        self.a = a as u8;
        self.set_flag(FLAG_Z, self.a == 0);
        self.set_flag(FLAG_H, false);
    }

    fn exec_x3(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) {
        match z {
            0 => match y {
                0..=3 => {
                    // RET cc
                    self.internal(bus);
                    if self.cond(y) {
                        let a = self.pop16(bus);
                        self.internal(bus);
                        self.pc = a;
                    }
                }
                4 => {
                    // LDH (a8),A
                    let n = self.fetch8(bus) as u16;
                    self.write8(bus, 0xFF00 + n, self.a);
                }
                5 => {
                    // ADD SP,e
                    let e = self.fetch8(bus) as i8 as i16 as u16;
                    let sp = self.sp;
                    self.internal(bus);
                    self.internal(bus);
                    self.set_flag(FLAG_Z, false);
                    self.set_flag(FLAG_N, false);
                    self.set_flag(FLAG_H, (sp & 0x0F) + (e & 0x0F) > 0x0F);
                    self.set_flag(FLAG_C, (sp & 0xFF) + (e & 0xFF) > 0xFF);
                    self.sp = sp.wrapping_add(e);
                }
                6 => {
                    // LDH A,(a8)
                    let n = self.fetch8(bus) as u16;
                    self.a = self.read8(bus, 0xFF00 + n);
                }
                _ => {
                    // LD HL,SP+e
                    let e = self.fetch8(bus) as i8 as i16 as u16;
                    let sp = self.sp;
                    self.internal(bus);
                    self.set_flag(FLAG_Z, false);
                    self.set_flag(FLAG_N, false);
                    self.set_flag(FLAG_H, (sp & 0x0F) + (e & 0x0F) > 0x0F);
                    self.set_flag(FLAG_C, (sp & 0xFF) + (e & 0xFF) > 0xFF);
                    self.set_hl(sp.wrapping_add(e));
                }
            },
            1 => {
                if q == 0 {
                    // POP rp2
                    let v = self.pop16(bus);
                    self.set_rp2(p, v);
                } else {
                    match p {
                        0 => {
                            // RET
                            let a = self.pop16(bus);
                            self.internal(bus);
                            self.pc = a;
                        }
                        1 => {
                            // RETI
                            let a = self.pop16(bus);
                            self.internal(bus);
                            self.pc = a;
                            self.ime = true;
                        }
                        2 => {
                            // JP HL
                            self.pc = self.hl();
                        }
                        _ => {
                            // LD SP,HL
                            self.internal(bus);
                            self.sp = self.hl();
                        }
                    }
                }
            }
            2 => match y {
                0..=3 => {
                    // JP cc,nn
                    let addr = self.fetch16(bus);
                    if self.cond(y) {
                        self.internal(bus);
                        self.pc = addr;
                    }
                }
                4 => {
                    // LD (C),A
                    self.write8(bus, 0xFF00 + self.c as u16, self.a);
                }
                5 => {
                    // LD (a16),A
                    let addr = self.fetch16(bus);
                    self.write8(bus, addr, self.a);
                }
                6 => {
                    // LD A,(C)
                    self.a = self.read8(bus, 0xFF00 + self.c as u16);
                }
                _ => {
                    // LD A,(a16)
                    let addr = self.fetch16(bus);
                    self.a = self.read8(bus, addr);
                }
            },
            3 => match y {
                0 => {
                    // JP nn
                    let addr = self.fetch16(bus);
                    self.internal(bus);
                    self.pc = addr;
                }
                6 => {
                    // DI
                    self.ime = false;
                    self.ei_pending = false;
                }
                7 => {
                    // EI
                    self.ei_pending = true;
                }
                _ => {} // illegal
            },
            4 => {
                if y <= 3 {
                    // CALL cc,nn
                    let addr = self.fetch16(bus);
                    if self.cond(y) {
                        self.push16(bus, self.pc);
                        self.pc = addr;
                    }
                }
            }
            5 => {
                if q == 0 {
                    // PUSH rp2
                    let v = self.get_rp2(p);
                    self.push16(bus, v);
                } else if p == 0 {
                    // CALL nn
                    let addr = self.fetch16(bus);
                    self.push16(bus, self.pc);
                    self.pc = addr;
                }
            }
            6 => {
                // ALU A,n
                let n = self.fetch8(bus);
                self.alu(y, n);
            }
            _ => {
                // RST
                self.push16(bus, self.pc);
                self.pc = (y as u16) * 8;
            }
        }
    }

    fn op_halt(&mut self, bus: &mut Bus) {
        let pending = bus.pending_interrupts();
        if self.ime {
            self.halted = true;
        } else if pending == 0 {
            self.halted = true;
        } else {
            // HALT bug: next byte fetched twice (PC fails to increment once).
            self.halt_bug = true;
        }
    }

    fn op_stop(&mut self, bus: &mut Bus) {
        let _ = self.fetch8(bus); // STOP is followed by a (usually 0x00) byte
        if bus.try_speed_switch() {
            bus.timer.set_div_counter(0);
        }
        // Non-speed-switch STOP would halt until joypad; games rarely rely on it.
    }

    // ---- ALU --------------------------------------------------------------
    fn alu(&mut self, op: u8, n: u8) {
        match op {
            0 => self.add(n, false),
            1 => self.add(n, true),
            2 => self.sub(n, false),
            3 => self.sub(n, true),
            4 => {
                self.a &= n;
                self.f = FLAG_H;
                self.set_flag(FLAG_Z, self.a == 0);
            }
            5 => {
                self.a ^= n;
                self.f = 0;
                self.set_flag(FLAG_Z, self.a == 0);
            }
            6 => {
                self.a |= n;
                self.f = 0;
                self.set_flag(FLAG_Z, self.a == 0);
            }
            _ => {
                // CP: subtract, discard result
                let a = self.a;
                self.sub(n, false);
                self.a = a;
            }
        }
    }

    fn add(&mut self, n: u8, carry: bool) {
        let c = (carry && self.flag(FLAG_C)) as u16;
        let a = self.a as u16;
        let r = a + n as u16 + c;
        self.set_flag(FLAG_Z, r as u8 == 0);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, (a & 0x0F) + (n as u16 & 0x0F) + c > 0x0F);
        self.set_flag(FLAG_C, r > 0xFF);
        self.a = r as u8;
    }

    fn sub(&mut self, n: u8, carry: bool) {
        let c = (carry && self.flag(FLAG_C)) as u16;
        let a = self.a as u16;
        let r = a.wrapping_sub(n as u16).wrapping_sub(c);
        self.set_flag(FLAG_Z, r as u8 == 0);
        self.set_flag(FLAG_N, true);
        self.set_flag(FLAG_H, (a & 0x0F) < (n as u16 & 0x0F) + c);
        self.set_flag(FLAG_C, a < n as u16 + c);
        self.a = r as u8;
    }

    // ---- CB ops -----------------------------------------------------------
    fn execute_cb(&mut self, bus: &mut Bus, op: u8) {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        match x {
            0 => {
                let v = self.read_r(bus, z);
                let r = self.cb_rot(y, v);
                self.write_r(bus, z, r);
            }
            1 => {
                // BIT y,r
                let v = self.read_r(bus, z);
                self.set_flag(FLAG_Z, v & (1 << y) == 0);
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, true);
            }
            2 => {
                // RES y,r
                let v = self.read_r(bus, z);
                self.write_r(bus, z, v & !(1 << y));
            }
            _ => {
                // SET y,r
                let v = self.read_r(bus, z);
                self.write_r(bus, z, v | (1 << y));
            }
        }
    }

    fn cb_rot(&mut self, op: u8, v: u8) -> u8 {
        let r;
        match op {
            0 => {
                // RLC
                let c = v >> 7;
                r = (v << 1) | c;
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            1 => {
                // RRC
                let c = v & 1;
                r = (v >> 1) | (c << 7);
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            2 => {
                // RL
                let old = self.flag(FLAG_C) as u8;
                let c = v >> 7;
                r = (v << 1) | old;
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            3 => {
                // RR
                let old = self.flag(FLAG_C) as u8;
                let c = v & 1;
                r = (v >> 1) | (old << 7);
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            4 => {
                // SLA
                let c = v >> 7;
                r = v << 1;
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            5 => {
                // SRA (arithmetic, keeps bit 7)
                let c = v & 1;
                r = (v >> 1) | (v & 0x80);
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
            6 => {
                // SWAP
                r = (v << 4) | (v >> 4);
                self.f = 0;
            }
            _ => {
                // SRL
                let c = v & 1;
                r = v >> 1;
                self.f = 0;
                self.set_flag(FLAG_C, c != 0);
            }
        }
        self.set_flag(FLAG_Z, r == 0);
        r
    }

    // ---- rp / rp2 helpers -------------------------------------------------
    fn get_rp(&self, p: u8) -> u16 {
        match p {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => self.sp,
        }
    }
    fn set_rp(&mut self, p: u8, v: u16) {
        match p {
            0 => self.set_bc(v),
            1 => self.set_de(v),
            2 => self.set_hl(v),
            _ => self.sp = v,
        }
    }
    fn get_rp2(&self, p: u8) -> u16 {
        match p {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => self.af(),
        }
    }
    fn set_rp2(&mut self, p: u8, v: u16) {
        match p {
            0 => self.set_bc(v),
            1 => self.set_de(v),
            2 => self.set_hl(v),
            _ => self.set_af(v),
        }
    }
}
