// REVENANT — browser front-end glue over the self-contained wasm core.
// No wasm-bindgen: the core exports a flat C-ABI + its linear memory, and the
// module needs no imports (it is deterministic — no time, no RNG), so wiring it
// up is just fetch -> instantiate -> drive run_frame at 59.7275 Hz.

const WIDTH = 160, HEIGHT = 144;
const FPS = 59.7275;
const FRAME_MS = 1000 / FPS;

// Keyboard -> joypad state bits [Right,Left,Up,Down,A,B,Select,Start].
const KEYMAP = {
  ArrowRight: 0, ArrowLeft: 1, ArrowUp: 2, ArrowDown: 3,
  KeyX: 4, KeyZ: 5, ShiftRight: 6, ShiftLeft: 6, Backspace: 6, Enter: 7,
  // also accept on-screen / alt keys
  KeyL: 4, KeyK: 5,
};

export class Revenant {
  constructor() {
    this.ex = null;            // wasm exports
    this.running = false;
    this.buttons = 0;          // joypad bitmask
    this.romLoaded = false;
    this.romKey = null;
    this.sampleRate = 48000;
    this.audioCtx = null;
    this.playTime = 0;
    this.acc = 0;
    this.last = 0;
    this.frames = 0;
    this.fps = 0;
    this._fpsT = 0;
    this.onframe = null;       // optional callback(debugState)
  }

  async load(url = 'revenant.wasm') {
    const bytes = await (await fetch(url)).arrayBuffer();
    const { instance } = await WebAssembly.instantiate(bytes, {});
    this.ex = instance.exports;
  }

  // Fresh views every access: a wasm memory.grow() detaches old ArrayBuffers.
  _u8() { return new Uint8Array(this.ex.memory.buffer); }
  _f32() { return new Float32Array(this.ex.memory.buffer); }

  loadRom(romBytes) {
    const ex = this.ex;
    // hand the ROM bytes to the core through its input buffer
    const ptr = ex.revenant_input_ptr(romBytes.length);
    this._u8().set(romBytes, ptr);
    const isCgb = ex.revenant_init(romBytes.length, this.sampleRate) !== 0;
    this.romLoaded = true;
    this.romKey = 'rev_save_' + hash32(romBytes);
    this.isCgb = isCgb;
    this._loadBattery();
    return isCgb;
  }

  reset() { if (this.romLoaded) this.ex.revenant_reset(); }

  setButton(bit, pressed) {
    if (pressed) this.buttons |= (1 << bit); else this.buttons &= ~(1 << bit);
    if (this.romLoaded) this.ex.revenant_set_buttons(this.buttons);
  }

  // ---- audio ------------------------------------------------------------
  initAudio() {
    if (this.audioCtx) return;
    const Ctx = window.AudioContext || window.webkitAudioContext;
    this.audioCtx = new Ctx();
    this.sampleRate = this.audioCtx.sampleRate; // match the core to the device
    this.playTime = 0;
  }

  _pumpAudio() {
    const ex = this.ex, ctx = this.audioCtx;
    if (!ctx) return;
    const len = ex.revenant_audio_len();
    if (len < 2) return;
    const ptr = ex.revenant_audio_ptr();
    const src = this._f32().subarray(ptr >> 2, (ptr >> 2) + len);
    const frames = len >> 1; // interleaved stereo
    const buf = ctx.createBuffer(2, frames, this.sampleRate);
    const l = buf.getChannelData(0), r = buf.getChannelData(1);
    for (let i = 0; i < frames; i++) { l[i] = src[2 * i]; r[i] = src[2 * i + 1]; }
    const node = ctx.createBufferSource();
    node.buffer = buf;
    node.connect(ctx.destination);
    const now = ctx.currentTime;
    if (this.playTime < now + 0.02) this.playTime = now + 0.05; // resync on underrun
    node.start(this.playTime);
    this.playTime += buf.duration;
  }

