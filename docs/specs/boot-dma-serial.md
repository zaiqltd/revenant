# Boot, OAM DMA, serial, joypad

I now have all the authoritative details needed. Here is the complete specification.

---

# REVENANT — Hardware Reference Spec: Boot ROM, OAM DMA, Serial, Joypad, Post-Boot State

Authoritative sources: Pan Docs (Power_Up_Sequence, OAM_DMA_Transfer, Serial, Joypad, OAM_Corruption_Bug), gb-bootroms disassembly (ISSOtm/codeberg), Mooneye-GB acceptance tests, GBEDG DMA notes, SameBoy boot ROMs. Clock base: 1 M-cycle = 4 T-cycles. DMG/SGB master clock 4.194304 MHz; CGB double-speed mode (CPU) 8.388608 MHz, but the divider-derived peripheral clocks (DIV, serial 8192 Hz base, OAM-DMA dot timing) are defined relative to the *current* CPU clock as noted per subsystem.

---

## 1. Boot ROM

### 1.1 Memory mapping and handoff

| Property | DMG / MGB / SGB / SGB2 | CGB / AGB |
|---|---|---|
| Boot ROM size | 256 B | 2048 B (256 + 1792) |
| Mapped region | `$0000–$00FF` over cart ROM | `$0000–$00FF` **and** `$0200–$08FF`; `$0100–$01FF` is the live cartridge header |
| CPU reset PC | `$0000` (NOT `$0100`) | `$0000` |
| Unmap trigger | write to `BANK` reg `$FF50` | write to `$FF50` |
| Unmap is | **one-way, latched** — once any value is written to `$FF50`, boot ROM is permanently unmapped until hard reset | same |

**Unmap mechanism / handoff timing**: The boot ROM's final instruction is `ld a, <val>` then `ldh [$FF50], a`. The `ldh [$FF50],a` opcode lives at `$00FE` and is 2 bytes (`E0 50`), so the very next fetch is at `$0100` — the cartridge entry point. The value in `A` at that moment is therefore observable by the game (see §2). The write to `$FF50` takes effect such that the fetch of the instruction at `$0100` already sees cartridge ROM (boot ROM is unmapped on the same M-cycle the write completes).

**`$FF50` (BANK) register semantics**:
- Write-only-effective for unmapping. Writing **any value with bit 0 = 1** unmaps (DMG boot writes `$01`, MGB `$FF`, SGB `$01`, SGB2 `$FF`, CGB/AGB `$11`).
- Reads of `$FF50`: after unmap returns `$FF` (open bus / all bits set). Before unmap, reads `$FE` on most implementations (only bit 0 meaningful).
- Implementation: hold `boot_rom_mapped: bool` initialized `true`. On write to `$FF50`, if `value & 1 == 1` set `false` (latch — never re-enable). Memory read of `$0000–$00FF` (and CGB `$0200–$08FF`) routes to boot ROM iff `boot_rom_mapped`.

### 1.2 Boot ROM variants (all official, for completeness)

| Name | Size | A→FF50 | Distinguishing behavior |
|---|---|---|---|
| DMG0 | 256 | `$01` | Early rev; logo+checksum check **before** display; blinks white/black on fail; no ® symbol |
| DMG | 256 | `$01` | Standard monochrome |
| MGB | 256 | `$FF` | 1-byte diff vs DMG (the `$FF` write) |
| SGB | 256 | `$01` | No checks; forwards header to SGB BIOS via packets `$F1,$F3,$F5,$F7,$F9,$FB` |
| SGB2 | 256 | `$FF` | 1-byte diff vs SGB |
| CGB0 | 2048 | `$11` | Does **not** init wave RAM (`$FF30–$FF3F`); affects R-Type title music |
| CGB | 2048 | `$11` | Standard color; split ROM, compatibility colorization |
| AGB0 | 2048 | `$11` | `inc b` for GBA detection (B differs) |
| AGB | 2048 | `$11` | Fixes logo TOCTTOU |

### 1.3 Monochrome boot sequence (DMG/MGB)

1. Zero VRAM (`$8000–$9FFF`).
2. Init audio: NR-registers as in §2 table; play setup.
3. Read 48-byte Nintendo logo from header `$0104–$0133`, unpack (each nibble doubled horizontally/vertically → scaled 2×) into VRAM tiles, build tilemap.
4. Set `SCY` and scroll logo **downward** from top; classic "drop" animation. (Reads from absent cart return `$FF` → black box.)
5. Once scroll finishes: play **"ba-ding!"** — two-note chime on CH1. Sequence (DMG boot): set NR-regs, trigger CH1 with sweep, two pitch steps.
6. Re-read logo, compare to internal copy (48 bytes). Compute header checksum: `x = 0; for i in $0134..=$014C { x = x - mem[i] - 1 }`; compare low byte to `mem[$014D]`.
7. On any mismatch → **lock up** (infinite loop; control never reaches cart). DMG0 instead blinks screen.
8. `ld a,$01` (`$FF` for MGB); `ldh [$FF50],a`; falls through to `$0100`.

