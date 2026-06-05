// REVENANT — browser front-end glue over the self-contained wasm core.
// No wasm-bindgen: the core exports a flat C-ABI + its linear memory and needs
// no imports (it is deterministic), so this is just fetch -> instantiate ->
// drive run_frame at 59.7275 Hz, blit to a canvas, route input + audio.

export const WIDTH = 160, HEIGHT = 144;
const FPS = 59.7275;
const FRAME_MS = 1000 / FPS;

// KeyboardEvent.code -> joypad bit [Right,Left,Up,Down,A,B,Select,Start].
export const KEYMAP = {
  ArrowRight: 0, ArrowLeft: 1, ArrowUp: 2, ArrowDown: 3,
  KeyX: 4, KeyZ: 5, KeyK: 4, KeyJ: 5,
  ShiftLeft: 6, ShiftRight: 6, Backspace: 6, Enter: 7,
};

export class Revenant {
  constructor() {
    this.ex = null;
    this.running = false;
    this.buttons = 0;
    this.romLoaded = false;
    this.romKey = null;
    this.sampleRate = 48000;
    this.audioCtx = null;
    this.muted = false;
    this.playTime = 0;
    this.acc = 0;
    this.last = 0;
    this.frames = 0;
    this.fps = 0;
    this._fpsMark = 0;
    this._fpsBase = 0;
    this.render = null;     // (Uint8Array fb) => void
    this.onframe = null;    // (debugState) => void
    this._raf = 0;
  }

  async load(url = 'revenant.wasm') {
    const bytes = await (await fetch(url)).arrayBuffer();
    const { instance } = await WebAssembly.instantiate(bytes, {});
    this.ex = instance.exports;
  }

  // Fresh views every access: wasm memory.grow() detaches old ArrayBuffers.
  _u8() { return new Uint8Array(this.ex.memory.buffer); }
  _f32() { return new Float32Array(this.ex.memory.buffer); }

  loadRom(romBytes) {
    this.stop();
    const ex = this.ex;
    const ptr = ex.revenant_input_ptr(romBytes.length);
    this._u8().set(romBytes, ptr);
    this.isCgb = ex.revenant_init(romBytes.length, this.sampleRate) !== 0;
    this.romLoaded = true;
    this.romKey = 'rev_save_' + hash32(romBytes);
    this.buttons = 0;
    this._loadBattery();
    return this.isCgb;
  }

  reset() { if (this.romLoaded) this.ex.revenant_reset(); }

  setButton(bit, pressed) {
    if (pressed) this.buttons |= (1 << bit); else this.buttons &= ~(1 << bit);
    if (this.romLoaded) this.ex.revenant_set_buttons(this.buttons);
  }

  // ---- audio ----
  initAudio() {
    if (this.audioCtx) { if (this.audioCtx.state === 'suspended') this.audioCtx.resume(); return; }
    const Ctx = window.AudioContext || window.webkitAudioContext;
    if (!Ctx) return;
    this.audioCtx = new Ctx();
    this.sampleRate = this.audioCtx.sampleRate;
    this.playTime = 0;
  }
  setMuted(m) { this.muted = m; if (this.audioCtx) (m ? this.audioCtx.suspend() : this.audioCtx.resume()); }

  _pumpAudio() {
    const ctx = this.audioCtx;
    if (!ctx || this.muted || ctx.state !== 'running') return;
    try {
      const ex = this.ex;
      const len = ex.revenant_audio_len();
      if (len < 2) return;
      const ptr = ex.revenant_audio_ptr();
      const src = this._f32().subarray(ptr >> 2, (ptr >> 2) + len);
      const n = len >> 1;
      const buf = ctx.createBuffer(2, n, this.sampleRate);
      const l = buf.getChannelData(0), r = buf.getChannelData(1);
      for (let i = 0; i < n; i++) { l[i] = src[2 * i]; r[i] = src[2 * i + 1]; }
      const node = ctx.createBufferSource();
      node.buffer = buf; node.connect(ctx.destination);
      const now = ctx.currentTime;
      if (this.playTime < now + 0.02) this.playTime = now + 0.06; // resync on underrun
      node.start(this.playTime);
      this.playTime += buf.duration;
    } catch (_) { /* audio is best-effort; never break the frame loop */ }
  }

  // ---- battery saves ----
  _loadBattery() {
    if (!this.ex.revenant_has_battery()) return;
    const s = localStorage.getItem(this.romKey);
    if (!s) return;
    try {
      const raw = b64dec(s);
      const ptr = this.ex.revenant_input_ptr(raw.length);
      this._u8().set(raw, ptr);
      this.ex.revenant_load_ram(raw.length);
    } catch (_) {}
  }
  _saveBattery() {
    const ex = this.ex;
    if (!ex.revenant_has_battery() || !ex.revenant_ram_dirty()) return;
    const ptr = ex.revenant_save_ram_ptr(), len = ex.revenant_save_ram_len();
    if (!ptr || !len) return;
    try { localStorage.setItem(this.romKey, b64enc(this._u8().slice(ptr, ptr + len))); } catch (_) {}
  }

  // ---- live debugger snapshot ----
  debugState() {
    const ptr = this.ex.revenant_cpu_state_ptr();
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

  // ---- main loop ----
  start(render) {
    this.render = render;
    if (this.running) return;
    this.running = true;
    this.last = performance.now();
    this._fpsMark = this.last; this._fpsBase = this.frames;
    this._raf = requestAnimationFrame(this._tick.bind(this));
  }
  stop() { this.running = false; if (this._raf) cancelAnimationFrame(this._raf); this._raf = 0; }
  togglePause() { if (this.running) this.stop(); else this.start(this.render); return this.running; }

  _tick(now) {
    if (!this.running) return;
    let dt = now - this.last; this.last = now;
    if (dt > 100) dt = 100;
    this.acc += dt;
    let ran = 0;
    while (this.acc >= FRAME_MS && ran < 4) {
      this.ex.revenant_run_frame();
      this._pumpAudio();
      this.acc -= FRAME_MS; ran++; this.frames++;
    }
    if (ran > 0) {
      const ptr = this.ex.revenant_framebuffer_ptr();
      if (ptr && this.render) this.render(this._u8().subarray(ptr, ptr + WIDTH * HEIGHT * 4));
      if ((this.frames & 63) === 0) this._saveBattery();
    }
    if (now - this._fpsMark >= 500) {
      this.fps = Math.round((this.frames - this._fpsBase) * 1000 / (now - this._fpsMark));
      this._fpsMark = now; this._fpsBase = this.frames;
    }
    if (this.onframe) this.onframe(this.debugState());
    this._raf = requestAnimationFrame(this._tick.bind(this));
  }
}

function hash32(b) { let h = 0x811c9dc5; const n = Math.min(b.length, 0x4000); for (let i = 0; i < n; i++) { h ^= b[i]; h = (h * 0x01000193) >>> 0; } return (h >>> 0).toString(16) + '_' + b.length; }
function b64enc(u8) { let s = ''; for (let i = 0; i < u8.length; i++) s += String.fromCharCode(u8[i]); return btoa(s); }
function b64dec(str) { const s = atob(str); const u8 = new Uint8Array(s.length); for (let i = 0; i < s.length; i++) u8[i] = s.charCodeAt(i); return u8; }
