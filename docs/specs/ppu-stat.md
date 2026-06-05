# PPU registers & STAT quirks

I have enough authoritative detail. The cycle-accurate LY=153 and LYC-compare timing details are well-established from the mooneye tests, the AntonioND cycle-accurate docs, and my training knowledge corroborated by these sources. I'll now write the complete spec.

I have all the source material I need. Producing the spec now.

# REVENANT — Hardware-Accuracy Spec: PPU Registers & STAT/IRQ Quirks (DMG/CGB)

> Scope: `FF40` LCDC, `FF41` STAT, `FF42`–`FF45` SCY/SCX/LY/LYC, `FF4A`/`FF4B` WY/WX, `FF47`–`FF49` BGP/OBP0/OBP1; the STAT interrupt line; STAT blocking; DMG STAT write bug; LY/LYC compare timing; LY=153/line-0 quirk; LCD off→on first-frame timing; OAM scan selection.
> Conventions: **dot = 1 T-cycle** at Normal Speed (4.194304 MHz); **1 M-cycle = 4 T = 4 dots** (Normal) / 2 dots (CGB Double Speed — but dots are model-time, so a scanline is always 456 dots, taking 228 M-cycles in double speed). Bit 0 = LSB. `$` = hex. Unless noted, timing is given in **dots from the start of the scanline (dot 0 = first dot of Mode 2)**.

---

## 1. Frame & Scanline Geometry (reference constants)

| Quantity | Value |
|---|---|
| Dots per scanline | **456** |
| Scanlines per frame | **154** (LY 0–153) |
| Visible scanlines | 144 (LY 0–143) |
| VBlank scanlines | 10 (LY 144–153) |
| Dots per frame | **70224** (= 456 × 154) |
| Frame rate | ≈ 59.7275 Hz (DMG/CGB) |
| Mode 2 (OAM scan) | **80 dots** (fixed) |
| Mode 3 (drawing) | **172–289 dots** (variable, see §9) |
| Mode 0 (HBlank) | **376 − mode3_len** dots → 87–204 dots |
| Mode 3 + Mode 0 | always **376 dots** (Mode 2 + Mode 3 + Mode 0 = 456) |
| Mode 1 (VBlank) | 4560 dots (= 10 × 456) |

Per-scanline mode order (visible lines): **Mode 2 → Mode 3 → Mode 0**. VBlank lines (144–153) are entirely **Mode 1** (no mode 2/3/0 cycling; LY 144 enters Mode 1 at dot 0... see §6 caveat about line-144 mode-2 IRQ).

---

## 2. FF40 — LCDC: LCD Control (R/W)

Never locked by PPU; writable mid-scanline (mid-scanline writes produce mealybug-class effects). Bit layout:

| Bit | Name | 0 | 1 | Notes |
|---|---|---|---|---|
| 7 | **LCD & PPU enable** | Off | On | Off → immediate full VRAM/OAM access; resets PPU state machine. Turn off only in VBlank (HW burn-in risk otherwise). See §10. |
| 6 | **Window tile map area** | `$9800–9BFF` | `$9C00–9FFF` | Selects window's 32×32 BG map. |
| 5 | **Window enable** | Off | On | On DMG, overridden to off if bit 0 = 0. Mid-frame toggles drive window quirks (§13). |
| 4 | **BG & Window tile data area** | `$8800–97FF` (signed, base `$9000`) | `$8000–8FFF` (unsigned, base `$8000`) | OBJs **always** use `$8000` unsigned, ignoring this bit. |
| 3 | **BG tile map area** | `$9800–9BFF` | `$9C00–9FFF` | |
| 2 | **OBJ size** | 8×8 | 8×16 | In 8×16, tile index LSB forced: top = `NN & $FE`, bottom = `NN | $01`. |
| 1 | **OBJ enable** | Off | On | Mid-frame toggle: mealybug `m3_lcdc_obj_en_change`. |
| 0 | **BG & Window enable / priority** | see below | see below | **Different meaning DMG vs CGB.** |

**Tile data addressing (bit 4):**
- Bit 4 = 1 ("$8000 method"): tile# unsigned 0–255 → addr = `$8000 + tile#*16`.
- Bit 4 = 0 ("$8800 method"): tile# signed −128…127 → addr = `$9000 + (i8)tile#*16` (equivalently base `$8800` with tile# XOR `$80`).

