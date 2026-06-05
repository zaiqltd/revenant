// Headless proof that the SHIPPED web wasm, driven through the SAME C-ABI the
// browser front-end uses, renders correctly. Instantiates revenant.wasm with no
// imports (exactly like revenant.js), loads a ROM, runs frames, and dumps the
// framebuffer to a PNG for tools/compare.py to diff against the hardware reference.
//   node web/verify_wasm.mjs web/demo.gb web/out_wasm.png [frames]
import { readFileSync, writeFileSync } from 'node:fs';

const [wasmPath, romPath, outPath, framesArg] = ['web/revenant.wasm', process.argv[2], process.argv[3], process.argv[4]];
const frames = parseInt(framesArg || '60', 10);

const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const ex = instance.exports;
const u8 = () => new Uint8Array(ex.memory.buffer);

const rom = readFileSync(romPath);
const ptr = ex.revenant_input_ptr(rom.length);
u8().set(rom, ptr);
const isCgb = ex.revenant_init(rom.length, 48000);
console.log(`loaded ${romPath} (${rom.length} bytes), cgb=${!!isCgb}`);

for (let i = 0; i < frames; i++) ex.revenant_run_frame();

const fbPtr = ex.revenant_framebuffer_ptr();
const fb = u8().slice(fbPtr, fbPtr + 160 * 144 * 4);
writeFileSync(outPath, encodePng(fb, 160, 144));
console.log(`ran ${frames} frames -> wrote ${outPath} (${160}x${144} RGBA)`);

// ---- minimal store-mode PNG encoder (mirrors core/examples/screenshot.rs) ----
function encodePng(rgba, w, h) {
  const raw = Buffer.alloc(h * (w * 4 + 1));
  for (let y = 0; y < h; y++) { raw[y * (w * 4 + 1)] = 0; rgba.copy ? rgba.copy(raw, y*(w*4+1)+1, y*w*4, (y+1)*w*4) : raw.set(rgba.subarray(y*w*4,(y+1)*w*4), y*(w*4+1)+1); }
  const idat = zlibStore(raw);
  const out = [Buffer.from([137,80,78,71,13,10,26,10])];
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w,0); ihdr.writeUInt32BE(h,4); ihdr.set([8,6,0,0,0],8);
  out.push(chunk('IHDR', ihdr), chunk('IDAT', idat), chunk('IEND', Buffer.alloc(0)));
  return Buffer.concat(out);
}
function chunk(tag, data) {
  const t = Buffer.from(tag); const len = Buffer.alloc(4); len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([t, data]); const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(body) >>> 0, 0);
  return Buffer.concat([len, body, crc]);
}
function zlibStore(data) {
  const parts = [Buffer.from([0x78, 0x01])];
  for (let i = 0; i < data.length; i += 0xffff) {
    const c = Math.min(0xffff, data.length - i); const last = i + c >= data.length;
    const hdr = Buffer.alloc(5); hdr[0] = last ? 1 : 0; hdr.writeUInt16LE(c, 1); hdr.writeUInt16LE(~c & 0xffff, 3);
    parts.push(hdr, data.subarray(i, i + c));
  }
  let a = 1, b = 0; for (const x of data) { a = (a + x) % 65521; b = (b + a) % 65521; }
  const ad = Buffer.alloc(4); ad.writeUInt32BE(((b << 16) | a) >>> 0, 0); parts.push(ad);
  return Buffer.concat(parts);
}
function crc32(buf) { let c = 0xffffffff; for (const x of buf) { c ^= x; for (let k = 0; k < 8; k++) c = (c & 1) ? (c >>> 1) ^ 0xedb88320 : c >>> 1; } return (c ^ 0xffffffff) >>> 0; }