**SGB/SGB2**: no local checks; unpack logo, transmit entire header to SNES via 16-byte command packets (packet IDs `$F1…$FB`), then unmap. SGB BIOS (on SNES) does the logo+checksum verification. Boot duration is **header-dependent** (sending a set bit is 1 cycle faster than a reset bit; 4 VBlank waits per packet) → `DIV`/`LY`/`STAT` at `$0100` are non-deterministic for a given build but static per header.

### 1.4 CGB boot sequence (CGB/AGB)

1. Unpack logo to VRAM (scaled, like DMG) **and** copy full logo to an HRAM buffer (anti-tamper, HRAM is on-die).
2. Decompress logo a second time at native size → small logo under "GAME BOY" wordmark.
3. Compute compatibility-palette ID (see §1.5), set up CGB palettes.
4. Play logo animation + "ba-ding!". During animation, if header `$0143` bit 7 = 0 (DMG-only game), user may pick a manual palette via button combos; each new pick delays end by 30 frames.
5. Check logo **from HRAM buffer** — but only the **first half** is compared (TOCTTOU/colorization quirk; full logo present but bottom half unchecked). Verify header checksum (same formula).
6. On fail → lock up.
7. Fade all BG palettes to white. Set compatibility mode:
   - If `$0143` indicates CGB-compatible: write `$0143` byte directly to `KEY0 ($FF4C)` (can enable PGB mode).
   - Else (DMG game): write `$04` to `KEY0` (DMG-compat CPU mode), write `$01` to `OPRI ($FF6C)` (DMG OBJ priority), install compatibility palettes; write DMG logo tilemap if palette ID is `$43` or `$58`.
8. `ld a,$11`; `ldh [$FF50],a`; → `$0100`.

CGB0 omits wave-RAM init and writes debug copies to unused WRAM.

### 1.5 CGB auto-colorization palette ID (for DMG games)

```
if old_licensee($014B) == $33:
    nintendo = (new_licensee($0144,$0145) == "01")     // ASCII 30 31
else:
    nintendo = (old_licensee == $01)
if !nintendo: palette_id = $00
else:
    title_chk = sum(mem[$0134..=$0143]) & $FF          // 16 bytes, 8-bit sum
    idx = lookup(title_chk in checksum_table)           // 0..N
    if not found: palette_id = $00
    elif idx <= 64: palette_id = idx
    else:
        row = lookup(mem[$0137] /*4th title letter*/ in letter_table)  // table of rows
        if not found: palette_id = $00
        else: palette_id = idx + 14 * row
```
B register handoff value mirrors `title_chk` when Nintendo, else `$00` (see §2 CGB-DMG-mode notes). Full checksum/letter tables: SameBoy `cgb_boot.asm` / TCRF.

### 1.6 Open-source boot ROM option (licensing)

**Recommendation: SameBoy boot ROMs** (`LIJI32/SameBoy`, `BootROMs/`). Clean-room reimplementations of DMG/MGB/SGB/SGB2/CGB/AGB boot ROMs in RGBASM, **MIT-licensed**. Drop-in: produce byte-accurate logo scroll, ding, checks, and identical handoff registers. Alternative: **bootix** (`Hacktix/Bootix`, `dmg_boot.bin`/`cgb_boot.bin`) — MIT, DMG+CGB, also clean-room. Both avoid Nintendo copyright on the original masked ROMs.

- REVENANT integration: ship SameBoy/bootix blobs embedded (`include_bytes!`) and gate behind a build flag; OR skip boot ROM entirely and apply the §2 post-boot state directly (recommended default for "just run the game"). Do **not** ship Nintendo's original boot ROM. The Nintendo *logo bytes* in cart headers are legally reproducible per *Sega v. Accolade* (US) as necessary for interoperability.

---

## 2. Post-Boot State (run games WITHOUT a boot ROM)

All values are at `PC=$0100`. When skipping the boot ROM, set CPU registers and the full I/O map below, **and** set `boot_rom_mapped = false`. VRAM/OAM contents: zeroed by real boot ROM (logo tiles aside) — emulators commonly zero VRAM; WRAM/HRAM are random on HW (init to `$00` and offer a "break on uninit read" option).

### 2.1 CPU registers

