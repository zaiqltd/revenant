# Timer & DIV

# REVENANT Hardware Spec — Divider (DIV) & Timer (TIMA/TMA/TAC)

DMG/CGB cycle-accurate reference. All timing in T-cycles (`T`) and M-cycles (`M`); `1 M = 4 T` in normal speed. CGB double-speed (`KEY1.7=1`) halves the wall-clock period of every T-cycle but does **not** change the system-counter increment rate per T-cycle (see §9).

---

## 1. Register Map

| Addr | Name | R/W | Reset (DMG) | Reset (CGB) | Function |
|------|------|-----|-------------|-------------|----------|
| FF04 | DIV  | R/W | see §3 | see §3 | Upper 8 bits of internal 16-bit system counter. Any write resets the **whole** counter to 0x0000. |
| FF05 | TIMA | R/W | 0x00 | 0x00 | Timer counter. Increments at rate selected by TAC. On overflow, reloads from TMA + IRQ. |
| FF06 | TMA  | R/W | 0x00 | 0x00 | Timer modulo. Value loaded into TIMA on overflow. |
| FF07 | TAC  | R/W | 0xF8 (bits 0–2 = 000) | 0xF8 | Timer control. Bits 7–3 read as 1. |

---

## 2. Internal System Counter (the "DIV counter")

- A free-running **16-bit** up-counter, here named `SYSCLK` (also called `DIV-counter` / `internal divider`).
- Increments by **1 every T-cycle** (every 4 increments = 1 M-cycle... no: it increments **every T-cycle**, i.e. +4 per M-cycle). It is clocked at the full ~4.194304 MHz machine clock.
- Bit naming: `SYSCLK[15..0]`, bit 0 is the least-significant (fastest-toggling) bit.
- `DIV` (FF04) read = `SYSCLK[15..8]` (the high byte).
- Therefore DIV's bit 0 (FF04 bit 0 = `SYSCLK[8]`) toggles every 256 T-cycles, and the full DIV byte increments every 256 T-cycles → DIV-register increment rate = 16384 Hz on DMG (4194304 / 256).

```
SYSCLK:  bit  15 14 13 12 11 10  9  8 | 7  6  5  4  3  2  1  0
              \___________________/  ^   \________________/
                 DIV register (FF04) |    sub-DIV (not visible)
                                  FF04.bit0 = SYSCLK[8]
```

### 2.1 Reset / power-on value of SYSCLK
The boot ROM runs for a fixed number of cycles, so the counter has a defined post-boot value. For emulators that skip boot, the canonical post-boot DIV values are:

| Model | DIV (FF04) post-boot | SYSCLK post-boot (approx) |
|-------|----------------------|---------------------------|
| DMG   | 0xAB | 0xABCC |
| CGB (CGB mode) | 0x?? (commonly 0x1E used) | ~0x1EA0 |

(Use the DMG value 0xABCC for SYSCLK if matching Mooneye `boot_div` tests; CGB boot_div tests check model-specific values — `boot_div-dmg0`, `boot_div-dmgABCmgb`, `boot_div-cgb`, `boot_div-cgbABCDE`. Match per target model.)

---

## 3. DIV (FF04) — Write Behavior

- **Read:** returns `SYSCLK[15..8]`.
- **Write (any value):** sets `SYSCLK = 0x0000` on the write M-cycle. The written value is ignored.
- The reset is to the **entire 16-bit counter**, not just the visible byte. This is the source of multiple glitches (§6, §7).

---

## 4. TAC (FF07) — Timer Control

```
bit:  7  6  5  4  3 | 2     | 1  0
      1  1  1  1  1 | ENABLE| CLOCK SELECT
                    (read as 1)
```

| Bit | Name | Meaning |
|-----|------|---------|
| 2 | Timer Enable | 0 = TIMA frozen (does not increment). 1 = TIMA increments on selected falling edge. |
| 1–0 | Clock Select | Selects which `SYSCLK` bit feeds the falling-edge detector. |

### 4.1 Clock-select decode

| TAC[1:0] | Selected SYSCLK bit | TIMA period (T-cycles) | TIMA freq (DMG, Hz) | TIMA freq (M-cycles) |
|----------|---------------------|------------------------|----------------------|----------------------|
| 0b00 | `SYSCLK[9]`  | 1024 | 4096   | 256 M |
| 0b01 | `SYSCLK[3]`  | 16   | 262144 | 4 M   |
| 0b10 | `SYSCLK[5]`  | 64   | 65536  | 16 M  |
| 0b11 | `SYSCLK[7]`  | 256  | 16384  | 64 M  |

