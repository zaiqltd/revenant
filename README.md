# REVENANT

**A from-scratch, gate-level-accurate Game Boy / Game Boy Color emulator — Rust → WebAssembly — with a playable in-browser arcade.**

Every chip is written by hand, no emulation libraries: a T-cycle-stepped SM83 CPU, a true **pixel-FIFO PPU**, a 4-channel APU, the cartridge mappers (MBC1/2/3/5 + a real-time clock), and full Game Boy Color support (double-speed, HDMA, palettes). The goal isn’t “it runs Pokémon” — it’s **sub-instruction accuracy**: emulating the machine on its 4.194304 MHz clock faithfully enough to pass the console’s own hardware torture-test ROMs.

> **acid2 renders pixel-perfect** (0 / 23040 pixels different from real hardware, on both DMG and CGB) and the suite currently passes **130 / 279** of the canonical accuracy gates. The full scoreboard lives in [`SCORECARD.md`](SCORECARD.md).

<p align="center"><em>dmg-acid2 &nbsp;·&nbsp; cgb-acid2 — REVENANT’s own framebuffer, bit-for-bit identical to reference hardware</em></p>

---

## Play it

```bash
rustup target add wasm32-unknown-unknown   # one-time
./web/build.sh                              # builds the wasm core + the bundled games
cd web && python -m http.server 8080        # then open http://localhost:8080
```

Pick a game from the catalog, or drop in any `.gb` / `.gbc` ROM of your own.

- **D-Pad** arrow keys · **A** `X` · **B** `Z` · **Start** `Enter` · **Select** `Shift` · **Esc** back to catalog
- On a phone, on-screen touch controls appear automatically.
- Battery saves persist in your browser, per cartridge.
- A live **CPU / PPU debugger** panel ticks alongside the game.

### Bundled games (original homebrew, hand-assembled SM83)

| Game | What it is | Controls |
|------|------------|----------|
| **Snake** | Eat, grow, don’t crash. Walls + self-collision, score, speed-ramp. | D-Pad |
| **Breakout** | Bounce the ball, clear the bricks. | ← → paddle |
| **Dodge** | Weave between falling blocks; survive. | ← → move |
| **Hello** | A movable smiley — the first thing this emulator drew. | Arrows |

These are built from `core/examples/make<game>.rs` on top of a tiny shared assembler (`core/examples/common/asm.rs`). They contain no copyrighted content.

---

## The accuracy ladder

Progress is measured, not vibed — by named test ROMs that either pass or don’t (run them yourself with the headless harness below).

| Tier | Gate | Status |
|------|------|--------|
| 1 — CPU | Blargg `cpu_instrs`, `instr_timing`, `mem_timing(+2)`, `halt_bug`, `interrupt_time` | ✅ (only `oam_bug` remains) |
| 2 — PPU image | `dmg-acid2`, `cgb-acid2`, `cgb-acid-hell`, scribbltests | ✅ **pixel-perfect** |
| 4 — timing | Mooneye `acceptance` 56/75 · `emulator-only` (all MBC) **28/28** | climbing |
| 3 / S — sound, mealybug | APU sub-cycle + FIFO latch precision | in progress |

The PPU is a genuine **pixel FIFO** (fetcher → BG/sprite FIFOs → per-dot register latching), the architecture required for the hardest demoscene and hardware-quirk tests.

### The proof harness

```bash
cargo build --release --example scorecard
./target/release/examples/scorecard score roms/gbtr out   # writes out/scorecard.json + SCORECARD.md
# or one ROM:
./target/release/examples/scorecard run <rom> <serial|mooneye|screen|image> <maxframes>
```

It runs each test ROM in an isolated subprocess (a panic aborts only that child) and classifies it three ways: Blargg serial **and** on-screen tilemap text, the Mooneye `LD B,B` + Fibonacci-register protocol, and palette-agnostic image-diff for acid2 / mealybug.

---

## Architecture

```
core/        revenant-core — the emulator, no_std-friendly, zero deps
  src/cpu.rs       SM83, M-cycle accurate (every bus access ticks the machine)
  src/ppu.rs       pixel-FIFO PPU
  src/apu.rs       4-channel audio
  src/bus.rs       MMU, OAM-DMA bus-conflict model, CGB H/GDMA
  src/cartridge.rs MBC0/1/2/3/5 + MBC3 RTC (cycle-driven, deterministic)
  src/timer.rs     DIV/TIMA with exact falling-edge + reload-window timing
  examples/        the proof harness + the homebrew game generators
wasm/        a flat C-ABI over the core (no wasm-bindgen) — the whole module needs zero imports
web/         the browser front-end: catalog, player, live debugger, touch controls
docs/specs/  hardware specifications the implementation is written against
```

**Determinism guarantee:** identical input → byte-identical frame hashes (no wall-clock, no RNG in the core). This is the foundation for instruction-level rewind and lockstep netplay.

### Build from source

```bash
cargo test                                              # core
cargo build -p revenant-wasm --release --target wasm32-unknown-unknown
cargo run --release --example screenshot -- <rom> out.png 60   # render a frame
```

---

## Roadmap

Done: alive → correct CPU → pixel-perfect picture → most of the deep-timing suite → a real pixel-FIFO PPU → an in-browser arcade.

Next: mealybug per-register latch precision · sub-cycle APU · instruction-level rewind · a dynarec JIT · WebRTC rollback netplay (the wasm already exposes the link-cable primitives) · a WebGPU LCD shader + BLEP AudioWorklet.

## License

MIT — see [`LICENSE`](LICENSE). Test ROMs under `roms/` belong to their respective authors and are not part of this repository’s license.