  // ---- battery saves (localStorage, base64) -----------------------------
  _loadBattery() {
    if (!this.ex.revenant_has_battery()) return;
    const s = localStorage.getItem(this.romKey);
    if (!s) return;
    const raw = b64decode(s);
    const ptr = this.ex.revenant_input_ptr(raw.length);
    this._u8().set(raw, ptr);
    this.ex.revenant_load_ram(raw.length);
  }
  _saveBattery() {
    const ex = this.ex;
    if (!ex.revenant_has_battery() || !ex.revenant_ram_dirty()) return;
    const ptr = ex.revenant_save_ram_ptr();
    const len = ex.revenant_save_ram_len();
    if (!ptr || !len) return;
    const data = this._u8().slice(ptr, ptr + len);
    localStorage.setItem(this.romKey, b64encode(data));
  }

  // ---- live debugger snapshot -------------------------------------------
  debugState() {
    const ex = this.ex;
    const ptr = ex.revenant_cpu_state_ptr();
    if (!ptr) return null;
    const dv = new DataView(this.ex.memory.buffer, ptr, 24);
    const u16 = (o) => dv.getUint16(o, true);
    return {
      AF: u16(0), BC: u16(2), DE: u16(4), HL: u16(6), SP: u16(8), PC: u16(10),
      IME: dv.getUint8(12), HALT: dv.getUint8(13), MODE: dv.getUint8(14),
      LY: dv.getUint8(15), LCDC: dv.getUint8(16), IF: dv.getUint8(17),
      IE: dv.getUint8(18), DSPEED: dv.getUint8(19), fps: this.fps,
    };
  }

  // ---- main loop --------------------------------------------------------
  start(ctxRender) {
    this.render = ctxRender;
    this.running = true;
    this.last = performance.now();
    requestAnimationFrame(this._tick.bind(this));
  }
  pause() { this.running = false; }
  resume() { if (!this.running) { this.running = true; this.last = performance.now(); requestAnimationFrame(this._tick.bind(this)); } }

  _tick(now) {
    if (!this.running) return;
    let dt = now - this.last; this.last = now;
    if (dt > 100) dt = 100;                 // avoid spiral-of-death after a stall
    this.acc += dt;
    let ran = 0;
    while (this.acc >= FRAME_MS && ran < 4) {
      this.ex.revenant_run_frame();
      this._pumpAudio();
      this.acc -= FRAME_MS; ran++; this.frames++;
    }
    if (ran > 0) {
      this._blit();
      if ((this.frames & 31) === 0) this._saveBattery();
    }
    // fps meter
    this._fpsT += dt;
    if (this._fpsT >= 500) { this.fps = Math.round(this.frames * 1000 / (this._fpsAcc || 1)) || this.fps; }
    if (now - (this._fpsMark || 0) >= 1000) { this.fps = this.frames - (this._fpsLast || 0); this._fpsLast = this.frames; this._fpsMark = now; }
    if (this.onframe) this.onframe(this.debugState());
    requestAnimationFrame(this._tick.bind(this));
  }

  _blit() {
    const ptr = this.ex.revenant_framebuffer_ptr();
    if (!ptr) return;
    const fb = this._u8().subarray(ptr, ptr + WIDTH * HEIGHT * 4);
    this.render(fb);
  }
}

// ---- helpers --------------------------------------------------------------
function hash32(bytes) {
  let h = 0x811c9dc5;
  const n = Math.min(bytes.length, 0x4000);
  for (let i = 0; i < n; i++) { h ^= bytes[i]; h = (h * 0x01000193) >>> 0; }
  return (h >>> 0).toString(16) + '_' + bytes.length;
}
function b64encode(u8) { let s = ''; for (let i = 0; i < u8.length; i++) s += String.fromCharCode(u8[i]); return btoa(s); }
function b64decode(str) { const s = atob(str); const u8 = new Uint8Array(s.length); for (let i = 0; i < s.length; i++) u8[i] = s.charCodeAt(i); return u8; }

export { WIDTH, HEIGHT, KEYMAP };