| Reg | DMG0 | DMG | MGB | SGB | SGB2 | CGB | AGB |
|---|---|---|---|---|---|---|---|
| **A** | `$01` | `$01` | `$FF` | `$01` | `$FF` | `$11` | `$11` |
| **F** | `0000` | `Z 0 H? C?`¹ | `Z 0 H? C?`¹ | `0000` | `0000` | `Z000`=`$80` | `0000` |
| **B** | `$FF` | `$00` | `$00` | `$00` | `$00` | `$00` | `$01` |
| **C** | `$13` | `$13` | `$13` | `$14` | `$14` | `$00` | `$00` |
| **D** | `$00` | `$00` | `$00` | `$00` | `$00` | `$FF` | `$FF` |
| **E** | `$C1` | `$D8` | `$D8` | `$00` | `$00` | `$56` | `$56` |
| **H** | `$84` | `$01` | `$01` | `$C0` | `$C0` | `$00` | `$00` |
| **L** | `$03` | `$4D` | `$4D` | `$60` | `$60` | `$0D` | `$0D` |
| **SP** | `$FFFE` | `$FFFE` | `$FFFE` | `$FFFE` | `$FFFE` | `$FFFE` | `$FFFE` |
| **PC** | `$0100` | `$0100` | `$0100` | `$0100` | `$0100` | `$0100` | `$0100` |

¹ **DMG/MGB F flags**: `Z=1, N=0`. If header checksum byte computation result `mem[$014D]` path yields header-checksum `== $00` then `H=0, C=0`, else `H=1, C=1`. Concretely: `F = $80` if checksum byte is `$00`, else `F = $B0`. The common "any normal game" value is `F = $B0` (AF = `$01B0`). The frequently-cited DMG `AF=$01B0, BC=$0013, DE=$00D8, HL=$014D, SP=$FFFE, PC=$0100` corresponds to a valid-checksum cart.

**CGB DMG-compatibility mode** (running a DMG game on CGB, boot skipped): `A=$11, F=$80, B=` title checksum (or `$00` if non-Nintendo), `C=$00, D=$00, E=$08`, `HL = $991A` if `B∈{$43,$58}` else `$007C`. AGB-DMG: same but `B` incremented by 1 and `F.Z` follows `inc b` result.

**Most-common single-value set to hardcode** (valid-checksum carts):
- DMG: `AF=$01B0 BC=$0013 DE=$00D8 HL=$014D SP=$FFFE PC=$0100`
- CGB (CGB game): `AF=$1180 BC=$0000 DE=$FF56 HL=$000D SP=$FFFE PC=$0100`

### 2.2 I/O register post-boot values

