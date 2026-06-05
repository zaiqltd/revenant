# CGB features

I now have all the authoritative data needed. Let me write the comprehensive specification.

```markdown
# REVENANT — Game Boy Color (CGB) Subsystem Specification

Authoritative basis: Pan Docs (gbdev.io), gbdev wiki, Mooneye-GB / Blargg / Mealybug / MagenTests test-ROM docs, ISSOtm gb-bootroms disassembly, SameBoy boot-ROM reimplementation. All timings in T-cycles (4.194304 MHz base) and M-cycles (1 M = 4 T). "DSM" = Double-Speed Mode.

---

## 0. Terminology & Global Constants

| Symbol | Value | Notes |
|---|---|---|
| T-cycle (base) | 1 / 4194304 s ≈ 238.4 ns | "dot" period; never changes with speed |
| M-cycle (normal) | 4 T ≈ 0.954 µs | CPU machine cycle |
| M-cycle (double) | 4 T but at 2× CPU clock → ≈ 0.477 µs | "fast" M-cycle |
| CPU clock normal | 4.194304 MHz | 1.05 MHz instruction throughput |
| CPU clock double | 8.388608 MHz | 2.10 MHz instruction throughput |
| Scanline | 456 dots | constant in both speeds |
| Frame | 154 lines × 456 = 70224 dots | constant in both speeds |

CRITICAL TIMING INVARIANT: In DSM, the **CPU, DIV/timers, serial, and OAM-DMA** double in rate; the **PPU/LCD, HDMA-to-VRAM rate, and all APU timing/frequencies** do NOT. The PPU is driven by the unchanging dot clock; the CPU simply executes twice as many M-cycles per scanline (912 fast M-cycles' worth of CPU work per line vs 456).

---

## 1. CGB Mode Detection & Hardware Identification

### 1.1 Cartridge header byte $0143 (CGB flag)

| $0143 value | Meaning | Resulting machine mode on CGB |
|---|---|---|
| `$80` | CGB-enhanced, DMG-compatible | Full CGB mode |
| `$C0` | CGB-only | Full CGB mode |
| `$00` (or any with bit 7 clear) | DMG game | DMG-compatibility mode (CGB DMG-compat) |
| `$X8`, `$XC` (bit 7 set + bits 2/3) | PGB candidate | See §11 (PGB, unresearched) |

Rule the boot ROM applies: **bit 7 set ($80/$C0) → CGB mode; bit 7 clear → DMG-compatibility mode.** Bits 0–5 of $0143 are otherwise part of the title region for older carts; only bit 7 (and the special $X8/$XC PGB encodings via bits 2/3) matter for mode selection. CGB-only registers (VBK, SVBK, KEY1, HDMA1–5, RP, BCPS/BCPD, OCPS/OCPD, SVBK) are **inert and read $FF in DMG-compat / Non-CGB mode** — you must be in CGB mode (header bit 7 set) to use them.

### 1.2 Software detection of CGB/AGB hardware (post-boot register state)

At handoff (`PC=$0100`), register **A** identifies the running console:

| A | Console family |
|---|---|
| `$01` | DMG / SGB |
| `$FF` | MGB / SGB2 |
| `$11` | **CGB or GBA (AGB)** |

When `A=$11`, distinguish CGB vs GBA via **B bit 0**: `0` = CGB, `1` = GBA. (AGB boot ROM does an extra `inc b`.) Games use this to apply the GBA brightness correction (≈ `GBA = GBC×3/4 + $08` per channel; see §7.4).

### 1.3 Full post-boot CPU register state (CGB / AGB, at PC=$0100)

| Reg | CGB (CGB mode) | AGB (CGB mode) | CGB (DMG-compat) | AGB (DMG-compat) |
|---|---|---|---|---|
| A | $11 | $11 | $11 | $11 |
| F | Z=1 N=0 H=0 C=0 | Z=0 N=0 H=0 C=0 | Z=1 N=0 H=0 C=0 | Z=? N=0 H=? C=0 |
| B | $00 | $01 | title-checksum* | title-checksum*+1 |
| C | $00 | $00 | $00 | $00 |
| D | $00 | $00 | $00 | $00 |
| E | $56 | $56 | $08 | $08 |
| H | $00 | $00 | varies† | varies† |
| L | $0D | $0D | varies† | varies† |
| PC | $0100 | $0100 | $0100 | $0100 |
| SP | $FFFE | $FFFE | $FFFE | $FFFE |

\* B in DMG-compat = sum of 16 title bytes if licensee==Nintendo ($01, or $33 with new-licensee "01"), else $00; +1 on AGB.
† DMG-compat HL: if B==$43 or $58 (CGB)/$44 or $59 (AGB) then **HL=$991A**, else **HL=$007C**. (These encode whether the DMG-logo tilemap was written.)

### 1.4 Post-boot I/O register state — CGB/AGB-specific (PC=$0100)

| Reg | Addr | CGB/AGB value | Notes |
|---|---|---|---|
| DMA | $FF46 | $00 | (DMG: $FF) |
| KEY0 | $FF4C | locked (write-only by boot; reads vary) | see §10 |
| KEY1 | $FF4D | $7E | bit0=0, bits unused read 1 |
| VBK | $FF4F | $FE | bit0=0 → bank 0; other bits read 1 |
| HDMA1–4 | $FF51–54 | $FF | write-only |
| HDMA5 | $FF55 | $FF | bit7=1 ⇒ "no active transfer" |
| RP | $FF56 | $3E | IR idle |
| BCPS | $FF68 | impl/compat-dependent | depends on DMG-compat |
| OCPS | $FF6A | impl/compat-dependent | |
| SVBK | $FF70 | $F8 | bits0-2=0 (→bank1 mapped); bits3-7 read 1 |
| FF72 | $FF72 | $00 | r/w |
| FF73 | $FF73 | $00 | r/w |
| FF74 | $FF74 | $00 (CGB) / $FF locked (DMG-compat) | |
| FF75 | $FF75 | bits 4-6 r/w (init 0), others read 1 | |
| PCM12 ($FF76) | $FF76 | live APM | read-only |
| PCM34 ($FF77) | $FF77 | live APM | read-only |

These CGB-only registers read **$FF in Non-CGB / DMG mode**. LCDC=$91, BGP=$FC, STAT≈$81/$85 region, others as DMG.

---

## 2. VRAM Banking — VBK ($FF4F)

VRAM is 16 KiB on CGB = 2 banks × 8 KiB, mapped at `$8000–$9FFF`.

### 2.1 VBK register layout

| Bit | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
|---|---|---|---|---|---|---|---|---|
| VBK | — | — | — | — | — | — | — | Bank |

- **Write:** only bit 0 is used; bank = bit0 (0 or 1). All other bits ignored.
- **Read:** returns current bank in bit 0; **all other bits read as 1** (so bank0 reads $FE, bank1 reads $FF).

### 2.2 Bank purpose

| Range | Bank 0 | Bank 1 |
|---|---|---|
| `$8000–$97FF` | Tile data (blocks 0/1/2) | **More tile data** (accessible same way/time as bank 0) |
| `$9800–$9BFF` | BG tile-map 0 (tile indices) | **BG attribute map 0** |
| `$9C00–$9FFF` | BG tile-map 1 (tile indices) | **BG attribute map 1** |

A tile fetch can pull pixel data from **either** bank per-tile (selected by attribute "Bank" bit), independent of the current VBK setting. VBK only governs CPU access to `$8000–$9FFF`.

### 2.3 BG attribute map (VRAM bank 1, `$9800–$9FFF`)

Each attribute byte at `1:addr` describes the single tile whose index is at `0:addr` (1:1 positional, NOT per tile-id).

| Bit | 7 | 6 | 5 | 4 | 3 | 2-0 |
|---|---|---|---|---|---|---|
| Field | BG-OAM Priority | Y flip | X flip | (ignored) | Tile VRAM bank | BG palette (BGP0–7) |

- **Bit 7 — BG/OAM priority:** `0`=no; `1`= BG/Window color indices **1–3** drawn over OBJ regardless of OAM priority (subject to LCDC.0; see §8).
- **Bit 6 — Y flip:** `1` = tile drawn vertically mirrored.
- **Bit 5 — X flip:** `1` = tile drawn horizontally mirrored.
- **Bit 4:** ignored by hardware, **but read/write storable normally** (test ROMs verify it round-trips).
- **Bit 3 — Bank:** tile pixel data fetched from VRAM bank 0 (`0`) or bank 1 (`1`).
- **Bits 2-0 — Palette:** selects BG palette 0–7 from BG-CRAM.

Attribute fetch participates in the PPU pixel-fetcher pipeline (fetched alongside the tile index during BG/Window tile fetch in Mode 3).

---

## 3. WRAM Banking — SVBK ($FF70)

CGB WRAM = 32 KiB = 8 banks × 4 KiB.

| Range | Mapping |
|---|---|
| `$C000–$CFFF` | Always WRAM bank 0 (fixed) |
| `$D000–$DFFF` | Switchable WRAM bank 1–7 (via SVBK) |
| `$E000–$FDFF` | Echo of `$C000–$DDFF` (mirrors bank0 + selected bank) |

### 3.1 SVBK layout

| Bit | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
|---|---|---|---|---|---|---|---|---|
| SVBK | — | — | — | — | — | WRAM bank (3 bits) |||

- Bits 0–2 select the bank into `$D000–$DFFF`.
- **Bank-0 quirk:** writing `%000` maps **bank 1** (i.e. value 0 ⇒ bank 1). Values 1–7 map banks 1–7. Effectively `bank = (value & 7) == 0 ? 1 : (value & 7)`.
- **Read:** bits 0–2 return the stored value; **bits 3–7 read as 1** (so a value of 0 reads as $F8).
- Echo RAM at `$E000–$FDFF` follows the same banked mapping.

---

## 4. CGB Color Palettes — BCPS/BCPD ($FF68/$FF69), OCPS/OCPD ($FF6A/$FF6B)

CRAM = two independent 64-byte palette RAMs: **BG-CRAM** (8 palettes × 4 colors × 2 bytes) and **OBJ-CRAM** (same size). Accessed indirectly through index+data register pairs.

### 4.1 BCPS / OCPS (palette specification / index) — $FF68 / $FF6A

| Bit | 7 | 6 | 5-0 |
|---|---|---|---|
| Field | Auto-increment | (unused, reads 1) | Address (0–63) |

- **Address (bits 0–5):** byte offset into the 64-byte palette RAM. Layout:
  `addr = palette*8 + color*2 + byte` where byte 0 = low (red + low green), byte 1 = high (blue + high green).
  Example: addr `$03` = high byte of BGP0 color #1.
- **Bit 7 — Auto-increment:** `1` = after **each write to BCPD/OCPD**, Address increments by 1 (wraps 63→0, staying within 6 bits). Reads of the data port **never** auto-increment.
- Auto-increment still occurs **even when the data write is blocked during Mode 3** (the write to CRAM fails, but the index still advances — Mealybug/accuracy ROMs check this).
- BCPS/OCPS themselves are readable/writable at any time (not locked by PPU).

### 4.2 BCPD / OCPD (palette data) — $FF69 / $FF6B

- Reads/writes the CRAM byte at the current BCPS/OCPS Address.
- **CRAM is locked during PPU Mode 3** (pixel transfer): reads return $FF / writes are dropped — exactly like VRAM. Accessible only during Mode 0 (HBlank), Mode 1 (VBlank), Mode 2 (OAM scan), and when LCD off. (NOTE: the *index* register can still be touched in Mode 3; only the *data* port is blocked.)

### 4.3 Color format — little-endian RGB555 (stored), displayed BGR

Each color = 2 bytes, 15-bit color:

| Bit | 15 | 14-10 | 9-5 | 4-0 |
|---|---|---|---|---|
| Field | unused | Blue (0–31) | Green (0–31) | Red (0–31) |

- Low byte (even addr): `GGGRRRRR` (green bits 2-0 in high nibble of low byte, red in bits 4-0).
- High byte (odd addr): `0BBBBBGG` (blue bits 4-0, green bits 4-3).
- 8 BG palettes (BGP0–7), 8 OBJ palettes (OBP0–7), 4 colors each.
- **OBJ color #0 is always transparent** — never displayed; may be left uninitialized.

### 4.4 Boot-ROM CRAM init state
- **CGB mode:** boot ROM sets all BG colors to white; OBJ colors left **uninitialized/random** except OBP0 color#0 low byte = $00 (unused). Software must init its own OBJ palettes.
- **DMG-compat mode:** boot ROM writes compatibility palettes (§7).

---

## 5. Double-Speed Mode — KEY1 ($FF4D) + STOP

### 5.1 KEY1 layout

| Bit | 7 | 6-1 | 0 |
|---|---|---|---|
| Field | Current speed (R) | (unused, reads 1) | Switch armed (R/W) |

- **Bit 7 (read-only):** `0` = Normal speed, `1` = Double speed (current state).
- **Bit 0 (R/W):** `1` = a speed switch is armed.
- Unused bits read 1 → KEY1 reads `$7E` (normal, unarmed) or `$FE` (double, unarmed) etc.
- Power-on: Normal speed (bit7=0); KEY1=$7E.

### 5.2 Speed-switch sequence (must execute `STOP`)

```
IF KEY1.bit7 != desired_speed:
    IE   = $00      ; FFFF = 0   (avoid pending interrupt aborting STOP)
    JOYP = $30      ; FF00 = $30 (deselect both key matrices — avoids STOP-as-2-byte quirk)
    KEY1 = $01      ; arm switch (bit0=1)
    STOP            ; performs the switch
