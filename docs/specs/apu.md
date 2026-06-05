# APU

I have the canonical gbdev wiki (Nightshade/blargg-derived) values confirming the divisor table and reload formulas. I now have everything needed to write the exhaustive spec.

# REVENANT — APU (Audio Processing Unit) Hardware Specification

Cycle-accurate reference for DMG / CGB. Clock domain: master clock `CLK = 4_194_304 Hz` (T-cycle). `1 M-cycle = 4 T-cycles`. APU tick lengths are **unaffected by CGB double-speed** (the APU is clocked off `DIV`, which still increments at the same wall-clock rate; double-speed only changes which `DIV` bit is used — see §1).

---

## 0. Register Map & Reset Values

| Addr | Name | Bits `7654 3210` | DMG post-boot | CGB post-boot |
|---|---|---|---|---|
| FF10 | NR10 | `-PPP NSSS` (Pace, Negate dir, Step) | `$80` | `$80` |
| FF11 | NR11 | `DDLL LLLL` (Duty, Length load) | `$BF` | `$BF` |
| FF12 | NR12 | `VVVV ADDD` (init Vol, env Add dir, env Period) | `$F3` | `$F3` |
| FF13 | NR13 | `PPPP PPPP` (Period low, **W-only**) | `$FF` | `$FF` |
| FF14 | NR14 | `TL-- -PPP` (Trigger, Length-en, Period hi) | `$BF` | `$BF` |
| FF15 | — | unused (reads `$FF`) | `$FF` | `$FF` |
| FF16 | NR21 | `DDLL LLLL` | `$3F` | `$3F` |
| FF17 | NR22 | `VVVV ADDD` | `$00` | `$00` |
| FF18 | NR23 | `PPPP PPPP` (**W-only**) | `$FF` | `$FF` |
| FF19 | NR24 | `TL-- -PPP` | `$BF` | `$BF` |
| FF1A | NR30 | `E--- ----` (DAC Enable) | `$7F` | `$7F` |
| FF1B | NR31 | `LLLL LLLL` (Length load, **W-only on DMG**) | `$FF` | `$FF` |
| FF1C | NR32 | `-VV- ----` (output level / Volume code) | `$9F` | `$9F` |
| FF1D | NR33 | `PPPP PPPP` (**W-only**) | `$FF` | `$FF` |
| FF1E | NR34 | `TL-- -PPP` | `$BF` | `$BF` |
| FF1F | — | unused (`$FF`) | `$FF` | `$FF` |
| FF20 | NR41 | `--LL LLLL` (Length load, **W-only**) | `$FF` | `$FF` |
| FF21 | NR42 | `VVVV ADDD` | `$00` | `$00` |
| FF22 | NR43 | `SSSS WDDD` (clock Shift, Width, Divisor code) | `$00` | `$00` |
| FF23 | NR44 | `TL-- ----` (Trigger, Length-en) | `$BF` | `$BF` |
| FF24 | NR50 | `LVVV Rvvv` (VINl, Lvol, VINr, Rvol) | `$77` | `$77` |
| FF25 | NR51 | `4321 4321` (L pan CH4..1, R pan CH4..1) | `$F3` | `$F3` |
| FF26 | NR52 | `E--- 4321` (Enable; status CH4..1, **R-only status**) | `$F1` | `$F1` |
| FF27–FF2F | — | unused (`$FF`) | — | — |
| FF30–FF3F | Wave RAM | 16 bytes, 32×4-bit samples | unspecified | unspecified |

**Read-back masks (OR these into the raw register on CPU read):**

| Reg | OR mask | Reg | OR mask | Reg | OR mask |
|---|---|---|---|---|---|
| NR10 | `$80` | NR21 | `$3F` | NR41 | `$FF` |
| NR11 | `$3F` | NR22 | `$00` | NR42 | `$00` |
| NR12 | `$00` | NR23 | `$FF` | NR43 | `$00` |
| NR13 | `$FF` | NR24 | `$BF` | NR44 | `$BF` |
| NR14 | `$BF` | NR30 | `$7F` | NR50 | `$00` |
| NR31 | `$FF` | NR32 | `$9F` | NR51 | `$00` |
|  | | NR33 | `$FF` | NR52 | `$70` |
|  | | NR34 | `$BF` | FF15/1F/27–2F | `$FF` |

These masks make write-only / unused bits read as `1`. Blargg `dmg_sound/01-registers` verifies every one of these.