| Reg | Addr | DMG0 | DMG/MGB | SGB/SGB2 | CGB/AGB |
|---|---|---|---|---|---|
| P1/JOYP | FF00 | `$CF` | `$CF` | `$C7`/`$CF` | `$C7`/`$CF` |
| SB | FF01 | `$00` | `$00` | `$00` | `$00` |
| SC | FF02 | `$7E` | `$7E` | `$7E` | `$7F` |
| DIV | FF04 | `$18` | `$AB` | —² | —³ |
| TIMA | FF05 | `$00` | `$00` | `$00` | `$00` |
| TMA | FF06 | `$00` | `$00` | `$00` | `$00` |
| TAC | FF07 | `$F8` | `$F8` | `$F8` | `$F8` |
| IF | FF0F | `$E1` | `$E1` | `$E1` | `$E1` |
| NR10 | FF10 | `$80` | `$80` | `$80` | `$80` |
| NR11 | FF11 | `$BF` | `$BF` | `$BF` | `$BF` |
| NR12 | FF12 | `$F3` | `$F3` | `$F3` | `$F3` |
| NR13 | FF13 | `$FF` | `$FF` | `$FF` | `$FF` |
| NR14 | FF14 | `$BF` | `$BF` | `$BF` | `$BF` |
| NR21 | FF16 | `$3F` | `$3F` | `$3F` | `$3F` |
| NR22 | FF17 | `$00` | `$00` | `$00` | `$00` |
| NR23 | FF18 | `$FF` | `$FF` | `$FF` | `$FF` |
| NR24 | FF19 | `$BF` | `$BF` | `$BF` | `$BF` |
| NR30 | FF1A | `$7F` | `$7F` | `$7F` | `$7F` |
| NR31 | FF1B | `$FF` | `$FF` | `$FF` | `$FF` |
| NR32 | FF1C | `$9F` | `$9F` | `$9F` | `$9F` |
| NR33 | FF1D | `$FF` | `$FF` | `$FF` | `$FF` |
| NR34 | FF1E | `$BF` | `$BF` | `$BF` | `$BF` |
| NR41 | FF20 | `$FF` | `$FF` | `$FF` | `$FF` |
| NR42 | FF21 | `$00` | `$00` | `$00` | `$00` |
| NR43 | FF22 | `$00` | `$00` | `$00` | `$00` |
| NR44 | FF23 | `$BF` | `$BF` | `$BF` | `$BF` |
| NR50 | FF24 | `$77` | `$77` | `$77` | `$77` |
| NR51 | FF25 | `$F3` | `$F3` | `$F3` | `$F3` |
| NR52 | FF26 | `$F1` | `$F1` | `$F0` | `$F1` |
| WaveRAM | FF30–FF3F | — | uninit (CGB0: uninit; CGB inits) | | |
| LCDC | FF40 | `$91` | `$91` | `$91` | `$91` |
| STAT | FF41 | `$81` | `$85` | —² | —³ |
| SCY | FF42 | `$00` | `$00` | `$00` | `$00` |
| SCX | FF43 | `$00` | `$00` | `$00` | `$00` |
| LY | FF44 | `$91` | `$00` | —² | —³ |
| LYC | FF45 | `$00` | `$00` | `$00` | `$00` |
| DMA | FF46 | `$FF` | `$FF` | `$FF` | `$00` |
| BGP | FF47 | `$FC` | `$FC` | `$FC` | `$FC` |
| OBP0 | FF48 | uninit⁴ | uninit⁴ | uninit⁴ | uninit⁴ |
| OBP1 | FF49 | uninit⁴ | uninit⁴ | uninit⁴ | uninit⁴ |
| WY | FF4A | `$00` | `$00` | `$00` | `$00` |
| WX | FF4B | `$00` | `$00` | `$00` | `$00` |
| KEY0 | FF4C | — | — | — | —² |
| KEY1 | FF4D | — | — | — | `$7E`⁵ |
| VBK | FF4F | — | — | — | `$FE`⁵ |
| BANK | FF50 | (unmapped→`$FF`) | `$FF` | `$FF` | `$FF` |
| HDMA1 | FF51 | — | — | — | `$FF`⁵ |
| HDMA2 | FF52 | — | — | — | `$FF`⁵ |
| HDMA3 | FF53 | — | — | — | `$FF`⁵ |
| HDMA4 | FF54 | — | — | — | `$FF`⁵ |
| HDMA5 | FF55 | — | — | — | `$FF`⁵ |
| RP | FF56 | — | — | — | `$3E`⁵ |
| BCPS | FF68 | — | — | — | compat⁶ |
| BCPD | FF69 | — | — | — | compat⁶ |
| OCPS | FF6A | — | — | — | compat⁶ |
| OCPD | FF6B | — | — | — | compat⁶ |
| OPRI | FF6C | — | — | — | `$01` (DMG mode) |
| SVBK | FF70 | — | — | — | `$F8`⁵ |
| IE | FFFF | `$00` | `$00` | `$00` | `$00` |

² SGB: boot duration header-dependent → DIV/STAT/LY non-deterministic; don't rely. ³ CGB: duration depends on header (and user palette input) → DIV/STAT/LY/KEY0 non-deterministic. ⁴ OBP0/OBP1 left fully uninitialized (commonly `$00` or `$FF`; never rely). ⁵ CGB-only regs: read `$FF` in Non-CGB mode. ⁶ BCPS/BCPD/OCPS/OCPD depend on whether DMG-compat mode is active.

Note `DIV=$AB` on DMG: when skipping boot, seed the internal 16-bit DIV counter so its high byte reads `$AB` (i.e. counter ≈ `$ABCC`); this matters for timer/serial sub-clock phase in some test ROMs. `STAT=$85` reflects mode/coincidence at handoff (LY=0, mode 1, LYC=0 → coincidence set).

---

## 3. OAM DMA (`FF46`)

### 3.1 Register and transfer

| Field | Value |
|---|---|
| Register | `DMA` = `$FF46`, R/W (reads back last written page) |
| Source | `$XX00–$XX9F`, where `XX` = written byte; valid `$00–$DF` (Pan Docs). HW will accept `$00–$FF`; `$E0–$FF` reads echo/OAM/IO region (some emus mask to `$DF`). |
| Destination | `$FE00–$FE9F` (160 bytes, 40 objects × 4) |
| Duration (active copy) | **160 M-cycles = 640 T-cycles** = 640 dots normal speed (1.4 scanlines); 320 dots in CGB double-speed |
| Start delay | **1 M-cycle (4 T-cycles)** after the write completes before first byte copies |
| Total wall time | write-cycle + 4 T setup + 640 T copy = **644 T-cycles** from the `ldh [$FF46]` to completion |
| Copy rate | 1 byte / M-cycle (4 T) |

### 3.2 State machine (per M-cycle tick)