**Bit 0 semantics:**
- **DMG / SGB / CGB-in-DMG-mode:** 0 ⇒ BG **and** Window are blank (color 0 / white); window-enable (bit 5) ignored. Objects still draw if bit 1 set.
- **CGB mode:** 0 ⇒ BG/Window lose priority; OBJs always drawn on top regardless of OAM "BG-over-OBJ" and BG-map priority bits. 1 ⇒ normal CGB priority resolution. BG/Window are still rendered (not blanked) — this is a *master priority* bit, not an enable.

**LCD enable (bit 7) edge behavior:** see §10 (off→on first-frame timing). On disable: STAT mode bits read **0**, LY reads **0** and is frozen, VRAM/OAM fully accessible.

---

## 3. FF41 — STAT: LCD Status

| Bit | Name | Access | Meaning |
|---|---|---|---|
| 7 | (unused) | R | Reads **1** (always set). |
| 6 | **LYC int select** | R/W | Enable LYC=LY as a STAT-line source. |
| 5 | **Mode 2 int select** | R/W | Enable Mode-2 (OAM) as a source. |
| 4 | **Mode 1 int select** | R/W | Enable Mode-1 (VBlank) as a source. |
| 3 | **Mode 0 int select** | R/W | Enable Mode-0 (HBlank) as a source. |
| 2 | **LYC == LY** | R | Coincidence flag; continuously updated (see §5 timing). |
| 1–0 | **PPU mode** | R | Current mode 0–3. Reads **0** when LCD disabled. |

Mode encoding (bits 1–0): `0`=HBlank, `1`=VBlank, `2`=OAM scan, `3`=Drawing.

Write semantics: bits 7 and 2–0 are read-only; only bits 6–3 are stored. On DMG the write also triggers the write bug (§7). Bit 7 always reads 1 (a `LD A,[FF41]; ...` with mode 0 + LYC≠LY yields `$80`).

---

## 4. The STAT Interrupt Line (INT $48) — single-line OR + rising-edge

There is **one** internal "STAT interrupt line." It is the logical OR of the four sources, each gated by its enable bit:

```
stat_line = (LYC_int_sel  & (LY == LYC))
          | (mode2_int_sel & (mode == 2))
          | (mode1_int_sel & (mode == 1))
          | (mode0_int_sel & (mode == 0))
```

**An IF.bit1 (STAT, INT $48) request is set ONLY on a rising edge (0→1) of `stat_line`.** A level that stays high requests nothing further.

**STAT blocking:** if a new source ORs the line high while it is *already* high (or in the same dot it would otherwise go low), there is no 0→1 transition, so **no interrupt fires.** Consequences the test ROMs check:
- Enabling both Mode 0 and Mode 1 (or Mode 2): the second-entered mode produces **no** interrupt because the line never dropped between them. E.g. Mode 0 (HBlank of line 143) → Mode 1 (VBlank line 144): only one edge.
- Mode 2 + LYC where LYC matches the line whose Mode 2 you're in: the LYC source and Mode 2 source overlap; only the first rising edge counts.

Implementation requirement: evaluate `stat_line` **every dot** (or every M-cycle with sub-cycle correctness for the edge), latch previous value, request IF.1 on `prev==0 && cur==1`. Do **not** request on writes that merely set an enable bit unless they cause a fresh 0→1 (but see DMG write bug §7, which injects a spurious all-ones level).

VBlank: note INT $40 (VBlank, IF.0) fires independently at LY=144 entry. A Mode-1-enabled STAT source *also* fires a STAT IRQ on entry to VBlank (subject to blocking).

---

## 5. LY / LYC Compare Timing

Reference: mooneye `stat_lyc_onoff`, `ly00_01_mode0_2`, `ly_lyc*`, `intr_2_*`; gbdev "Timing of LYC STAT Handlers."

Core facts:
- The comparison `LY == LYC` is performed continuously, but the **STAT bit-2 flag and the LYC line source update with a fixed phase relative to the LY increment.**
- For most lines, **LY visibly increments at dot 0 of the new line** (the 456-dot boundary). The coincidence flag (bit 2) for the new LY value becomes valid essentially at that boundary, and the LYC STAT source can produce its rising edge there.
- Writing `LYC` takes effect immediately for the comparison; if `LY` already equals the new `LYC` and the LYC source is enabled, whether an edge occurs depends on whether the line was already high.

