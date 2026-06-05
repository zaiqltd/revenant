//! Run a ROM for N frames and write the framebuffer to a PNG.
//!   cargo run -q --release --example screenshot -- rom.gb out.png [frames]

use revenant_core::{GameBoy, SCREEN_H, SCREEN_W};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let out = &args[2];
    let frames: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(150);

    let rom = std::fs::read(path).expect("rom");
    let mut gb = GameBoy::new(rom, None, 48000);
    for _ in 0..frames {
        gb.run_frame();
    }
    let png = encode_png(gb.framebuffer(), SCREEN_W as u32, SCREEN_H as u32);
    std::fs::write(out, png).expect("write png");
    eprintln!("wrote {} ({}x{}, {} frames)", out, SCREEN_W, SCREEN_H, frames);
}

fn encode_png(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity((h * (w * 4 + 1)) as usize);
    for y in 0..h {
        raw.push(0); // no filter
        let row = &rgba[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
        raw.extend_from_slice(row);
    }
    let zlib = zlib_store(&raw);

    let mut png = Vec::new();
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    // IHDR
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA
    write_chunk(&mut png, b"IHDR", &ihdr);
    write_chunk(&mut png, b"IDAT", &zlib);
    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc = Crc::new();
    crc.update(tag);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut i = 0;
    while i < data.len() {
        let chunk = (data.len() - i).min(0xFFFF);
        let last = i + chunk >= data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(chunk as u16).to_le_bytes());
        out.extend_from_slice(&(!(chunk as u16)).to_le_bytes());
        out.extend_from_slice(&data[i..i + chunk]);
        i += chunk;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

struct Crc {
    val: u32,
}
impl Crc {
    fn new() -> Crc {
        Crc { val: 0xFFFFFFFF }
    }
    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.val ^= byte as u32;
            for _ in 0..8 {
                if self.val & 1 != 0 {
                    self.val = (self.val >> 1) ^ 0xEDB88320;
                } else {
                    self.val >>= 1;
                }
            }
        }
    }
    fn finish(self) -> u32 {
        self.val ^ 0xFFFFFFFF
    }
}