```
enum DmaState { Idle, Starting, Active }
struct OamDma { state, page: u8, index: u8 /*0..=159*/, restart_pending: Option<u8> }

on write FF46 = val:
    page_pending = val
    // restart semantics:
    if state == Active or Starting:
        restart_pending = Some(val)   // current transfer keeps running this M-cycle
    else:
        state = Starting; page = val; index = 0

tick() each M-cycle (after CPU M-cycle executes):
    match state:
      Starting:                 // the 1-cycle delay
          state = Active; index = 0
          if let Some(p)=restart_pending { page=p; restart_pending=None }
      Active:
          oam[index] = bus_read_dma(page<<8 | index)   // see source bus below
          index += 1
          if index == 160:
              state = Idle
          if let Some(p)=restart_pending {             // restart latches AFTER finishing? -> see quirk
              ...
          }
```

**Restart quirk**: Writing `FF46` while a DMA is already running does **not** abort instantly. The running transfer's current byte still completes; a new transfer is then armed with its own 1-cycle startup. Net effect tested by ROMs: two overlapping DMAs cost the setup delay again. Model: on write during Active/Starting, set `restart_pending`; when the next M-cycle boundary applies it, reset `index=0, page=new, state=Starting` (re-incur 1 cycle). Conservative/passes Mooneye: treat a mid-transfer write as scheduling a fresh transfer that begins 1 M-cycle later, the old one continuing until then.

### 3.3 Bus conflict (the critical accuracy point)

During the **active copy** (not during the 1-cycle setup), the DMA owns the source bus:

| CPU access region during active DMA | DMG behavior | CGB behavior |
|---|---|---|
| HRAM `$FF80–$FFFE` | **OK** — only safe region | OK |
| `$FF00–$FF7F` I/O | OK (I/O bus separate) | OK |
| ROM `$0000–$7FFF` | **Conflict**: read returns the byte DMA is currently transferring (the source byte for current `index`); writes ignored | OK to read if DMA source is in WRAM (separate bus); conflict if same bus |
| VRAM `$8000–$9FFF` | Conflict (returns DMA byte) | bus-dependent |
| External RAM `$A000–$BFFF` | Conflict | bus-dependent |
| WRAM `$C000–$DFFF` | Conflict | OK to read if DMA source is ROM/SRAM (separate cart vs WRAM bus) |
| OAM `$FE00–$FE9F` | Reads return `$FF`/garbage; the PPU also can't read OAM properly | same |
| Echo `$E000–$FDFF` | Conflict (mirrors WRAM) | — |

DMG rule of thumb: **only HRAM and `$FF00–$FF7F` are reliably accessible**; everything else returns the in-flight DMA byte on read and ignores writes. This is why the DMA-start routine must live in HRAM and busy-wait there (Pan Docs `run_dma`, 160-cycle wait via `ld a,40 / dec a / jr nz`).

CGB has split cart-bus vs WRAM-bus: CPU may read the *other* bus during DMA. But `call`/interrupt/`push` touch the stack (usually WRAM) → still busy-wait in HRAM. Disable interrupts (`di`) or run DMA in VBlank handler — an interrupt during DMA pushes to stack and fetches a handler from ROM, both potentially conflicting.

### 3.4 PPU interaction during DMA

- DMA active during **Mode 2 (OAM scan)**: most PPU revisions read each object as off-screen → object hidden that line.
- DMA active during **Mode 3 (render)**: PPU reads whatever 16-bit word the DMA is currently writing → corrupted tile#/attributes for already-in-range objects → graphical glitch.
- Standard practice: run DMA in Mode 1 (VBlank). Mid-frame DMA (for >40 sprites) should wait for Mode 0 first so it cleanly overlaps the next two Mode-2 scans.

### 3.5 Source-byte read function

```
fn bus_read_dma(addr):
    // DMA reads ignore CPU access rights; reads even VRAM/locked OAM freely.
    // $E0..$FF page wraps: hardware reads from the corresponding echo/OAM/IO map,
    // but emulators commonly clamp source page to <= $DF.
    read_underlying_memory(addr)
```

---

## 4. DMG OAM Corruption Bug (optional, DMG/MGB/SGB only; CGB/AGB immune)

Trigger condition: a 16-bit register holds a value in `$FE00–$FEFF` *before* the operation **and** PPU is in **Mode 2 (OAM scan)**. Objects 0 and 1 (`$FE00`, `$FE04`, i.e. row 0) are never affected.

Two underlying bugs:
1. Any read/write to OAM (incl. `$FEA0–$FEFF`) during Mode 2 corrupts it.
2. `inc rr`/`dec rr` on BC/DE/HL/SP/PC while the reg is in `$FE00–$FEFF` triggers a stray OAM access (the 16-bit IDU drives the address bus even with no asserted R/W).