Per-line LY timing exceptions (the quirks emulators must special-case):

| Line | LY value seen | When it changes | Notes |
|---|---|---|---|
| 0–142 | n | dot 0 of line | normal |
| 143 | 143 | dot 0 | last visible; HBlank → enters VBlank next |
| 144 | 144 | dot 0 | VBlank start; VBlank IRQ + (if enabled) Mode 1 STAT |
| 145–152 | n | dot 0 | normal VBlank |
| 153 | **153 for ~1 dot, then 0** | see §6 | the LY=153/line-0 quirk |

**LYC=0 / LY=153 interaction (critical):** Because LY reads 0 for almost all of physical line 153 (see §6), an `LYC=0` coincidence can trigger **twice per frame**: once at the real LY=0 (start of frame) and once during line 153 when LY has wrapped to 0 early. Mooneye `stat_lyc_onoff` and SameBoy/`intr_2_*` check the exact dots.

---

## 6. The LY=153 / Line-0 Quirk (single-dot LY=153)

On physical scanline 153 (the last VBlank line), LY does **not** stay 153 for the whole 456 dots. Observed DMG behavior (model-dependent in detail; below is DMG/MGB, the common target):

| Dots into line 153 | LY register reads | LYC compare uses |
|---|---|---|
| 0 | **153** (for ~1 M-cycle, ≈4 dots) | 153 |
| ~4 → 456 | **0** | 0 |

So:
- LY=153 is observable for only ~1 machine cycle at the very start of line 153; for the rest of line 153, LY reads **0**.
- The LYC=LY coincidence for `LYC=153` is therefore a very short window at the top of line 153.
- The LYC=LY coincidence for `LYC=0` becomes true during line 153 (after the wrap) **and** again at the true start of frame (line 0), giving the double-trigger.
- The STAT mode during line 153 is Mode 1 throughout (still VBlank).
- Line 0 then begins normally: LY=0, Mode 2 starts at dot 0.

CGB note: the exact sub-cycle phase of the LY 153→0 transition differs slightly between CGB revisions and DMG; emulators targeting both should make this a per-model constant. The *existence* of the early wrap is universal.

Implementation: model line 153 as "LY=153 for the first M-cycle, then LY=0 for the remainder," and run the LYC comparator against the *currently latched* LY each dot so both the brief 153 match and the early 0 match are produced.

---

## 7. DMG STAT Write Bug (spurious STAT interrupt)

Reference: Pan Docs "Spurious STAT interrupts"; affects Ocean *Road Rash*, Vic Tokai *Xerd no Densetsu*.

**Behavior (DMG / MGB / SGB only — NOT CGB, NOT CGB-in-DMG-mode):**
Any write to STAT (`FF41`), **including writing `$00`**, acts as if `$FF` were written into the enable bits for **one M-cycle**, then the actual value is written the next M-cycle.

Effect: for that one M-cycle, all four enable bits are effectively 1, so `stat_line` becomes the raw OR of all four *conditions*. If the PPU is currently in Mode 0, Mode 1, Mode 2, or LY=LYC, this momentarily forces the line high → a rising edge → a spurious STAT interrupt request (IF.1 set), even if the program intended to disable interrupts by writing `$00`.

Conditions that trigger it: writing STAT while in **HBlank, VBlank, OAM scan, or while LY==LYC.** Writing during Mode 3 with LYC≠LY does *not* (no condition is true to be ORed high).

Implementation: on a DMG-class STAT write, compute `stat_line` for **one M-cycle** using enables = `1111`, then on the following M-cycle apply the written enables and recompute. Run the rising-edge detector across both steps. On CGB, skip this entirely (write enables directly).

---

## 8. OAM Scan (Mode 2) — Sprite Selection

Mode 2 spans **dots 0–79** of each visible scanline. The PPU walks OAM `$FE00–$FE9F` (40 entries × 4 bytes) **in address order** and selects sprites for this line.