Note the non-monotonic ordering: `00` is the slowest, `01` is the fastest. The selected bit is the one whose **falling edge** triggers a TIMA increment.

---

## 5. The Falling-Edge Detector (core of TIMA increment)

TIMA is **not** incremented by a counter divide; it is incremented by an edge detector watching a single AND-gated signal.

### 5.1 The gated signal
```
mux_out  = SYSCLK[ select_bit(TAC[1:0]) ]
and_out  = mux_out AND TAC.enable           ; bit 2
```
A **TIMA increment** occurs on each **1 → 0 transition (falling edge)** of `and_out`.

### 5.2 Implementation model (per T-cycle)
Maintain a 1-bit `prev_and_out` register updated every T-cycle (after SYSCLK increments and after any register write within that cycle).

```
each T-cycle:
    SYSCLK = (SYSCLK + 1) & 0xFFFF          ; unless overridden by a DIV write this cycle
    bit    = (SYSCLK >> SELECT_T[TAC&3]) & 1 ; SELECT_T = {9,3,5,7}
    cur    = bit & ((TAC>>2)&1)
    if prev_and_out == 1 and cur == 0:
        tima_increment()                     ; may trigger overflow sequence
    prev_and_out = cur
```

Because the edge depends only on the gated signal, **any** event that drives `and_out` from 1 to 0 causes an increment — including writes to DIV and TAC (§6, §7). This is the key to all timer glitches.

---

## 6. TIMA Overflow / Reload Sequence (exact T-cycle timing)

When `tima_increment()` takes TIMA from `0xFF` to `0x00`, the reload + IRQ are **delayed by exactly one full M-cycle (4 T-cycles)**. During that window TIMA reads `0x00`.

### 6.1 Canonical timeline

Let cycle `A` = the M-cycle in which TIMA overflows (0xFF→0x00).

| M-cycle | TIMA value (as read) | IF.timer | Internal state |
|---------|----------------------|----------|----------------|
| A   | `0x00` | unchanged | Overflow detected; `reloading` pending. TIMA holds 0x00. |
| A+1 | `0x00` → `TMA` at the **end/edge** of this cycle | set to 1 on this cycle | TIMA is reloaded **from TMA**; timer interrupt (`IF` bit 2) is requested. |
| A+2 | `TMA` | (stays requested) | Normal operation resumes. |

Precise statement used by Mooneye tests:
- For **4 T-cycles** after the overflow, TIMA = `0x00`.
- On the M-cycle following those 4 T-cycles, TIMA = `TMA` **and** `IF.2` is set simultaneously.

```
T:      ... | A0 A1 A2 A3 | B0 B1 B2 B3 | C0 ...
TIMA:        FF→00 reads 00            reads TMA
                  \_____ 00 for 4T ____/
IF.timer:                        set here (start of B / end of A window)
```

### 6.2 Abort window — writing TIMA during the overflow delay

The reload is cancelable. There is a 1-M-cycle window where a CPU write to TIMA wins over the pending reload:

| When CPU writes TIMA | Resulting TIMA | IRQ requested? |
|----------------------|----------------|----------------|
| During cycle **A** (the 4T where TIMA=00, i.e. before reload) | The **written value** is kept; the pending reload is **canceled**. | **No** IRQ (the reload that would set IF is aborted). |
| During cycle **A+1** (the exact reload cycle) | The reload **wins**: TIMA = `TMA` (the CPU write is ignored/overwritten). | **Yes**, IRQ still requested. |
| During cycle **A+2 or later** | Normal write; TIMA = written value. | (IRQ from the overflow already happened in A+1.) |

Summary: a TIMA write *one M-cycle before* the reload aborts both the reload and the interrupt; a TIMA write *on* the reload cycle is ignored.

### 6.3 Writing TMA during the reload

- If TMA is written on cycle **A+1** (the reload cycle), TIMA is loaded with the **new** TMA value (the write and the load coincide; the new value is used).
- If TMA is written on cycle **A** (during the 00 window, before reload), TIMA at A+1 = new TMA (the value latched at reload time is the current TMA).
- Net rule the test ROM checks: **TIMA is reloaded with whatever value TMA holds at the moment of reload (cycle A+1)**, so a same-cycle TMA write is reflected in TIMA.

### 6.4 Mooneye reference timings (acceptance tests)