---

## 1. Frame Sequencer (DIV-APU)

The frame sequencer is **not** a free-running 512 Hz oscillator; it is a counter incremented by a **falling edge of a specific `DIV` bit**.

| Mode | `DIV` bit watched | Falling-edge rate |
|---|---|---|
| Single speed | bit **4** (`mask $10`) | 16384 → /32 → **512 Hz** |
| CGB double speed | bit **5** (`mask $20`) | 32768 → /64 → **512 Hz** |

The watched bit is the *internal 16-bit DIV counter* bit, not the visible FF04 (which is the high 8 bits). On a `1→0` transition of that bit the DIV-APU step counter increments (mod 8). **Writing any value to FF04 resets the internal DIV counter to 0**; if the watched bit was `1` at that moment, the clear forces a falling edge → an extra DIV-APU tick (test ROMs exploit this to single-step the sequencer).

### 8-step pattern

| Step | Length (256 Hz) | Envelope (64 Hz) | CH1 Sweep (128 Hz) |
|---|:---:|:---:|:---:|
| 0 | ✔ | | |
| 1 | | | |
| 2 | ✔ | | ✔ |
| 3 | | | |
| 4 | ✔ | | |
| 5 | | | |
| 6 | ✔ | | ✔ |
| 7 | | ✔ | |

Aggregate rates: Length = 256 Hz (steps 0,2,4,6), Sweep = 128 Hz (steps 2,6), Envelope = 64 Hz (step 7).

**Power-on init:** On APU enable, the DIV-APU step counter resets such that the **first** length-clocking step will be step 0. (Behaviorally: the sequencer starts at a state where the *next* step is 7 or 0 depending on model; what matters for tests is the parity used by the "extra length clock" quirk — §6.) Treat the step counter as `0..7`; "next step clocks length" iff next step ∈ {0,2,4,6}.

---

## 2. Per-Channel Frequency / Period Timers (T-cycles)

All period dividers are **up-counters that reload on overflow** (clock when already at `$7FF` reloads from NRx3/NRx4). Equivalent down-counter "period" formulas given below; both implementations are valid.

| Channel | Reload period (T-cycles) | Clock rate | On reload |
|---|---|---|---|
| CH1 / CH2 (pulse) | `(2048 − period) × 4` | 1 048 576 Hz max | advance duty step (0..7) |
| CH3 (wave) | `(2048 − period) × 2` | 2 097 152 Hz max | advance sample index (0..31), read nibble |
| CH4 (noise) | `divisor × 2^shift` (T-cycles, see §5) | 262 144 / (div·2^shift) Hz | clock LFSR |

`period` = 11-bit value from NRx3 (low 8) + NRx4 bits 0–2 (high 3).

- Pulse sample rate = `1048576 / (2048−period)` Hz; tone = that `/8`.
- Wave sample rate = `2097152 / (2048−period)` Hz; tone = that `/32`.
- **Pulse period-write delay:** writes to NR13/NR14 period take effect only after the *current* sample completes (the divider keeps the old reload until next overflow).
- **Wave period-write delay:** takes effect on the *next* wave-RAM read.
- **Trigger period-timer quirk:** On CH1/CH2 trigger, the low **2 bits** of the frequency timer are **NOT** reloaded (preserved). Used by SameSuite `channel_1_freq_change` / Blargg trigger timing.

---

## 3. Channel 1 — Square + Sweep + Envelope + Length + Duty

### Registers
- **NR10** `-PPP NSSS`: Pace (bits 6–4), Negate/Direction (bit 3: `0`=add/increase freq, `1`=subtract), Step/shift (bits 2–0).
- **NR11** `DDLL LLLL`: Duty (7–6), initial length (5–0, write-only).
- **NR12** `VVVV ADDD`: init volume (7–4), env direction (bit 3: `1`=increase), env period (2–0).
- **NR13/NR14**: 11-bit period; NR14 bit 7 Trigger, bit 6 Length-enable.

### Duty table (8-step, MSB-first index = duty step)
| Code | % | Pattern (steps 0→7) |
|---|---|---|
| `00` | 12.5 | `0 0 0 0 0 0 0 1` |
| `01` | 25 | `1 0 0 0 0 0 0 1` |
| `10` | 50 | `1 0 0 0 0 1 1 1` |
| `11` | 75 | `0 1 1 1 1 1 1 0` |
Duty step counter **only resets on APU power-off**, never on trigger of the duty *phase* — but the duty *timer* (frequency divider) reloads on trigger, so frequent retrigger can stall the duty step. First output after power-on is always digital 0.