Selection rule (only the Y coordinate is tested here):
```
sprite_height = (LCDC.2 ? 16 : 8)
selected if:  (LY + 16) >= Y  AND  (LY + 16) < Y + sprite_height
   equivalently: Y <= LY+16 < Y+height
```
- Scan picks the **first up to 10** matching entries (lowest OAM index first), then stops adding (still counts toward the 10 even if off-screen by X). X is **not** consulted during selection — an X=0 or X≥168 sprite still consumes one of the 10 slots.
- An off-screen-by-Y sprite (Y=0, Y≥160; or Y≤8 in 8×8 mode) does not match and frees the slot.

Timing model: 2 dots per OAM entry × 40 = 80 dots. A common cycle-accurate model checks two entries' worth per M-cycle; the practical requirement is that the *set of selected sprites is finalized by dot 80*. CPU/DMA contention: during Mode 2, OAM is inaccessible to CPU (reads `$FF`, writes ignored) unless LCD off.

**Drawing priority** (resolved later, during Mode 3 fetch, not during selection):
- **DMG / Non-CGB:** lower X wins; tie broken by lower OAM index.
- **CGB mode:** OAM index only (lower index wins); X ignored for priority.
- "BG-over-OBJ" (OAM byte3 bit7, and in CGB the BG-map priority bit + LCDC.0): the winning object pixel is chosen *first* (ignoring BG-over-OBJ), *then* its BG-over-OBJ flag decides whether BG color 1–3 covers it. A high-priority BG-over-OBJ sprite masks lower-priority sprites beneath it.

---

## 9. Mode 3 Length & Penalties (affects Mode 0 start → STAT/HBlank IRQ timing)

`mode3_len = 172 + penalties`. `mode0_len = 376 − mode3_len`. Sources of penalty (each lengthens Mode 3, shortens Mode 0, and shifts when the Mode-0 STAT IRQ / HBlank fires):

| Source | Penalty |
|---|---|
| Base (2 dummy/first tile fetches) | +12 (included in the 172 floor: 160 + 12) |
| **SCX fine scroll** | `+ (SCX & 7)` dots at start of Mode 3 (pixels discarded). |
| **Window activation** | `+6` dots when the window first triggers on the line (BG fetcher reset to window). |
| **Each object on the line** | `+6` to `+11` dots, per OBJ penalty algorithm below. |

OBJ penalty algorithm (per object, processed left→right by X, ties by OAM index): consider the object's leftmost pixel ("The Pixel"); find the BG/Window tile it lands in; if that tile hasn't been counted for a prior OBJ this line: penalty = `max(0, (pixels of that tile strictly right of The Pixel) − 2)`; then always `+6` for the OBJ tile fetch. **Exception:** an OBJ with OAM X = 0 always incurs a flat **+11** regardless of SCX.

Max Mode 3 ≈ 289 dots (10 objects + window + SCX worst case). These exact numbers are what mooneye `intr_2_mode3_timing`, `intr_2_mode0_timing*`, `hblank_ly_scx_timing-GS`, and all mealybug `m3_*` tests verify — the Mode-0 STAT interrupt must fire at the *correct* dot = 80 + mode3_len.

---

## 10. LCD Disable → Enable: First-Frame Mode 3 Timing

Reference: mooneye `lcdon_timing-GS`, `lcdon_write_timing-GS`. Verified pass on DMG/MGB/SGB/SGB2; CGB differs (per-revision).

When LCDC.7 goes 1→0:
- PPU halts; LY resets to 0 and freezes; STAT mode bits read 0; VRAM/OAM fully open; no STAT/VBlank IRQs generated.

When LCDC.7 goes 0→1 (re-enable):
- PPU restarts at **LY=0** immediately. The **screen output for the first frame is blank** (not displayed) but the PPU is fully timing-active and *does* generate STAT/coincidence behavior.
- **Line 0 of the first frame is special:** it starts in **Mode 0**, then goes **straight to Mode 3** — there is effectively **no Mode 2** reported on this first line, and the line runs **~2 T-cycles "late"** versus a normal line on DMG. Lines 1 and 2 already have normal timing.
- Concretely (DMG/MGB), after enabling: the first line's Mode 3 begins about 2 dots earlier/shifted, so the dot at which OAM/VRAM become inaccessible and the dot at which Mode 0 begins are offset by 2 from steady state. Emulators must special-case the first scanline after LCD-on.
- The LY=LYC coincidence and STAT line are live from the first line; an `LYC=0` + LYC-enable will coincide at first-frame line 0.