OAM = 20 rows × 8 bytes (16-bit words). During Mode 2 the PPU reads one row per M-cycle. Corruption pattern depends only on operation type (read/write/both) and the currently-accessed row — actual address/value irrelevant.

Per-instruction trigger counts:

| Instruction class | Effect |
|---|---|
| `inc rr`,`dec rr` (rr in OAM) | one **write** corruption |
| `ld [hli],a`,`ld [hld],a`,`ld a,[hli]`,`ld a,[hld]` (hl in OAM) | corrupts **twice**: memory access + the implied inc/dec write |
| `pop rr`, `ret` family | triggers **3×**: read, glitched write, read (no glitched write on last) |
| `push rr`, `call`, `rst`, interrupt dispatch | triggers **4×** (two real writes + two glitched from `dec sp`); one glitched coincides with a real write → effectively 3 writes |
| executing code from OAM (PC in OAM, fetches `$FF`=`rst $38`) | triggers twice (PC inc write + opcode read) |

**Write corruption** (row ≠ row 0):
```
row[0] = ((a ^ c) & (b ^ c)) ^ c   // a=orig word0 of row, b=word0 of prev row, c=word2 of prev row
row[1..=3] = prev_row[1..=3]        // last three words copied from preceding row
```
**Read corruption**: same row-copy, but `row[0] = b | (a & c)`.

**Read+write same M-cycle** (e.g. `ld [hli],a` extra access): if accessed row is **not** rows 0–3 and **not** the last row:
```
let A=word0 two rows before, B=word0 prev row (the corrupted one),
    C=word0 current row, D=word2 prev row;
prev_row[0] = (B & (A | C | D)) | (A & C & D)
current_row = prev_row (after its word0 fix)
two_rows_before = prev_row
```
Then a normal read corruption is additionally applied regardless.

Implementation: only run this when `model is DMG-family && ppu_mode == 2`. Gate behind a `oam_bug` feature flag (most games never hit it; Tetris/others rely on its absence). Mealybug/Mooneye have targeted tests.

---

## 5. Serial Link (`FF01` SB / `FF02` SC)

### 5.1 Registers

**`SB` = `$FF01`** (Serial transfer data, R/W): the 8-bit shift register. Holds outgoing byte before transfer; during transfer it is a blend of out/in bits; after transfer holds the fully received byte.

**`SC` = `$FF02`** (Serial transfer control, R/W):

| Bit | Name | Meaning |
|---|---|---|
| 7 | Transfer Enable/Start | `1` = transfer requested/in progress. HW clears to `0` at end of transfer. Read only bit 7 to poll completion. |
| 6 | (unused) | reads `1` |
| 5–2 | (unused) | read `1` |
| 1 | Clock Speed | **CGB only**. `0` = normal, `1` = fast (~256 kHz normal-speed). Reads `0` / ignored on DMG. |
| 0 | Clock Select | `0` = External clock (slave), `1` = Internal clock (master). |

Read masks: DMG `SC` reads `$7E | (bit7) | (bit0)` → unused bits return 1, bit1 reads 0 (post-boot `$7E`). CGB `SC` post-boot `$7F` (bit1 available).

### 5.2 Clock rates

DMG/Non-CGB: internal clock fixed **8192 Hz** → 8192 bits/s = 1024 B/s. One bit shifts every 4096 M-cycles? — precisely: 8192 Hz from 4.194304 MHz → period = 512 T-cycles per bit = **128 M-cycles per bit**, **4096 T-cycles (1024 M-cycles) per full byte**.

| SC.1 | CPU speed | Bit clock | Byte rate | Bit period (T) |
|---|---|---|---|---|
| 0 | Normal | 8192 Hz | 1 KB/s | 512 |
| 0 | CGB 2× | 16384 Hz | 2 KB/s | 256 |
| 1 | Normal | 262144 Hz | 32 KB/s | 16 |
| 1 | CGB 2× | 524288 Hz | 64 KB/s | 8 |

Internal serial clock is derived from the DIV/system counter (8192 Hz = DIV bit 8 falling edge in normal speed) — matters for serial-timing test ROMs.

### 5.3 Shift behavior (8 shifts)

Each clock edge: leftmost bit (bit 7) shifts out onto the wire; the incoming bit shifts into bit 0.

| State | b7 b6 b5 b4 b3 b2 b1 b0 |
|---|---|
| init | o7 o6 o5 o4 o3 o2 o1 o0 |
| after 1 | o6 o5 o4 o3 o2 o1 o0 i7 |
| after 4 | o3 o2 o1 o0 i7 i6 i5 i4 |
| after 8 | i7 i6 i5 i4 i3 i2 i1 i0 |

After 8 shifts: SB = fully received byte, SC.7 cleared, **Serial interrupt** requested.