### Sweep unit (clocked at 128 Hz, steps 2 & 6)
Internal state: `shadow` (11-bit), `sweep_timer`, `sweep_enabled`, `negate_used` (latch).

**On trigger:**
1. `shadow ← NR13/NR14 period`.
2. `sweep_timer ← pace` (if pace==0, treat as **8**).
3. `sweep_enabled ← (pace ≠ 0) OR (step ≠ 0)`.
4. `negate_used ← false`.
5. **If step ≠ 0:** run frequency-calc + overflow check **immediately** (may disable channel). This calc uses negate mode and sets `negate_used` if direction was subtract.

**Frequency calc:** `new = shadow + ( (shadow >> step) × (negate ? −1 : +1) )` (computed in ≥12-bit width). **Overflow check:** if `new > $7FF` → **disable CH1** (NR52 CH1 bit cleared).

**On each 128 Hz sweep clock:**
- Decrement `sweep_timer`; if it reaches 0:
  - Reload `sweep_timer ← pace` (0→8).
  - If `sweep_enabled AND pace ≠ 0`:
    - Compute `new`; overflow-check (disable on overflow).
    - If `new ≤ $7FF` **AND step ≠ 0`: write `new` back to `shadow` **and** to NR13/NR14, then **run calc + overflow check a second time** (this second `new` is discarded, but can still disable on overflow).

### Sweep quirks (Blargg / mooneye / SameSuite check)
- **Overflow check happens even when pace==0** if step≠0 at trigger.
- **Negate-clear lockout:** if at least one calc has been done in **subtract** mode since the last trigger (`negate_used`), then writing NR10 to clear the direction bit (subtract→add) **immediately disables CH1**. Prevents lowering then raising frequency without a retrigger. (mooneye `sweep_details`, Blargg `04-sweep`.)
- Modifying NR13/NR14 directly while sweep active does **not** update `shadow`; next sweep clock overwrites the change.
- Sweep can never underflow → decreasing sweep never disables the channel via underflow.
- If period reaches 0 in shadow, sweep can no longer change it.

---

## 4. Channel 2 — Square + Envelope + Length + Duty

Identical to CH1 **minus the sweep unit**. Registers NR21/NR22/NR23/NR24 = NR11/NR12/NR13/NR14 semantics. NR20 (FF15) does not exist (reads `$FF`). Trigger side-effects identical except no sweep step.

---

## 5. Channel 4 — Noise (LFSR)

### Registers
- **NR41** `--LL LLLL`: 6-bit length load (write-only).
- **NR42** `VVVV ADDD`: envelope (same layout as NR12).
- **NR43** `SSSS WDDD`: clock Shift `S` (7–4), Width `W` (bit 3: `0`=15-bit, `1`=7-bit), Divisor code `D` (2–0).
- **NR44** `TL-- ----`: Trigger (7), Length-enable (6).

### Divisor table
| Code D | Divisor (T-cycles base) |
|---|---|
| 0 | 8 |
| 1 | 16 |
| 2 | 32 |
| 3 | 48 |
| 4 | 64 |
| 5 | 80 |
| 6 | 96 |
| 7 | 112 |

(Code 0 → 0.5×16; i.e. divisor=8.) **Frequency timer period = `divisor << shift` T-cycles.** LFSR clock = `262144 / (divisor × 2^shift) Hz`.

### LFSR (15-bit + 1 feedback bit = 16-bit register)
On each noise clock:
1. `b = (LFSR bit0) XNOR (LFSR bit1)` → write `b` into bit **15**.
2. **If 7-bit mode (W=1):** also copy `b` into bit **7**.
3. Shift entire LFSR right by 1.
4. Output = `(LFSR bit0 == 0) ? 0 : volume`. (Bit shifted out selects 0 vs. current envelope volume.)

LFSR `← 0` on trigger. (Some references seed all-1s after the first shift; functionally the reset value is 0 before first clock.)

### Noise quirks
- **Clock shift 14 or 15 → LFSR receives no clocks** (channel frozen). Blargg/SameSuite check this.
- **Lockup:** switching 15→7-bit mode while the low 7 bits are all `1` freezes output (only 1s generated). Cleared by retrigger.

---

## 6. Length Counters & the Extra-Clock Quirk

| Channel | Counter width | Load formula | Reload-on-zero (trigger) |
|---|---|---|---|
| CH1/CH2/CH4 | 6-bit (max 64) | `64 − (NRx1 & 0x3F)` | reloads to 64 |
| CH3 | 8-bit (max 256) | `256 − NR31` | reloads to 256 |

Internally the counter is loaded with `(max − data)` and **counts down to 0**; reaching 0 with length-enable set **disables the channel**. Length unit is clocked at 256 Hz (DIV-APU steps 0,2,4,6).

**On trigger:** if length counter == 0, it is reloaded to max (64 / 256). If length-enable is set, the reload then applies normally.

### Extra length clock quirk (Blargg `03-trigger`, mooneye `length_ctr` family)
When **NRx4 is written** while the **next** DIV-APU step is one that does **NOT** clock length (i.e. an odd half), there is an off-by-one interaction:

1. **Length-enable rising edge:** if length-enable goes `0→1` while in the "first half" (next step does not clock length) **and** the counter is non-zero, the counter is **decremented once** immediately. If that decrement hits 0 **and the trigger bit is clear**, the channel is **disabled**.
   - **CGB-02 variant:** the counter only needs to have been *previously disabled*; current enable state is ignored (breaks *Prehistorik Man*). Fixed on CGB-04/CGB-05.
2. **Trigger + length-reload collision:** if a channel is triggered in this "first half" with length-enable now set and length was just reloaded from 0 to max (64/256), it is loaded with **max−1** (63 / 255) instead.

Implementation: track `frame_seq_step`; "first half" = the next length step won't fire = current step ∈ {odd, or even where length already fired}. Concretely, gate on whether `(step & 1) == 0` after the most recent step. The two rules above must be applied in NRx4 write order: handle length-enable edge, then trigger.

---

## 7. Volume Envelope ("zombie" mode)

State: `volume` (0–15), `env_timer`, `env_period` (NRx2 bits 2–0), `env_dir` (bit 3), `env_running`.

**On trigger:** `volume ← NRx2 init volume`; `env_timer ← env_period` (0→**8**); `env_running ← true`.

**On 64 Hz envelope clock (step 7):** if `env_period ≠ 0`: decrement `env_timer`; on 0, reload `env_period` (0→8); if `env_running`:
- increase mode: if `volume < 15` → `volume++`; else stop running.
- decrease mode: if `volume > 0` → `volume--`; else stop running.

**Trigger-on-envelope-step quirk:** if a channel is triggered on a DIV-APU step that *will* clock the envelope (i.e. when the next step is 7), the envelope timer is loaded with **`period + 1`**.

### Zombie / manual volume (writing NRx2 while channel active)
Most consistent on CGB-02/CGB-04:
- If old env period was 0 **and** env still auto-updating → `volume += 1`; else if env was in decrease mode → `volume += 2`.
- If env direction bit toggled (inc↔dec) → `volume = 16 − volume`.
- Keep only low 4 bits afterward.

DMG behavior is erratic; the only portable contract: write `$08` to NRx2 (increase, period 0) to bump volume by 1. Setting NRx2 bits 7–3 to all-0 (vol 0, decrease) turns the **DAC** off → disables channel.

---

## 8. Channel 3 — Wave

### Registers
- **NR30** `E--- ----`: bit 7 = DAC enable (CH3's DAC is **directly** controlled here, not via NR32).
- **NR31** `LLLL LLLL`: 8-bit length load.
- **NR32** `-VV- ----`: output level / volume code.
- **NR33/NR34**: 11-bit period; NR34 bit 7 Trigger, bit 6 Length-enable.

### Volume code (NR32 bits 6–5) — a **digital right-shift**, not analog scaling
| Code | Level | Right-shift applied to 4-bit sample |
|---|---|---|
| `00` | Mute | output forced 0 (`>>4`) |
| `01` | 100% | `>>0` |
| `10` | 50% | `>>1` |
| `11` | 25% | `>>2` |

Changing mid-playback biases digital values toward 0 → biases analog toward "1"; HPF smooths.

### Wave RAM (FF30–FF3F)
32 samples, 4 bits, read **upper-nibble first** per byte. Sample index 0..31; index increments at sample rate, each step reads its nibble into the **sample buffer** and that buffer is what's emitted continuously.

**Access while CH3 active:**
| Model | Read | Write |
|---|---|---|
| DMG (monochrome) | `$FF` unless CPU access coincides exactly with CH3's read cycle | ignored unless same cycle |
| CGB (non-AGB) | returns the byte CH3 is *currently* reading (address ignored — aliases to the active byte) | writes go to the currently-read byte |
| AGB | reads `$FF`, writes ignored | ignored |

Wave RAM is accessible normally when DAC on but channel **not active**. Wave RAM survives APU power-off.

### Wave trigger quirks
- **Index reset, buffer NOT refilled:** trigger resets sample index but the buffer keeps the last sample; the *last sample ever read* is re-emitted until the next read. First sample actually read after trigger is **index 1** (lower nibble of FF30) — index 0 is skipped.
- **Buffer cleared only on APU power-on** → CH3 emits digital 0 on first power-up.
- **DMG wave-RAM corruption on retrigger:** triggering CH3 on DMG while it is about to read a byte corrupts the first 4 bytes of wave RAM:
  - If the read was within bytes 0–3: byte 0 is overwritten with the byte being read.
  - If within bytes 4–15: bytes 0–3 are overwritten with the aligned 4-byte block being read (4–7, 8–11, or 12–15). E.g. reading byte 9 → bytes 0–3 ← bytes 8–11.
  - Avoid by writing `$00` then `$80` to NR30 before retrigger. (*Duck Tales* hits this.)

---

## 9. DACs, Muting, Channel Enable

**DAC enable condition:**
- CH1/CH2/CH4: `(NRx2 & $F8) ≠ 0` (any of init-volume bits or direction bit set).
- CH3: `NR30 bit 7`.

**DAC off ⇒ channel forced off** (NR52 status bit cleared). DAC off fades analog toward 0 (= digital 7.5), model-dependent fade. Envelope reaching volume 0 does **NOT** turn the channel off (DAC stays on because NRx2 bits remain non-zero). DAC: maps digital `0..15` → analog `+1 .. −1` (negative slope: digital 0 → analog +1).

**Channel enable rules:**
- Set by trigger (NRx4 bit 7) **only if its DAC is on**; trigger with DAC off does nothing.
- Cleared by: DAC turning off, length expiry (if length-enabled), or (CH1) sweep overflow.
- NR52 low 4 bits report **channel** (generator) status, not DAC status — **read-only**; writes ignored.

---

## 10. NR52 / NR50 / NR51 — Global

### NR52 (FF26) `E--- 4321`
- Bit 7 (R/W): APU master enable.
- Bits 3–0 (R-only): CH4/CH3/CH2/CH1 active flags.
- Bits 6–4 read as `1` (OR mask `$70`).

**Powering APU OFF (write bit7=0):**
- All registers NR10–NR51 cleared to `$00` and become **read-only** (writes ignored) until power-on. NR52 itself stays writable (only bit 7 acts).
- All channels disabled; status bits → 0.
- **Wave RAM is NOT cleared** and remains read/writable.
- **DIV-APU counter is NOT reset.**
- **DMG only:** length counters (NRx1 load values) are also cleared / and on DMG length registers (NRx1, NR31, NR41) **remain writable while powered off** for length loads — this is the footnoted DMG exception. On CGB all NRx1 writes while off are ignored.

**Powering APU ON:** frame sequencer step reset; pulse duty steps reset to 0; wave buffer cleared (CH3 outputs 0). Does not auto-trigger any channel.

### NR50 (FF24) `LVVV Rvvv`
- Bit 7 VIN-left, bits 6–4 Left master volume (0–7), bit 3 VIN-right, bits 2–0 Right master volume (0–7).
- Volume `n` scales by `n+1` (range 1..8). **Never mutes** a non-silent input (0 → ×1, quiet but not silent).

### NR51 (FF25) `4321 4321`
- Bits 7–4: CH4..CH1 → **Left**. Bits 3–0: CH4..CH1 → **Right**. `1` routes channel to that output.
- Toggling a routed channel whose DAC is on causes an audio pop (DC-offset change).

---

## 11. Mixer / HPF (informative for output)

Per ear: sum the (up to 4) panned analog channel outputs (range −4..+4), scale by `(NRx vol)+1` from NR50 (÷ as needed), then high-pass filter:
```
out = dacs_enabled ? (in − capacitor) : 0
capacitor = in − out × CHARGE        // DMG: 0.999958, MGB/CGB: 0.998943 (per T-cycle)
charge_at_rate = CHARGE ^ (4194304 / sample_rate)   // e.g. ≈0.996 @44100 Hz (DMG)
```
When **all 4 DACs are off**, master-volume units disconnect and output is hard 0. HPF aggressiveness: GBA > CGB > DMG.

**CGB-only PCM registers** (read generator outputs, for testing):
- **PCM12 (FF76)** `HHHH LLLL`: low nibble = CH1 digital out, high nibble = CH2.
- **PCM34 (FF77)**: low = CH3, high = CH4.
Read `$00` on DMG.

---

## 12. Trigger Event — Full Side-Effect Checklist (per channel)

On write to NRx4/NR44 with bit 7 set:

| Step | CH1 | CH2 | CH3 | CH4 |
|---|---|---|---|---|
| Enable channel (if DAC on) | ✔ | ✔ | ✔ | ✔ |
| Length: if 0, reload to max | ✔ | ✔ | ✔ | ✔ |
| Reload frequency timer (CH1/2: keep low 2 bits) | ✔ | ✔ | ✔ (period→divider) | n/a (reload noise period) |
| Envelope timer reset (period→8; +1 if next step=7) | ✔ | ✔ | — | ✔ |
| Volume ← NRx2 init | ✔ | ✔ | — | ✔ |
| Wave: index reset, buffer NOT refilled | — | — | ✔ | — |
| LFSR ← 0 | — | — | — | ✔ |
| Sweep: copy shadow, reset timer, set enabled, immediate calc/overflow if step≠0 | ✔ | — | — | — |
| Apply length extra-clock / 63-255 collision quirk (§6) | ✔ | ✔ | ✔ | ✔ |
| If DAC off → trigger is a no-op (channel stays disabled) | ✔ | ✔ | ✔ | ✔ |

---

## 13. Test-ROM Coverage Map

| Test | Asserts |
|---|---|
| Blargg `01-registers` | read-back OR masks (§0), write-only bits |
| Blargg `02-len ctr` | length load values, 256 Hz clocking, disable on expiry |
| Blargg `03-trigger` | extra length clock, 63/255 collision, reload-on-trigger |
| Blargg `04-sweep` | overflow disable, pace=0 + step≠0 overflow, negate-clear lockout |
| Blargg `05-sweep details` | second calc write-back, shadow vs NR13/14 |
| Blargg `06-overflow on trigger` | immediate sweep overflow at trigger |
| Blargg `07-len sweep period sync` | sequencer phase vs length/sweep alignment |
| Blargg `08-len ctr during power` | DMG length writes while APU off; power-off clears regs |
| Blargg `09-wave read while on` | DMG same-cycle wave access, `$FF` otherwise |
| Blargg `10-wave trigger while on` | DMG wave-RAM corruption pattern (§8) |
| Blargg `11-regs after power` | post-power-off register values & read-only behavior |
| Blargg `12-wave` | wave sample read order, index-1-first |
| SameSuite `channel_1_freq_change*` | period-write delay, low-2-bit preservation |
| SameSuite `channel_3_first_sample` | first read = index 1 |
| SameSuite `channel_3_delay` | last-sample re-emit after trigger |
| SameSuite `channel_3_wave_ram_locked_write` | CGB aliasing vs DMG/AGB lock |
| SameSuite `channel_4_lfsr*` | LFSR feedback, 7-bit copy-to-bit7, shift-14/15 freeze |
| mooneye `sweep_details` | negate-mode lockout, calc timing |
| mealybug | (PPU; not APU) |

---

## Sources
- [Pan Docs — Audio](https://gbdev.io/pandocs/Audio.html), [Audio Details](https://gbdev.io/pandocs/Audio_details.html), [Audio Registers](https://gbdev.io/pandocs/Audio_Registers.html)
- [gbdev wiki — Game Boy sound hardware](https://gbdev.gg8.se/wiki/articles/Gameboy_sound_hardware) (Nightshade/blargg-derived divisor & timer formulas)
- Cross-references: Blargg `dmg_sound`/`cgb_sound`, LIJI32 SameSuite APU tests, mooneye-gb acceptance tests.

Scraped reference markdown saved at: `C:\Users\ChadE\Tester Bridgeming Goat\.firecrawl\audio_details.md`, `audio_registers.md`, `audio_overview.md`, `gbdev_wiki_sound.md`.