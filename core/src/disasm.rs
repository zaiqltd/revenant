//! Minimal SM83 disassembler for the live debugger. Given a byte-reader and an
//! address, returns the mnemonic and the instruction length in bytes.

pub fn disassemble<F: Fn(u16) -> u8>(read: &F, addr: u16) -> (String, u8) {
    let b0 = read(addr);
    let b1 = read(addr.wrapping_add(1));
    let b2 = read(addr.wrapping_add(2));
    let nn = ((b2 as u16) << 8) | b1 as u16;
    let e = b1 as i8;

    if b0 == 0xCB {
        return (disasm_cb(b1), 2);
    }

    let r = |i: u8| ["B", "C", "D", "E", "H", "L", "(HL)", "A"][i as usize];
    let rp = |i: u8| ["BC", "DE", "HL", "SP"][i as usize];
    let rp2 = |i: u8| ["BC", "DE", "HL", "AF"][i as usize];
    let cc = |i: u8| ["NZ", "Z", "NC", "C"][i as usize];
    let alu = |i: u8| {
        ["ADD A,", "ADC A,", "SUB ", "SBC A,", "AND ", "XOR ", "OR ", "CP "][i as usize]
    };

    let x = b0 >> 6;
    let y = (b0 >> 3) & 7;
    let z = b0 & 7;
    let p = y >> 1;
    let q = y & 1;

    let (txt, len): (String, u8) = match (x, z) {
        (0, 0) => match y {
            0 => ("NOP".into(), 1),
            1 => (format!("LD (${:04X}),SP", nn), 3),
            2 => ("STOP".into(), 2),
            3 => (format!("JR ${:04X}", (addr as i32 + 2 + e as i32) as u16), 2),
            _ => (
                format!("JR {},${:04X}", cc(y - 4), (addr as i32 + 2 + e as i32) as u16),
                2,
            ),
        },
        (0, 1) => {
            if q == 0 {
                (format!("LD {},${:04X}", rp(p), nn), 3)
            } else {
                (format!("ADD HL,{}", rp(p)), 1)
            }
        }
        (0, 2) => {
            let s = match (q, p) {
                (0, 0) => "LD (BC),A",
                (0, 1) => "LD (DE),A",
                (0, 2) => "LD (HL+),A",
                (0, 3) => "LD (HL-),A",
                (1, 0) => "LD A,(BC)",
                (1, 1) => "LD A,(DE)",
                (1, 2) => "LD A,(HL+)",
                _ => "LD A,(HL-)",
            };
            (s.into(), 1)
        }
        (0, 3) => {
            if q == 0 {
                (format!("INC {}", rp(p)), 1)
            } else {
                (format!("DEC {}", rp(p)), 1)
            }
        }
        (0, 4) => (format!("INC {}", r(y)), 1),
        (0, 5) => (format!("DEC {}", r(y)), 1),
        (0, 6) => (format!("LD {},${:02X}", r(y), b1), 2),
        (0, 7) => (
            ["RLCA", "RRCA", "RLA", "RRA", "DAA", "CPL", "SCF", "CCF"][y as usize].into(),
            1,
        ),
        (1, _) => {
            if y == 6 && z == 6 {
                ("HALT".into(), 1)
            } else {
                (format!("LD {},{}", r(y), r(z)), 1)
            }
        }
        (2, _) => (format!("{}{}", alu(y), r(z)), 1),
        (3, 0) => match y {
            0..=3 => (format!("RET {}", cc(y)), 1),
            4 => (format!("LDH (${:02X}),A", b1), 2),
            5 => (format!("ADD SP,${:02X}", b1), 2),
            6 => (format!("LDH A,(${:02X})", b1), 2),
            _ => (format!("LD HL,SP+${:02X}", b1), 2),
        },
        (3, 1) => {
            if q == 0 {
                (format!("POP {}", rp2(p)), 1)
            } else {
                match p {
                    0 => ("RET".into(), 1),
                    1 => ("RETI".into(), 1),
                    2 => ("JP HL".into(), 1),
                    _ => ("LD SP,HL".into(), 1),
                }
            }
        }
        (3, 2) => match y {
            0..=3 => (format!("JP {},${:04X}", cc(y), nn), 3),
            4 => ("LD (C),A".into(), 1),
            5 => (format!("LD (${:04X}),A", nn), 3),
            6 => ("LD A,(C)".into(), 1),
            _ => (format!("LD A,(${:04X})", nn), 3),
        },
        (3, 3) => match y {
            0 => (format!("JP ${:04X}", nn), 3),
            6 => ("DI".into(), 1),
            7 => ("EI".into(), 1),
            _ => ("DB ??".into(), 1),
        },
        (3, 4) => {
            if y <= 3 {
                (format!("CALL {},${:04X}", cc(y), nn), 3)
            } else {
                ("DB ??".into(), 1)
            }
        }
        (3, 5) => {
            if q == 0 {
                (format!("PUSH {}", rp2(p)), 1)
            } else if p == 0 {
                (format!("CALL ${:04X}", nn), 3)
            } else {
                ("DB ??".into(), 1)
            }
        }
        (3, 6) => (format!("{}${:02X}", alu(y), b1), 2),
        (3, 7) => (format!("RST ${:02X}", y * 8), 1),
        _ => ("DB ??".into(), 1),
    };
    (txt, len)
}

fn disasm_cb(op: u8) -> String {
    let r = ["B", "C", "D", "E", "H", "L", "(HL)", "A"];
    let x = op >> 6;
    let y = (op >> 3) & 7;
    let z = (op & 7) as usize;
    match x {
        0 => {
            let m = ["RLC", "RRC", "RL", "RR", "SLA", "SRA", "SWAP", "SRL"][y as usize];
            format!("{} {}", m, r[z])
        }
        1 => format!("BIT {},{}", y, r[z]),
        2 => format!("RES {},{}", y, r[z]),
        _ => format!("SET {},{}", y, r[z]),
    }
}