### 5.4 Serial interrupt

- IF bit 3 (`$FF0F` bit 3), IE bit 3, **vector `$0058`**, priority 4 (after VBlank, STAT, Timer).
- Master: load SB, write `SC=$81`. On completion SC.7→0 and IRQ fires.
- Slave (external clock): load SB, write `SC=$80`; transfer completes only when 8 external clock pulses received; then IRQ fires, SC.7→0.

### 5.5 State machine

```
enum Serial { Idle, Transferring{ bits_left:u8, counter:u32 } }

on write SC:
    if bit7 set:
        if bit0 == 1 (internal): start internal-clocked transfer
        else (external): arm; wait for external clock pulses
write SB: update shift register (only meaningful before/between transfers)

internal tick (per T-cycle, when Transferring + internal clock):
    counter -= 1
    if counter == 0:
        // one bit period elapsed
        out_bit = SB & 0x80
        in_bit  = peer_bit_or_1()        // disconnected master reads 1 -> receives $FF
        SB = (SB << 1) | in_bit
        push out_bit to peer
        bits_left -= 1
        counter = bit_period(SC, speed)
        if bits_left == 0:
            SC &= ~0x80; request_irq(SERIAL); Serial=Idle

external: only advance on incoming peer clock edges; if no peer, never completes
          (game must use a timeout). On disconnect, input line pulls to 1 over ~20µs.
```

Disconnected-master detail: input reads `1`, so a master with no peer receives `$FF` bytes each transfer (used for link detection). On mid-transfer disconnect the input is pulled up to 1 over ~20 µs (measured CGB rev E) — a 0 being received can read as 0 for up to 20 µs (>1 byte at top CGB speed).

### 5.6 Two-emulator link protocol (Pokémon trade-grade)

To let two REVENANT instances trade, emulate the physical link as a bidirectional bit/byte channel with explicit master/slave roles:

**Physical model**: connect two SC/SB units. Exactly one side has SC.0=1 (internal/master) and drives the clock. Per bit, both sides simultaneously present their `SB.7` on their TX line and sample the peer's TX line into `SB.0`. The master's internal clock paces all 8 shifts; the slave shifts on the clock it receives.

**Practical byte-exchange transport** (TCP/IPC):
```
Sync point = one full byte exchange.
- Master side: when SC=$81 written, send local SB byte to peer, block until peer's byte arrives,
  then set local SB = peer_byte, clear SC.7, raise serial IRQ.
- Slave side: when SC=$80 written, register "ready" + local SB byte with the link;
  when master initiates, the link swaps the two bytes; slave sets SB=peer_byte,
  clears SC.7, raises IRQ.
- The link layer must rendezvous: a transfer only resolves when BOTH sides have a byte staged
  (master via $81, slave via $80). If only the master is ready, master keeps clocking and
  receives $FF (or stalls per chosen fidelity); real HW: master receives whatever the slave's
  SB currently holds, or last byte if slave hasn't reloaded.
```
**Pokémon Gen-1/2 trade handshake** rides on this raw byte channel: the games implement their own application protocol on top (master = the GB that initiated). Sequence the emulator must faithfully relay (it does **not** parse this — it just exchanges bytes losslessly and timely):
1. Both enter Cable Club; one becomes master (SC.0=1), other slave.
2. Continuous byte exchange of sync/preamble bytes (e.g. `$FD` "PREAMBLE"/idle, then a determination byte to pick master/slave: master sends `$01`, slave sends `$02`; if both send same they retry).
3. Exchange of the "random seed"/RNG-sync bytes.
4. Exchange of the full trade block (party data: 415 bytes Gen 1, terminated by `$FD` separators) — streamed byte-by-byte, each byte one SC transfer.
5. Patch list, then the chosen-mon selection bytes, then a confirmation byte exchange; trade commits when both confirm.

Implementation requirements for correctness:
- **Lossless, in-order** byte relay; never drop or reorder.
- **Latency tolerance**: the GB master delay between transfers (Pan Docs: master inserts a small delay after each byte so the slave can reload SB) means the transport may block; that's fine — the receiving GB "eagerly waits."
- **Timeout**: if peer absent, master receives `$FF` and games abort via their own timeout — so a missing peer should yield `$FF` after the bit-period elapses, not a hang.
- **Clock-role honoring**: only the master's writes advance the shared clock; a two-master or two-slave pairing exchanges nothing (matches HW).

---

## 6. Joypad (`FF00` P1/JOYP)

### 6.1 Register layout

