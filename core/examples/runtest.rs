//! Headless test-ROM runner. Loads a ROM, runs it, and watches the serial output
//! (where Blargg/Mooneye style ROMs print results). Exits 0 on "Passed".
//!
//!   cargo run -q --release --example runtest -- path/to/rom.gb [max_frames]

use revenant_core::GameBoy;
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: runtest <rom> [max_frames]");
        std::process::exit(2);
    }
    let path = &args[1];
    let max_frames: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4000);

    let rom = std::fs::read(path).expect("read rom");
    let mut gb = GameBoy::new(rom, None, 48000);
    eprintln!("title: {:?}  cgb: {}", gb.title(), gb.cgb);

    let mut last_len = 0usize;
    let mut stable_for = 0u64;
    for frame in 0..max_frames {
        gb.run_frame();
        let out = gb.serial_output();
        if out.len() != last_len {
            let s: String = out[last_len..].iter().map(|&b| b as char).collect();
            print!("{}", s);
            std::io::stdout().flush().ok();
            last_len = out.len();
            stable_for = 0;
        } else {
            stable_for += 1;
        }
        let text: String = out.iter().map(|&b| b as char).collect();
        if text.contains("Passed") {
            println!("\n[runtest] PASSED after {} frames", frame);
            std::process::exit(0);
        }
        if text.contains("Failed") || text.contains("Error") {
            println!("\n[runtest] FAILED after {} frames", frame);
            std::process::exit(1);
        }
        // Mooneye ROMs signal via registers (magic in B,C,D,E,H,L = 3,5,8,13,21,34)
        if stable_for > 600 {
            // Output stabilized — check Mooneye fibonacci signature
            let c = &gb.cpu;
            if c.b == 3 && c.c == 5 && c.d == 8 && c.e == 13 && c.h == 21 && c.l == 34 {
                println!("\n[runtest] Mooneye PASS (magic registers)");
                std::process::exit(0);
            }
            if c.b == 0x42 && c.c == 0x42 && c.d == 0x42 {
                println!("\n[runtest] Mooneye FAIL signature");
                std::process::exit(1);
            }
            break;
        }
    }
    let text: String = gb.serial_output().iter().map(|&b| b as char).collect();
    println!("\n[runtest] inconclusive. serial=<{}>", text.trim());
    std::process::exit(3);
}