CGB caveat: pre-CGB-D and CGB-D/E/AGB/AGS each diverge from the DMG result (the test expects DMG-class hardware to pass and CGB to "fail" the DMG expectation). Encode the line-0-after-enable offset as a per-model constant.

---

## 11. FF42 / FF43 — SCY / SCX (R/W)

- Specify top-left of the 160×144 viewport within the 256×256 BG map. `bottom = (SCY+143) mod 256`, `right = (SCX+159) mod 256`.
- Accessible during all modes (even Mode 3).
- **Mid-frame re-read:** SCY and the high bits of SCX are re-read **on each BG tile fetch** during Mode 3. The **low 3 bits of SCX (`SCX & 7`) are latched once at the start of the scanline** (used for the initial fine-scroll discard and the Mode-3 SCX penalty). Pre-CGB-D reads SCY once *per bitplane* (allows a precisely-timed SCY write to desync the two bitplanes — mealybug `m3_scy_change`); CGB-D+ uses one SCY for both.
- Tests: mealybug `m3_scx_low_3_bits`, `m3_scx_high_5_bits[_change2]`, `m3_scy_change[2]`.

## 12. FF44 / FF45 — LY (R) / LYC (R/W)

- **LY (`FF44`)**: read-only, 0–153. Reads 0 when LCD off. Writes ignored. (See §5/§6 for timing/quirks.)
- **LYC (`FF45`)**: R/W; compared against LY every dot to drive STAT bit 2 and the LYC STAT source. LYC writes take effect immediately for comparison.

## 13. FF4A / FF4B — WY / WX (R/W)

- **WY (`FF4A`)**: window top Y. **WX (`FF4B`)**: window left X **+ 7** (WX=7 ⇒ x=0).
- **Window "WY trigger" (Y condition):** cleared each VBlank; at the start of each scanline, if `WY == LY` the Y-condition latches true and stays true for the rest of the frame. (CGB: clearing LCDC.5 resets the Y-condition; to re-show, WY must be ≤ current LY again.)
- **Window X trigger:** an internal per-scanline counter starts at 0 and pre-increments 7 times (covering fine-scroll discards); when it equals WX and the Y-condition is true and LCDC.5 is set, the BG fetcher resets to the window tilemap, the window line counter increments, and a **+6** Mode-3 penalty is incurred.
- Window internal **line counter** only advances when the window actually activates on a line; hiding the window for a line (any method) does *not* advance it → re-showing repeats the same window row (vertical-stretch artifact; mealybug `m2_win_en_toggle`).
- Quirks: `WX=0` shifts the window left by `SCX&7`. **DMG only:** `WX=166` makes the window span the whole screen offset down by one scanline; and disabling the window mid-line at a BG tile boundary can insert one BGP-index-0 pixel (the "Star Trek" bug).
- Tests: mealybug `m3_window_timing`, `m3_window_timing_wx_0`, `m3_wx_4_change[_sprites]`, `m3_wx_5_change`, `m3_wx_6_change`, `m3_lcdc_win_en_change_multiple[_wx]`, `m2_win_en_toggle`, `m3_lcdc_win_map_change[2]`.

## 14. FF47 / FF48 / FF49 — BGP / OBP0 / OBP1 (R/W, DMG/CGB-DMG-mode)

Each register: four 2-bit fields mapping color **index → shade**.

| Bits | BGP field | OBP0/OBP1 field |
|---|---|---|
| 7–6 | shade for index 3 | shade for index 3 |
| 5–4 | shade for index 2 | shade for index 2 |
| 3–2 | shade for index 1 | shade for index 1 |
| 1–0 | shade for index 0 | **ignored** (index 0 = transparent for OBJ) |

Shade values: `0`=white, `1`=light gray, `2`=dark gray, `3`=black.

- **Mid-Mode-3 writes** to BGP/OBP0/OBP1 take effect with dot-precise timing and shift the visible effect by exactly the write's dot offset — this is the primary tool for measuring Mode-3 penalties on DMG. Tests: mealybug `m3_bgp_change`, `m3_bgp_change_sprites`, `m3_obp0_change`.
- In CGB mode BGP/OBP0/OBP1 are unused; palettes come from CRAM via `FF68`–`FF6B` (BCPS/BCPD/OCPS/OCPD), RGB555 little-endian, writable except during Mode 3 (auto-increment still advances on a failed Mode-3 write).