| Bit | Name | R/W | Meaning |
|---|---|---|---|
| 7 | — | — | reads `1` |
| 6 | — | — | reads `1` |
| 5 | Select Buttons | W | `0` = select action buttons (Start/Select/B/A) onto bits 3–0 |
| 4 | Select D-pad | W | `0` = select direction keys (Down/Up/Left/Right) onto bits 3–0 |
| 3 | Down / Start | R | `0` = pressed |
| 2 | Up / Select | R | `0` = pressed |
| 1 | Left / B | R | `0` = pressed |
| 0 | Right / A | R | `0` = pressed |

- Inputs are **active-low**: pressed = `0`, released = `1`.
- Bits 3–0 are read-only; bits 5–4 are write-only-effective selects; bits 7–6 always read `1`.
- If both selects = `1` (wrote `$30`): low nibble reads `$F` (all released).
- If both selects = `0`: low nibble = AND of both matrices (a key pressed in either set reads 0).

### 6.2 Read logic

```
fn read_p1() -> u8:
    let mut v = 0b1100_0000 | (p1_select & 0b0011_0000); // bits7,6=1; keep written selects
    let mut low = 0x0F;
    if (p1_select & 0x20) == 0 {  // action buttons selected
        low &= action_bits();     // bit3=Start,2=Select,1=B,0=A (0=pressed)
    }
    if (p1_select & 0x10) == 0 {  // d-pad selected
        low &= dpad_bits();       // bit3=Down,2=Up,1=Left,0=Right (0=pressed)
    }
    v | low
```
`action_bits()`/`dpad_bits()`: start with `$0F`, clear the bit for each currently-pressed button. Store only `p1_select = written_value & 0x30`.

### 6.3 Joypad interrupt

- IF bit 4 (`$FF0F` bit 4), IE bit 4, **vector `$0060`**, priority 5 (lowest).
- Fires on a **high→low transition of any of bits 3–0** of P1 *as currently selected* (i.e., a relevant button press while its group is selected). HW triggers on any selected line going `1→0`.
- Used to wake from STOP / low-power. Latch previous low-nibble; on each update, if `(prev & ~cur) & 0x0F != 0` request joypad IRQ. Because most games keep selects changing, common practice (and HW) is edge on the effective combined nibble.
- Quirk: programs read P1 several times after switching selects (first reads = settle delay due to line capacitance); emulators can return the final stable value immediately, but multi-read patterns must not glitch.

### 6.4 SGB note

SGB games abuse P1 to send command packets to the SNES (writing select bits to clock out packet bits) and to read up to 4 multitap joypads. If REVENANT targets SGB, the P1 write path must also feed the SGB packet receiver; otherwise standard joypad behavior applies.

---

## 7. Interrupt vector / priority summary (cross-subsystem)

| IRQ | IF/IE bit | Vector | Priority | Source subsystem here |
|---|---|---|---|---|
| VBlank | 0 | `$0040` | 1 (highest) | — |
| LCD STAT | 1 | `$0048` | 2 | — |
| Timer | 2 | `$0050` | 3 | — |
| **Serial** | 3 | `$0058` | 4 | §5 |
| **Joypad** | 4 | `$0060` | 5 (lowest) | §6 |

Post-boot `IF=$E1` (= `1110_0001`: VBlank pending bit 0 set, bits 5–7 read 1), `IE=$00`.

---

## 8. Sources

- [Pan Docs — Power-Up Sequence](https://gbdev.io/pandocs/Power_Up_Sequence.html)
- [Pan Docs — OAM DMA Transfer](https://gbdev.io/pandocs/OAM_DMA_Transfer.html)
- [Pan Docs — Serial Data Transfer (Link Cable)](https://gbdev.io/pandocs/Serial_Data_Transfer_(Link_Cable).html)
- [Pan Docs — Joypad Input](https://gbdev.io/pandocs/Joypad_Input.html)
- [Pan Docs — OAM Corruption Bug](https://gbdev.io/pandocs/OAM_Corruption_Bug.html)
- [gb-bootroms disassembly (ISSOtm / codeberg)](https://codeberg.org/ISSOtm/gb-bootroms)
- [GBEDG — DMA timing](https://github.com/Hacktix/GBEDG/blob/master/dma/index.md)
- [Mooneye-GB (Gekkio) acceptance tests](https://github.com/Gekkio/mooneye-gb)
- [SameBoy (MIT boot ROMs)](https://github.com/LIJI32/SameBoy)
- [bootix (Hacktix, MIT DMG/CGB boot ROMs)](https://github.com/Hacktix/Bootix)
- [TCRF — Game Boy Color Bootstrap ROM palettes](https://tcrf.net/Notes:Game_Boy_Color_Bootstrap_ROM)

Reference files saved locally: `C:\Users\ChadE\Tester Bridgeming Goat\.firecrawl\powerup.md`, `oamdma.md`, `serial.md`, `joypad.md`, `oamcorrupt.md`, `gbedg-dma.md`.