These are the exact behaviors `mooneye-test-suite/acceptance/timer/*` assert:

| Test | Asserts |
|------|---------|
| `tima_reload` | TIMA = 00 for one M-cycle after overflow, then = TMA; IF set on reload cycle. |
| `tima_write_reloading` | CPU TIMA write during the reload (A+1) is ignored (reload wins). |
| `tma_write_reloading` | TMA write during reload cycle (A+1) propagates into TIMA. |
| `rapid_toggle` | Toggling TAC enable rapidly produces extra increments via §7 falling edges. |
| `tim00 / tim01 / tim10 / tim11` | Each TAC clock-select increments TIMA at the exact selected SYSCLK bit period (1024/16/64/256 T). |
| `tim00_div_trigger … tim11_div_trigger` | Writing DIV at the precise SYSCLK phase forces a falling edge → extra increment (§6.2/§7). |

---

## 7. Glitches from Writing DIV / TAC (extra increments)

All of these are direct consequences of the falling-edge detector in §5 — there is **no special case**; they fall out of resetting/changing the gated input.

### 7.1 Writing DIV → possible extra TIMA increment

Writing DIV sets `SYSCLK = 0`. If the **currently selected** SYSCLK bit was `1` just before the write and the enable bit is `1`, then `and_out` goes `1→0` → **falling edge → TIMA increments**.

Condition for the glitch:
```
extra_inc_on_DIV_write =  TAC.enable
                       AND  ( SYSCLK[ SELECT_T[TAC&3] ] == 1 )   ; before the write
```
This is exactly what `tim00_div_trigger`…`tim11_div_trigger` verify: set DIV so the selected bit is high, write DIV, observe one unexpected TIMA increment.

### 7.2 Writing TAC → possible extra TIMA increment (the disable / select glitch)

When TAC is written, `and_out` is recomputed from the **new** TAC against the **current** SYSCLK. A `1→0` transition of `and_out` produces a falling edge. The hardware quirk (DMG/MGB vs CGB differ slightly; this is the DMG/Mooneye model) is computed from old vs new gate:

Let:
```
old_bit = SYSCLK[ SELECT_T[OLD_TAC & 3] ]
new_bit = SYSCLK[ SELECT_T[NEW_TAC & 3] ]
old_en  = (OLD_TAC >> 2) & 1
new_en  = (NEW_TAC >> 2) & 1
```

The DMG glitch logic (as implemented by accurate emulators / matching Mooneye `rapid_toggle`):

| Transition | Extra TIMA increment? |
|------------|------------------------|
| Enable 1→0 while `old_bit==1` | **Yes** (disabling the timer while the selected bit is high creates a falling edge). |
| Enable stays 1, clock-select changes such that `old_bit==1` and `new_bit==0` | **Yes** |
| Enable 0→1 | No (rising of gate only). |
| Otherwise | No |

Compact DMG rule:
```
glitch_inc =  (old_en AND old_bit) AND NOT(new_en AND new_bit)
```
i.e. increment iff the gated signal goes from 1 to 0 as a result of the TAC write.

> CGB note: the CGB timer uses the same falling-edge model; the observable difference in some homebrew/test notes comes from the disable-edge behavior — REVENANT should gate §7.2 on the same `glitch_inc` expression for both models unless a model-specific test (mealybug/Mooneye CGB variant) requires divergence. Keep it behind a `model` flag for future tuning.

### 7.3 Why this matters
- Software can deliberately or accidentally clock the timer faster by hammering DIV/TAC. Test ROMs (`rapid_toggle`, `*_div_trigger`) lock this down to single-edge precision.

---

## 8. Combined Per-T-Cycle State Machine (authoritative)

```
state: SYSCLK:u16, TIMA:u8, TMA:u8, TAC:u8, prev_gate:bool,
       reload_pending:enum{None, Counting(n), Reload}, IF:u8

SELECT_T = [9, 3, 5, 7]            ; index = TAC & 3

fn gate(sysclk, tac) -> bool:
    bit = (sysclk >> SELECT_T[tac & 3]) & 1
    return (bit == 1) && ((tac >> 2) & 1 == 1)

; --- ordering within one T-cycle ---
tick_T():
    1. handle pending reload progression (see below) BEFORE incrementing,
       so the 00 window is exactly 4 T-cycles.
    2. SYSCLK = (SYSCLK + 1) & 0xFFFF
    3. cur = gate(SYSCLK, TAC)
    4. if prev_gate && !cur: inc_tima()
    5. prev_gate = cur

inc_tima():
    if TIMA == 0xFF:
        TIMA = 0x00
        start 4-T reload window (overflow_pending = 4)   ; reads 00 during window
    else:
        TIMA += 1

; reload window handled at M-cycle granularity:
on the M-cycle 1 after overflow:
    TIMA = TMA
    IF |= 0x04            ; request timer interrupt
    (unless aborted by a TIMA write during the 00 window — see §6.2)
```