---

## 15. Per-Quirk → Test-ROM Cross-Reference

**Mooneye (`acceptance/ppu/` unless noted):**

| Quirk / behavior | Test(s) |
|---|---|
| Mode 2→Mode 0 IRQ timing | `intr_2_0_timing` |
| Mode 2→Mode 3 boundary timing | `intr_2_mode3_timing` |
| Mode 2→Mode 0 timing (HBlank) | `intr_2_mode0_timing` |
| Same, with sprites lengthening Mode 3 | `intr_2_mode0_timing_sprites` |
| OAM accessibility window vs Mode 2 | `intr_2_oam_ok_timing` |
| Mode 1↔Mode 2 STAT edge order | `intr_1_2_timing-GS` |
| HBlank LY/SCX-dependent timing | `hblank_ly_scx_timing-GS` |
| **STAT blocking** (single-line OR, rising edge) | `stat_irq_blocking` |
| **LYC compare on/off + LYC=0/153 edges** | `stat_lyc_onoff` |
| VBlank-entry STAT interrupt | `vblank_stat_intr-GS` |
| **LCD-on first-frame line-0 timing** (LY/STAT/OAM/VRAM access) | `lcdon_timing-GS` |
| LCD-on write timing | `lcdon_write_timing-GS` |
| OAM DMA timing/restart (OAM-access interaction) | `oam_dma_timing`, `oam_dma_restart`, `oam_dma_start` (misc/) |

**Mealybug Tearoom (`ppu/`, expected results per DMG / CGB-C / CGB-D) — Mode-3 register-change timing:**

| LCDC mid-Mode-3 | `m3_lcdc_bg_map_change[2]`, `m3_lcdc_win_map_change[2]`, `m3_lcdc_tile_sel_change[2]`, `m3_lcdc_tile_sel_win_change[2]`, `m3_lcdc_obj_en_change[_variant]`, `m3_lcdc_obj_size_change[_scx]`, `m3_lcdc_bg_en_change[2]` |
|---|---|
| Window enable/timing | `m2_win_en_toggle`, `m3_lcdc_win_en_change_multiple[_wx]`, `m3_window_timing`, `m3_window_timing_wx_0` |
| Scroll | `m3_scx_low_3_bits`, `m3_scx_high_5_bits[_change2]`, `m3_scy_change[2]` |
| WX | `m3_wx_4_change[_sprites]`, `m3_wx_5_change`, `m3_wx_6_change` |
| Palettes mid-Mode-3 | `m3_bgp_change`, `m3_bgp_change_sprites`, `m3_obp0_change` |

**Blargg / others (general PPU/IRQ sanity):** `oam_bug` suite (OAM corruption), and integration via games depending on the DMG STAT write bug (*Road Rash*, *Xerd no Densetsu*).

---

## 16. Implementation Checklist (REVENANT engine)

1. Run PPU at dot granularity; maintain `(mode, ly, dot_in_line, mode3_len)`.
2. Recompute `stat_line` every dot from current mode + LYC compare gated by the four enables; latch prev; set `IF.1` on rising edge only.
3. STAT register: store only bits 6–3 on write; bits 7 reads 1, bits 2–0 reflect live state (0 when LCD off).
4. DMG-class STAT write → inject one M-cycle of enables=`1111` before applying written value; rerun edge detector. Skip on CGB.
5. LYC comparator must see the line-153 early-wrap LY (153 for ~1 M-cycle, then 0) so LYC=0 and LYC=153 both edge correctly.
6. OAM scan: address-ordered, Y-only test, cap 10, finalize by dot 80; off-screen-X sprites still consume slots.
7. Mode-3 length = 172 + (SCX&7) + window(+6) + Σ OBJ penalties (X=0 ⇒ +11); Mode-0 begins at dot 80+mode3_len → that's when the Mode-0 STAT source can edge and when OAM/VRAM reopen.
8. LCD-on: restart at LY=0, first line = Mode0→Mode3 (no Mode 2), 2-dot DMG offset; screen output suppressed for first frame; STAT/coincidence live.
9. SCX low-3 latched at line start; SCY/SCX-high re-read per tile fetch (per-bitplane SCY on pre-CGB-D).
10. Window Y-condition latched at line start when WY==LY, cleared at VBlank; window line counter advances only on actual activation.