```
After STOP: bit0 auto-clears, bit7 toggles, machine runs at the other speed.

### 5.3 STOP-for-speed-switch timing & state (exact)
- CPU is halted **2050 M-cycles = 8200 T-cycles** after STOP executes.
- During this window the CPU is in a "frozen" state:
  - **DIV does not tick** → some APU frame-sequencer events are skipped.
  - VRAM/OAM/CRAM locking is **frozen at whatever PPU mode the switch began in**:
    - Mode 0/1 (H/VBlank): PPU can access **no** video memory → outputs **black** pixels for the duration.
    - Mode 2 (OAM scan): PPU can read VRAM but not OAM → background renders, objects don't.
    - Mode 3 (rendering): PPU accesses everything normally → unaffected.
- The PPU/LCD dot clock itself never stops; only CPU/DIV are paused.

### 5.4 What changes in DSM
| Doubles in DSM | Unchanged in DSM |
|---|---|
| CPU clock (2.10 MHz) | LCD/PPU (dot clock) |
| DIV & TIMA timers | HDMA VRAM transfer rate (2 bytes/µs) |
| Serial (link) clock | All APU timings/frequencies |
| OAM-DMA (FF46) transfer | |

OAM DMA: 160 M-cycles normal, but completes in half the wall-clock time in DSM.

---

## 6. VRAM DMA (HDMA) — $FF51–$FF55

Two transfer engines selected by HDMA5 bit 7 on write: **General-purpose DMA (GDMA)** and **HBlank DMA (HDMA)**. Both copy ROM/SRAM/WRAM → VRAM in 16-byte units.

### 6.1 Source/Destination registers

| Reg | Addr | Field | Notes |
|---|---|---|---|
| HDMA1 | $FF51 | Source high byte | write-only |
| HDMA2 | $FF52 | Source low byte | low 4 bits ignored (treated 0) |
| HDMA3 | $FF53 | Dest high byte | only bits 12–8 used; upper 3 bits ignored (dest always VRAM) |
| HDMA4 | $FF54 | Dest low byte | low 4 bits ignored (treated 0) |
| HDMA5 | $FF55 | Length / mode / start (+ status on read) | |

- **Source:** `$0000–$7FF0` (ROM) or `$A000–$DFF0` (SRAM/WRAM). Lower 4 bits forced to 0. Source in VRAM → **garbage copied** (do not). Echo/OAM/IO/HRAM source: untested/undefined.
- **Destination:** `$8000–$9FF0`. Only bits 12–4 of the 16-bit address are respected (bits 15–13 ignored → always VRAM; bits 3–0 forced 0). Destination uses the **current VBK bank**.

### 6.2 HDMA5 — length / mode / start

| Bit | 7 | 6-0 |
|---|---|---|
| Write | Mode: 0=GDMA, 1=HDMA | Length = (bytes/$10) − 1 |

- **Length:** value `$00–$7F` ⇒ `$10`–`$800` bytes (i.e. `(N+1)*16`).
- **Read of HDMA5:**
  - Bit 7 = **0 ⇒ transfer active**, **1 ⇒ not active / completed**.
  - Bits 6–0 = remaining length − 1 (blocks remaining minus one); $FF overall = done.

### 6.3 GDMA (mode bit 7 = 0)
- Transfers **all** bytes at once; **CPU halted** for the whole transfer.
- Copies blindly — does **not** wait for the PPU to release VRAM. So safe only when **LCD off, during VBlank, or (short blocks) during HBlank**; running during Mode 3 corrupts.
- On completion HDMA5 reads **$FF**.
- **Timing:** **2 bytes per microsecond**, i.e. **8 M-cycles per $10 block** in Normal speed; **16 fast M-cycles per block** in DSM (≈8 µs/block regardless of speed → wall-clock identical). Total ≈ `(N+1) * 8` normal M-cycles. VBlank budget ≈ 2280 bytes (≈142.5 tiles).

### 6.4 HDMA (HBlank DMA, mode bit 7 = 1)
- Transfers exactly **$10 bytes per HBlank** (during Mode 0), at LY=0–143.
- **No** transfer during VBlank (LY 144–153); resumes at LY=0.
- CPU is halted **only during each 16-byte burst** (8 M-cyc normal / 16 fast M-cyc DSM); runs freely in the gaps.
- **Software must not change VBK (dest bank) or the source ROM/RAM bank mid-transfer** (pause it first).
- **HALT interaction:** if CPU is HALTed, the HDMA is also paused and only resumes when the CPU resumes (MagenTests `vram-dma-hblank` checks this).
- **WARNING:** do **not** start an HDMA (write HDMA5) while already in HBlank (STAT mode 0) — first block timing is undefined/buggy.

### 6.5 Terminating / overflow
- Writing HDMA5 with **bit 7 = 0 while an HDMA is active stops it.** Subsequent read: bits 6–0 = blocks remaining − 1, **bit 7 reads 1**. Stopping does **not** reset HDMA1–4 to $FF.
- If the destination address **overflows past $9FF0**, the transfer **stops prematurely** (exact register residue still under investigation — implement: stop, set HDMA5 bit7=1).
- Bit 7 of HDMA5 reliably reports active(0)/inactive(1) in all cases (GDMA done, HDMA done, manual stop).

### 6.6 MBC caveat
Older MBCs (MBC1-3) / slow ROMs may not sustain the 2 bytes/µs rate — the engine always demands 2 bytes/µs even in Normal speed; accuracy emulators model the fixed rate, not MBC stalls.

---

## 7. CGB DMG-Compatibility Palettes

When a DMG game ($0143 bit7 clear) runs on CGB, the machine enters **DMG-compatibility mode** (KEY0 ← $04; OPRI ← $01). The DMG palette registers still exist but **index into CGB CRAM**:

- BG uses **BG palette 0** (the whole attribute map is forced to 0 → palette0, bank0, no flips/priority).
- OBJs use **OBJ palette 0 or 1**, selected by OAM attribute **bit 4** (the DMG OBP0/OBP1 select bit).
- `BGP`, `OBP0`, `OBP1` ($FF47–49) act as **indices into the CGB palettes** (mapping the 2-bit shade IDs onto the boot-selected colors), not literal grays.

### 7.1 Compatibility palette selection algorithm (boot ROM)
1. Verify licensee == Nintendo: old-licensee $01, OR old-licensee $33 with new-licensee ASCII "01". If not → palette ID **$00**.
2. Compute **title checksum** = sum of all 16 title bytes ($0134–$0143).
3. Look up checksum in the boot-ROM table → index.
   - Not found → ID $00.
   - Index ≤ 64 → ID = index.
   - Index > 64 → disambiguate by the **4th title letter** via a second table: `ID = index + 14 * row`. Letter not found → ID $00.
4. ID picks 3 palettes (BG, OBP0, OBP1) from a table. Player can override during the logo animation with button combos (some overrides are unique).
- If ID ∈ {$43, $58}, the boot ROM also writes the **DMG-logo tilemap** to VRAM (sets HL=$991A at handoff; see §1.3).

(Full checksum→palette table: TCRF "Game Boy Color Bootstrap ROM"; SameBoy `cgb_boot.asm`.)

---

## 8. CGB OBJ Priority, BG/OBJ Priority, OPRI ($FF6C)

### 8.1 Object-vs-object drawing priority
- **DMG mode:** smaller X coordinate wins; ties broken by lower OAM index.
- **CGB mode:** **OAM index only** — earlier OAM entry (lower address) always wins, regardless of X.
- Selectable via OPRI (below).

### 8.2 OPRI — $FF6C

| Bit | 7-1 | 0 |
|---|---|---|
| Field | (unused) | Priority mode (R/W) |

- Bit 0: `0` = CGB-style (OAM-index) priority, `1` = DMG-style (X-coordinate) priority.
- Boot ROM sets OPRI=$01 when entering DMG-compat mode.
- Quirk: OPRI write takes effect **instantly** only while a PGB-style ($X8/$XC) value was written to KEY0 and STOP not yet executed. After boot ROM unmaps (in normal CGB/DMG operation) it has **no (or no instant) effect** — to be verified. Implement: latch OPRI at mode-entry; treat mid-game writes as no-ops for safety unless a test ROM dictates otherwise.

### 8.3 The 10-objects-per-scanline limit (unchanged from DMG)
- OAM scanned $FE00→$FE9F; first ≤10 objects whose Y range covers LY (using LCDC.2 size 8×8/8×16) selected.
- Off-screen X (0 or ≥168) objects **still count** toward the 10 limit; only Y outside range removes them.

### 8.4 BG-to-OBJ priority resolution in CGB mode (3-flag system)
Inputs: **LCDC.0** (BG&Win master priority), **OAM attr bit 7** (BG-over-OBJ), **BG attr bit 7** (BG-over-OBJ). Resolved per-pixel:

| LCDC.0 | OAM.7 | BGattr.7 | Result |
|---|---|---|---|
| 0 | x | x | **OBJ** always (BG loses priority entirely) |
| 1 | 0 | 0 | **OBJ** |
| 1 | 0 | 1 | BG if BG color 1–3, else OBJ |
| 1 | 1 | 0 | BG if BG color 1–3, else OBJ |
| 1 | 1 | 1 | BG if BG color 1–3, else OBJ |

Derived rules:
- BG color index **0** ⇒ OBJ always wins.
- LCDC.0 = 0 (CGB) ⇒ OBJ always on top (BG/Win keep displaying — LCDC.0 in CGB is a *priority master*, NOT a BG-disable like on DMG).
- Else if **either** BGattr.7 **or** OAM.7 set ⇒ BG colors 1–3 over OBJ.
- Else OBJ over BG.

### 8.5 OBJ-vs-OBJ vs "BG-over-OBJ" interaction (non-intuitive)
The PPU first picks the **object pixel** = first non-transparent pixel among objects sorted by drawing priority (§8.1). The "BG-over-OBJ" attribute is **not** consulted during this selection. *Only after* the object pixel is chosen is its owner's BG-over-OBJ flag applied vs the BG. Consequence: a higher-priority object with BG-over-OBJ set can **mask** lower-priority objects (even ones with BG-over-OBJ clear) — exploited to hide only parts of a sprite behind BG. (MagenTests verifies.)

### 8.6 LCDC.0 semantics by mode (summary)
- **DMG / DMG-compat:** LCDC.0=0 ⇒ BG & Window blank (white), Window-enable (LCDC.5) ignored; objects still draw.
- **CGB mode:** LCDC.0=0 ⇒ BG & Window **lose priority** (objects always on top), but BG/Window still render their colors. LCDC.0=1 ⇒ priority per §8.4.

---

## 9. CGB Display Color Science (for renderer correctness)

- Stored color is RGB555; display is **not** linear sRGB. Max intensity ≈ light-gray (screen unlit), not white. $10–$1F all appear very bright; $00–$0F are the medium/dark range.
- Channels cross-contaminate: raising one of R/G/B shifts the others (pigment cross-talk). E.g. $03EF (B=0,G=$1F,R=$0F) → neon green on sRGB but washed-out yellow on CGB.
- **GBA (original AGB) correction:** medium intensities differ; $00–07 nearly black on early GBA. Approx `GBA_channel = GBC_channel × 3/4 + $08`. GBA SP / Game Boy Player not affected — ideally make brightness correction user-controllable. Implement a CGB color LUT (e.g. the SameBoy/144p-derived curve) rather than a naive `×8` expansion.

---

## 10. KEY0 ($FF4C) — CPU Mode Select (boot-only)

- Written only by the CGB boot ROM; **locked** after boot ROM unmaps (write to BANK $FF50). Not directly testable post-boot (MagenTests "key0 lock after boot").

| Bit | 7-4 | 3 | 2 | 1-0 |
|---|---|---|---|---|
| Field | — | (PGB-related?) | DMG-compat mode | — |

- **Bit 2 — DMG compatibility:** `0` = full CGB mode, `1` = DMG-compat (boot writes $04 for DMG games).
- For CGB games, the boot ROM writes the raw $0143 value to KEY0 (so $80/$C0); the $X8/$XC encodings select **PGB mode** (bits 2/3 pattern).
- Bit 3 speculated to enable **PGB** (external LCD control) — unresearched.
- Locked-state read value: implementation should treat as fixed post-boot.

---

## 11. Undocumented Registers FF72–FF77

| Reg | Addr | Behavior |
|---|---|---|
| (unnamed) | $FF72 | Fully **R/W**. Init $00. Holds whatever is written. |
| (unnamed) | $FF73 | Fully **R/W**. Init $00. |
| (unnamed) | $FF74 | In **CGB mode**: fully R/W, init $00. In **DMG-compat/Non-CGB**: **read-only, locked $FF**. |
| (unnamed) | $FF75 | Only **bits 4, 5, 6** are R/W (init 0); bits 7,3-0 read 1. ⇒ reads as `1xxx1111` with xxx = stored bits6-4. |
| PCM12 | $FF76 | **Read-only.** Bits 0–3 = APU channel 1 current digital amplitude (0–15), bits 4–7 = channel 2 amplitude. Live. |
| PCM34 | $FF77 | **Read-only.** Bits 0–3 = channel 3 amplitude, bits 4–7 = channel 4 amplitude. Live. |

- FF72–FF75 read $FF in Non-CGB hardware contexts where CGB regs are inert; FF76/FF77 (PCM) reflect APU regardless and are R/O. Useful as emulator-vs-DMG fingerprints.
- PCM12/PCM34 update every APU sample tick; emulator must expose the per-channel post-envelope DAC amplitude (the value fed to the mixer), 4 bits each, 0 when DAC off / channel muted.

---

## 12. PGB Mode (research-incomplete — flag for later)
- Triggered by KEY0 value `$X8`/`$XC` (header $0143 = $X8/$XC). External-LCD control mode; behavior largely undocumented (Pan Docs issue #581). OPRI writes take instant effect in this transitional state (§8.2). Implement a stub gate; do not block CGB-core correctness on it.

---

## 13. Implementation Checklist / Test-ROM Coverage Map

| Feature | Source files / tests to pass |
|---|---|
| Boot register & HWIO state | mooneye `misc/boot_regs-cgb`, `misc/boot_regs-A`, `misc/boot_hwio-C` |
| HDMA HBlank + HALT pause | MagenTests `vram-dma-hblank` |
| GDMA vs HDMA timing (8 M-cyc/block) | hdma_normal_speed / hdma_double_speed (Pan Docs figs) |
| BG/OBJ + OPRI priority table | MagenTests (BG-OBJ priority) |
| CRAM auto-inc during Mode 3 | Mealybug Tearoom (palette write timing) |
| KEY0 lock after boot | MagenTests `key0 lock` |
| Speed-switch STOP (2050 M-cyc) | accuracy/hardware notes |
| DMG-compat colorization | TCRF bootstrap table, SameBoy cgb_boot.asm |

### Key invariants to assert in code
1. CGB regs inert + read $FF unless header bit7 set (CGB mode latched at reset).
2. VBK read = bank|$FE; SVBK read bits3-7=1, value0⇒bank1.
3. CRAM data port blocked in Mode 3 but index auto-inc still fires on blocked write.
4. HDMA = 16 bytes per Mode-0 entry; dest uses live VBK; overflow past $9FF0 aborts.
5. CGB OBJ priority = OAM index (when OPRI=0); object-pixel chosen before BG-over-OBJ applied.
6. DSM doubles CPU/DIV/serial/OAM-DMA only; PPU + HDMA-byte-rate + APU unchanged.
7. STOP speed-switch: 8200 T-cycle freeze, DIV stopped, PPU video-mem lock frozen at entry mode.
```