### 8.1 Register-write side effects (apply within the writing T/M-cycle)

| Write | Effect | Edge check |
|-------|--------|------------|
| DIV (FF04) | `SYSCLK = 0`. | Recompute `cur = gate(0, TAC)`; if `prev_gate && !cur` → inc TIMA (§7.1). |
| TAC (FF07) | Store new TAC. | Recompute `cur = gate(SYSCLK, new TAC)`; if `prev_gate && !cur` → inc TIMA (§7.2). Update `prev_gate=cur`. |
| TIMA (FF05) | If inside 00 window → cancel reload + cancel IRQ (§6.2 cycle A). If on reload cycle (A+1) → ignored. Else normal store. | — |
| TMA (FF06) | Normal store. If on reload cycle, value is used for the reload (§6.3). | — |

---

## 9. Double-Speed (CGB KEY1.7 = 1)

- In double-speed, the CPU and `SYSCLK` are clocked at ~8.388608 MHz. `SYSCLK` still increments once per (now half-length) T-cycle.
- **DIV register increments twice as fast in wall-clock time** (32768 Hz) — `boot_div` CGB-double-speed quirks and `div` behavior reflect this.
- All TAC periods (in T-cycles) are unchanged (1024/16/64/256 T), so TIMA frequencies in Hz double in double-speed.
- Practical implementation: keep everything in T-cycles; double-speed only changes how many wall-clock nanoseconds a T-cycle takes — the counter math in §8 is identical. The only model-visible difference is that `STOP`/speed-switch resets SYSCLK (a speed switch clears the divider), which `boot_div` / speed-switch tests check.

---

## 10. STOP Interaction (summary, for completeness)

- Executing `STOP` resets `SYSCLK` to 0 (divider cleared). On CGB, a speed switch (`STOP` after writing KEY1) also clears SYSCLK.
- While stopped, SYSCLK does not run → DIV frozen, no TIMA increments.
- Edge cases (STOP glitch, pending IRQ) are out of scope here but note SYSCLK=0 on resume affects the next selected-bit falling edge.

---

## 11. Implementation Checklist (test-ROM coverage)

| Behavior | Source test | §|
|----------|-------------|--|
| DIV = SYSCLK[15:8], 16384 Hz | Blargg `instr_timing` indirectly; div counting | §2 |
| Writing DIV zeroes full counter | Mooneye `div_write` | §3 |
| Each TAC select period exact | Mooneye `tim00/01/10/11` | §4.1, §5 |
| Overflow → 4T of 00 → reload + IRQ | Mooneye `tima_reload` | §6.1 |
| TIMA write aborts reload (cycle A) | Mooneye `tima_write_reloading` | §6.2 |
| TIMA write on reload ignored (A+1) | Mooneye `tima_write_reloading` | §6.2 |
| TMA write on reload used | Mooneye `tma_write_reloading` | §6.3 |
| DIV-write extra increment | Mooneye `tim0x_div_trigger` | §7.1 |
| TAC-write disable-edge increment | Mooneye `rapid_toggle` | §7.2 |
| Post-boot DIV value | Mooneye `boot_div-*` | §2.1 |
| Double-speed DIV rate | Mooneye CGB `boot_div`, div tests | §9 |

---

### Implementation notes for REVENANT (Rust)
- Represent `SYSCLK` as `u16`; never store DIV separately — always derive `div = (sysclk >> 8) as u8`.
- Tick the timer **per T-cycle** (4× per M-cycle) so falling-edge detection and the `*_div_trigger` phase tests pass; an M-cycle-granular timer will fail the sub-M-cycle edge tests.
- Keep `prev_gate: bool` and recompute it after **every** SYSCLK change and after **every** DIV/TAC write within the same cycle.
- Model the overflow as a small state machine with an explicit 4-T (1-M) delay plus the abort/override rules in §6.2–§6.3; do not collapse overflow→reload into a single instantaneous event.
- Gate §7.2 behind a `Model::{Dmg,Cgb}` flag for future mealybug/CGB-specific tuning.