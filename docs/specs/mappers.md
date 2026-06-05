# MBC mappers + RTC

I now have all authoritative details confirmed. I have everything needed: header layout/checksums, cartridge type table, ROM/RAM size tables, MBC1 (registers, modes, addressing diagrams, 0x20/0x40/0x60 quirk, MBC1m), MBC3 (RTC registers, latch, DH bit layout), MBC5 (9-bit ROM bank, 4-bit RAM bank, rumble). Producing the spec.

# REVENANT — Cartridge Mapper Specification

Reference for DMG/CGB cartridge ROM/RAM/RTC banking. All register addresses refer to the **Game Boy CPU address** that triggers the write (mappers decode only the high address bits; the low bits are don't-care unless noted). Writes to `$0000–$7FFF` are **register writes** (ROM is read-only); reads from `$0000–$7FFF` return mapped ROM; `$A000–$BFFF` is external RAM / RTC.

---

## 1. Cartridge Header (`$0100–$014F`)

| Range | Field | Notes |
|---|---|---|
| `$0100–$0103` | Entry point | Usually `00 C3 50 01` = `nop; jp $0150`. Boot ROM jumps to `$0100`. |
| `$0104–$0133` | Nintendo logo | 48 bytes, must match dump below. DMG/SGB check all 48; CGB checks only first `$18` (24) bytes. Mismatch → boot ROM locks up. |
| `$0134–$0143` | Title | Upper-case ASCII, `$00`-padded. On newer carts shortened to `$0134–$0142` (15) or `$0134–$013E` (11). |
| `$013F–$0142` | Manufacturer code | 4 ASCII (newer carts only; overlaps Title). |
| `$0143` | CGB flag | See §1.2. |
| `$0144–$0145` | New licensee code | 2 ASCII; valid only when `$014B == $33`. |
| `$0146` | SGB flag | `$03` = SGB functions supported; else SGB ignores command packets. |
| `$0147` | Cartridge type | See §1.3 (mapper select). |
| `$0148` | ROM size | See §1.4. |
| `$0149` | RAM size | See §1.5. |
| `$014A` | Destination | `$00`=Japan(+overseas), `$01`=Overseas only. |
| `$014B` | Old licensee code | `$33` ⇒ use new licensee code. SGB needs `$33` to accept packets. |
| `$014C` | Mask ROM version | Usually `$00`. |
| `$014D` | Header checksum | See §1.6. Boot ROM verifies; mismatch → lock up. |
| `$014E–$014F` | Global checksum | Big-endian. See §1.7. Not verified by boot ROM (except on original SGB/early units, generally ignored). |

**Nintendo logo dump (48 bytes):**
```
CE ED 66 66 CC 0D 00 0B 03 73 00 83 00 0C 00 0D
00 08 11 1F 88 89 00 0E DC CC 6E E6 DD DD D9 99
BB BB 67 63 6E 0E EC CC DD DC 99 9F BB B9 33 3E
```

### 1.2 CGB Flag `$0143`
Only meaningful on CGB hardware. Bit 7 = "CGB-aware". Bit 6 is ignored by hardware.

| Value | Meaning |
|---|---|
| `$80` | CGB-enhanced, DMG-compatible |
| `$C0` | CGB-only (bit 6 ignored ⇒ behaves like `$80`) |
| other (bit7=0) | DMG game; CGB runs in monochrome-compat (Non-CGB) mode |

If bit 7 set, the byte value is written to `KEY0` (`$FF4C`) by boot, selecting CPU mode. Decode rule: `cgb_mode = (val & 0x80) != 0` (treat `$80` and `$C0` identically). Note: the low 2 bits being nonzero (`$80|$04` style "PGB"/debug) is out of scope; treat any value with bit7 set as CGB-aware.

### 1.3 Cartridge Type `$0147` (full table)

| `$0147` | Type | MBC | RAM | Batt | RTC | Rumble |
|---|---|---|---|---|---|---|
| `$00` | ROM ONLY | none | – | – | – | – |
| `$01` | MBC1 | MBC1 | – | – | – | – |
| `$02` | MBC1+RAM | MBC1 | ✓ | – | – | – |
| `$03` | MBC1+RAM+BATTERY | MBC1 | ✓ | ✓ | – | – |
| `$05` | MBC2 | MBC2 | int 512×4b | – | – | – |
| `$06` | MBC2+BATTERY | MBC2 | int 512×4b | ✓ | – | – |
| `$08` | ROM+RAM | none | ✓ | – | – | – |
| `$09` | ROM+RAM+BATTERY | none | ✓ | ✓ | – | – |
| `$0B` | MMM01 | MMM01 | – | – | – | – |
| `$0C` | MMM01+RAM | MMM01 | ✓ | – | – | – |
| `$0D` | MMM01+RAM+BATTERY | MMM01 | ✓ | ✓ | – | – |
| `$0F` | MBC3+TIMER+BATTERY | MBC3 | – | ✓ | ✓ | – |
| `$10` | MBC3+TIMER+RAM+BATTERY | MBC3 | ✓ | ✓ | ✓ | – |
| `$11` | MBC3 | MBC3 | – | – | – | – |
| `$12` | MBC3+RAM | MBC3 | ✓ | – | – | – |
| `$13` | MBC3+RAM+BATTERY | MBC3 | ✓ | ✓ | – | – |
| `$19` | MBC5 | MBC5 | – | – | – | – |
| `$1A` | MBC5+RAM | MBC5 | ✓ | – | – | – |
| `$1B` | MBC5+RAM+BATTERY | MBC5 | ✓ | ✓ | – | – |
| `$1C` | MBC5+RUMBLE | MBC5 | – | – | – | ✓ |
| `$1D` | MBC5+RUMBLE+RAM | MBC5 | ✓ | – | – | ✓ |
| `$1E` | MBC5+RUMBLE+RAM+BATTERY | MBC5 | ✓ | ✓ | – | ✓ |
| `$20` | MBC6 | MBC6 | ✓ | ✓ | – | – |
| `$22` | MBC7+SENSOR+RUMBLE+RAM+BATTERY | MBC7 | EEPROM | ✓ | – | ✓ |
| `$FC` | POCKET CAMERA | Camera | ✓ | ✓ | – | – |
| `$FD` | BANDAI TAMA5 | TAMA5 | – | ✓ | ✓ | – |
| `$FE` | HuC3 | HuC3 | ✓ | ✓ | ✓ | – |
| `$FF` | HuC1+RAM+BATTERY | HuC1 | ✓ | ✓ | IR | – |

Note: `$10` (MBC3+TIMER+RAM+BATTERY) and `$12`/`$13` on real carts are frequently **MBC30** (a superset: up to 4 MiB ROM / 64 KiB RAM); REVENANT should treat MBC3 with RAM size up to `$05` (64 KiB) as MBC30-extended (see §3.6).

### 1.4 ROM Size `$0148`
General rule: `bytes = 32 KiB << value`; `banks = 2 << value` (each bank = 16 KiB).

| Val | Size | Banks | Bank# bits |
|---|---|---|---|
| `$00` | 32 KiB | 2 | 1 (no MBC banking) |
| `$01` | 64 KiB | 4 | 2 |
| `$02` | 128 KiB | 8 | 3 |
| `$03` | 256 KiB | 16 | 4 |
| `$04` | 512 KiB | 32 | 5 |
| `$05` | 1 MiB | 64 | 6 |
| `$06` | 2 MiB | 128 | 7 |
| `$07` | 4 MiB | 256 | 8 |
| `$08` | 8 MiB | 512 | 9 |
| `$52` | 1.1 MiB | 72 | (rare/legacy) |
| `$53` | 1.2 MiB | 80 | (rare/legacy) |
| `$54` | 1.5 MiB | 96 | (rare/legacy) |

Implementation: derive `num_banks` from `$0148`; `bank_mask = num_banks - 1` (valid because bank counts are powers of two for `$00–$08`). The mask is applied to the **effective** bank number (post-remap) to wrap out-of-range accesses (§2.5). `$52/$53/$54` are believed bogus; treat `num_banks` from actual file length if mismatched.

### 1.5 RAM Size `$0149`

| Val | Size | Banks (8 KiB each) |
|---|---|---|
| `$00` | 0 | 0 (no RAM) |
| `$01` | unused (2 KiB legacy/PD) | treat as 0 unless file dictates |
| `$02` | 8 KiB | 1 |
| `$03` | 32 KiB | 4 |
| `$04` | 128 KiB | 16 |
| `$05` | 64 KiB | 8 |

MBC2 ignores this field (512×4-bit internal). For carts whose type has no "RAM", this should be `$00`.

### 1.6 Header Checksum `$014D` (exact algorithm)
```c
uint8_t checksum = 0;
for (uint16_t a = 0x0134; a <= 0x014C; a++)   // inclusive, 25 bytes
    checksum = checksum - rom[a] - 1;          // 8-bit wraparound
// stored value must equal rom[0x014D]
```
Rust: `let mut c: u8 = 0; for a in 0x0134..=0x014C { c = c.wrapping_sub(rom[a]).wrapping_sub(1); }`

### 1.7 Global Checksum `$014E–$014F`
16-bit big-endian sum of **all** ROM bytes **except** the two checksum bytes themselves:
```c
uint16_t sum = 0;
for (size_t i = 0; i < rom_len; i++)
    if (i != 0x014E && i != 0x014F) sum += rom[i];
// stored: rom[0x014E]=high byte, rom[0x014F]=low byte
```
Not validated by boot ROM on retail units; compute/verify only for diagnostics.

---

## 2. MBC1 (max 2 MiB ROM / 32 KiB RAM)

### 2.1 Registers (all write-only; reset to `$00` on power-up)

| Range | Reg | Width | Function |
|---|---|---|---|
| `$0000–$1FFF` | RAMG (RAM enable) | 4-bit cmp | `(val & 0x0F) == 0x0A` → RAM enabled; any other → disabled |
| `$2000–$3FFF` | BANK1 (ROM bank low) | 5 bits | `val & 0x1F`; if result `== 0` it is forced to `1` |
| `$4000–$5FFF` | BANK2 (secondary) | 2 bits | `val & 0x03` |
| `$6000–$7FFF` | MODE | 1 bit | `val & 0x01`; 0=simple, 1=advanced |

The ROM-bank register is `$00`-on-reset but **treated as `$01`** (cannot select bank 0 there).

### 2.2 Effective bank composition
```
rom_bank_4000_7FFF = (BANK2 << 5) | (BANK1 == 0 ? 1 : BANK1)
```
- The `==0 → 1` translation inspects the **full 5-bit BANK1** value only; BANK2 bits are **not** part of the zero-check.
- Final bank is masked: `rom_bank & bank_mask` (§1.4).

### 2.3 Address → ROM/RAM mapping (canonical diagrams)
Effective ROM address = `(bank << 14) | (cpu_addr & 0x3FFF)`.

**`$0000–$3FFF` read:**
| MODE | Bank used |
|---|---|
| 0 (simple) | bank `$00` |
| 1 (advanced) | `BANK2 << 5` (i.e. `$00/$20/$40/$60`), masked |

**`$4000–$7FFF` read (both modes):**
- bank = `(BANK2 << 5) | (BANK1==0?1:BANK1)`, masked.

**`$A000–$BFFF` RAM (only if RAMG enabled):**
| MODE | RAM bank |
|---|---|
| 0 | bank `0` |
| 1 | `BANK2` (0–3) |
- RAM offset = `(ram_bank << 13) | (cpu_addr & 0x1FFF)`, ram_bank masked to available banks. If RAM disabled: reads = open bus (return `$FF`), writes ignored.

### 2.4 The `$20/$40/$60` quirk (standard ≥1 MiB carts)
Because BANK1 contributes bits 0–4 and BANK2 contributes bits 5–6, and the `==0→1` fixup runs on BANK1 alone, you **cannot** select banks `$20`, `$40`, `$60` through the `$4000–$7FFF` window: writing BANK1=`$00` with BANK2=`1/2/3` yields effective `$21/$41/$61` (the fixup bumps the low 5 bits from 0 to 1). Banks `$00/$20/$40/$60` are reachable **only** at `$0000–$3FFF` and **only in MODE 1**. Mealybug/Mooneye `bits_bank1`, `bits_bank2`, `bits_mode` test exactly this composition and the fixup.

### 2.5 Large-ROM vs large-RAM mutual exclusivity
- **Large ROM (≥1 MiB):** BANK2 is wired as ROM bank bits 5–6. RAM is fixed at ≤8 KiB (single bank); MODE only affects which `$0000–$3FFF`/`$4000–$7FFF` banks are visible. RAM banking via BANK2 is not available.
- **Large RAM (32 KiB) carts (ROM ≤512 KiB):** BANK2 is wired to RAM bank select (0–3) in MODE 1. ROM ≤512 KiB needs only 5 bits, so BANK2 doesn't extend ROM.
- On small carts (ROM ≤512 KiB **and** RAM ≤8 KiB) MODE has **no observable effect**.

### 2.6 Sub-5-bit ROM and the "bank 0 in `$4000`" trick
The full 5-bit BANK1 is always used for the `==0→1` fixup even if fewer bits address the ROM. On a ≤256 KiB cart (≤4 addressing bits) writing BANK1=`$10` makes the fixup see `$10` (≠0, so no bump) while the addressing bits (0–3) are all 0 → **bank `$00` becomes selectable at `$4000–$7FFF`**. Emulate by doing the `==0` check on the raw 5-bit value, then masking afterwards.

### 2.7 MBC1M (multicart, 1 MiB / 4×256 KiB compilations)
Detection (heuristic): cart type `$01–$03`, ROM size `$05` (1 MiB), **and** a valid Nintendo logo present in bank `$10` (verify the 48/24-byte logo at `bank0x10_offset + 0x0104`). Bad dumps duplicate `$10–$1F`≡`$00–$0F`.

Wiring difference: BANK1 top bit (bit 4) is **ignored** for addressing (effective 4-bit ROM bank), but the full 5-bit value still drives the `==0→1` fixup. BANK2 is shifted to bits **4–5** instead of 5–6:
```
rom_bank = (BANK2 << 4) | (BANK1 & 0x0F == 0 ? ... )   // see below
```
Composition (MBC1M):
- `$4000–$7FFF`: `bank = (BANK2 << 4) | (BANK1 & 0x0F)`, with the `==0→1` fixup applied to the full BANK1 (so BANK1=`$00`→`$01`); bit 4 of BANK1 unused.
- `$0000–$3FFF`: MODE 0 → bank `$00`; MODE 1 → `BANK2 << 4` (selects game base `$00/$10/$20/$30`).
- BANK2 selects the game (0–3 → base banks `$00/$10/$20/$30`); each sub-game then drives only BANK1 as if on a 256 KiB cart.

### 2.8 Implementation skeleton (MBC1)
State: `ramg:bool, bank1:u8(5), bank2:u8(2), mode:u8(1), rom_bank_mask, ram_bank_mask, multicart:bool`.
- write `$0000–$1FFF`: `ramg = (v & 0x0F)==0x0A`
- write `$2000–$3FFF`: `bank1 = v & 0x1F`
- write `$4000–$5FFF`: `bank2 = v & 0x03`
- write `$6000–$7FFF`: `mode = v & 0x01`
- read `$0000–$3FFF`: bank = `mode==1 ? (bank2<<(multicart?4:5)) : 0`
- read `$4000–$7FFF`: `b1 = bank1==0?1:bank1`; bank = `multicart ? ((bank2<<4)|(b1&0x0F)) : ((bank2<<5)|b1)`; `bank &= rom_bank_mask`
- RAM bank = `(mode==1 && !large_rom) ? bank2 : 0`, masked.

---

## 3. MBC3 (max 2 MiB ROM / 32 KiB RAM + RTC; MBC30 up to 4 MiB / 64 KiB)

### 3.1 Registers (write-only)

| Range | Reg | Width | Function |
|---|---|---|---|
| `$0000–$1FFF` | RAMG + Timer enable | – | `(val & 0x0F) == 0x0A` enables RAM **and** RTC register access; else disables |
| `$2000–$3FFF` | ROM bank | 7 bits | `val & 0x7F`; if `0` → forced to `1`. (MBC30: full 8 bits, `val`) |
| `$4000–$5FFF` | RAM bank / RTC select | – | see table below |
| `$6000–$7FFF` | Latch clock | – | `$00` then `$01` sequence latches RTC (§3.4) |

### 3.2 `$4000–$5FFF` selection

| Value | Maps `$A000–$BFFF` to |
|---|---|
| `$00–$03` | RAM bank 0–3 (`$00–$07` for MBC30 / 64 KiB) |
| `$08` | RTC S (seconds) |
| `$09` | RTC M (minutes) |
| `$0A` | RTC H (hours) |
| `$0B` | RTC DL (day low) |
| `$0C` | RTC DH (day high + flags) |

Only the low bits relevant to the selected mode are decoded; many ROMs write `$08–$0C` to map RTC. The selector latches whichever was last written; `$A000–$BFFF` then routes to RAM **or** the chosen RTC register.

### 3.3 ROM/RAM addressing
- `$0000–$3FFF`: fixed bank `$00`.
- `$4000–$7FFF`: `bank = (rom_bank==0?1:rom_bank) & rom_bank_mask` (MBC3 supports `$20/$40/$60` directly — no MBC1 quirk).
- `$A000–$BFFF` with selector `$00–$07` and RAMG: `ram[(ram_bank<<13)|(addr&0x1FFF)]`.
- `$A000–$BFFF` with selector `$08–$0C` and RAMG: returns the **latched** RTC register value (reads use latched copy, not live counter); writes go to the **live** register (and you should mirror to latched per common behavior — but canonical: writes set the live RTC register).

### 3.4 RTC registers and bit layout

| Sel | Reg | Range | Bits |
|---|---|---|---|
| `$08` | S | 0–59 (`$00–$3B`) | seconds |
| `$09` | M | 0–59 (`$00–$3B`) | minutes |
| `$0A` | H | 0–23 (`$00–$17`) | hours |
| `$0B` | DL | `$00–$FF` | day counter bits 0–7 |
| `$0C` | DH | – | bit0=day bit8; bit6=HALT(0=run,1=stop); bit7=DAY CARRY (sticky overflow); bits 1–5 unused (read as written/0) |

Day counter is 9 bits (0–511) across DL+DH.bit0. On increment past 511 → wraps to 0 **and** sets DH.bit7 (carry), which stays set until software clears it.

### 3.5 Latch sequence (`$6000–$7FFF`)
Latching copies the **live** counter into the **latch** registers that reads observe; the clock keeps ticking.
State machine (per write to `$6000–$7FFF`):
```
on write v:
  if last_latch_write == 0x00 && v == 0x01:
       copy live_rtc -> latched_rtc
  last_latch_write = v   // store low byte (or just track "was 0")
```
Only the exact `$00`→`$01` transition triggers a latch; other sequences (e.g. `$01`→`$01`, `$00`→`$00`) do not. Multiple latches re-copy.

### 3.6 Real-time ticking model
Maintain live RTC as a struct `{s,m,h,dl:u8, dh:u8}` plus a sub-second accumulator. Tick rule (drive from emulated clock or host wall time):
- Every 1 real second (when HALT=`0`): `s++`; at 60 → `s=0,m++`; m at 60 → `m=0,h++`; h at 24 → `h=0`, day=`(DH.bit0<<8)|DL`, `day++`; if day>511 → day=`day & 0x1FF` and set `DH.bit7`; write back DL=day&0xFF, DH.bit0=(day>>8)&1.
- HALT (DH.bit6=1) freezes all ticking.
- Out-of-range writes (e.g. S=`$3F`) are stored as-is and counted up from there until wrap (hardware does not clamp on write; only natural rollover normalizes). REVENANT should preserve raw written values and only normalize on tick. The "set HALT before writing" guidance is a software convention, not enforced by hardware.
- For wall-clock sync on load: compute elapsed real seconds since the saved RTC timestamp and advance the counter by that many ticks (respecting HALT and day-carry).

### 3.7 Implementation skeleton (MBC3)
State: `ramg, rom_bank:u8, map_select:u8, latch_state:u8, rtc_live, rtc_latched, rtc_subsec_accum`.
Reads `$A000–$BFFF`: if `map_select<=0x07` → RAM (if ramg); if `0x08..=0x0C` → `rtc_latched[sel]` (if ramg); else open bus `$FF`.
Writes `$A000–$BFFF`: route to RAM or `rtc_live[sel]` accordingly (only if ramg).

---

## 4. MBC5 (max 8 MiB ROM / 128 KiB RAM; first to be Double-Speed-safe)

### 4.1 Registers (write-only)

| Range | Reg | Width | Function |
|---|---|---|---|
| `$0000–$1FFF` | RAMG | 4-bit cmp | `(val & 0x0F)==0x0A` → enable; else disable |
| `$2000–$2FFF` | ROM bank low 8 | 8 bits | `rom_bank = (rom_bank & 0x100) \| val` |
| `$3000–$3FFF` | ROM bank bit 8 | 1 bit | `rom_bank = (rom_bank & 0x0FF) \| ((val & 1)<<8)` |
| `$4000–$5FFF` | RAM bank | 4 bits | `val & 0x0F`; bit 3 doubles as Rumble on rumble carts (§4.3) |

### 4.2 ROM/RAM addressing
- `$0000–$3FFF`: fixed bank `$00`.
- `$4000–$7FFF`: bank = `rom_bank & rom_bank_mask`. **Bank 0 is genuinely bank 0** here (writing `$000` gives ROM bank 0 in the high window — no `==0→1` remap, unlike MBC1/3). Full range `$000–$1FF` (9 bits).
- `$A000–$BFFF` (if RAMG): `ram[(ram_bank<<13)|(addr&0x1FFF)]`, ram_bank from `$4000–$5FFF` masked to available banks (0–15).

### 4.3 Rumble
On RUMBLE carts (`$1C–$1E`), **bit 3** of the `$4000–$5FFF` register is disconnected from RAM and drives the rumble motor: `1`=on, `0`=off (latched until changed). On those carts RAM bank uses only bits 0–2 (max 8 banks / 64 KiB effective). Intensity = software PWM (rapid toggling). Non-rumble MBC5 uses all 4 bits for RAM bank.

### 4.4 Implementation skeleton (MBC5)
State: `ramg, rom_bank:u16(9), ram_bank:u8(4), rumble:bool, has_rumble`.
- write `$2000–$2FFF`: `rom_bank = (rom_bank & 0x100) | v as u16`
- write `$3000–$3FFF`: `rom_bank = (rom_bank & 0x0FF) | (((v & 1) as u16)<<8)`
- write `$4000–$5FFF`: if `has_rumble { rumble = v & 0x08 != 0; ram_bank = v & 0x07 } else { ram_bank = v & 0x0F }`
- read `$4000–$7FFF`: bank = `rom_bank & rom_bank_mask`.

---

## 5. Battery Save Semantics & `.sav` Format

### 5.1 What persists
- External RAM contents (`$A000–$BFFF` banked), only if cart type has BATTERY.
- MBC3 RTC live state + real timestamp (TIMER+BATTERY types `$0F/$10`).
- MBC2: internal 512×4-bit RAM (lower nibble only meaningful; persist 512 bytes, high nibbles undefined/`$F`).

Write-back triggers: REVENANT should flush to disk on (a) RAM disable write transitioning enabled→disabled, (b) emulator shutdown/cart eject, and ideally (c) a debounced periodic flush. Real hardware auto-disables RAM at power loss.

### 5.2 Recommended `.sav` layout
Plain RAM dump for non-RTC carts (de-facto standard, BGB/VBA-compatible):
```
[0 .. ram_size)   raw external RAM bytes (ram_size from $0149)
```
MBC2: 512 bytes (one nibble used per byte).

For MBC3+RTC, append an RTC trailer after the RAM image (BGB/VBA-M-compatible **48-byte little-endian** format), so saves interop with mainstream emulators:
```
offset (after RAM):
  +0  u32  rtc_seconds      (S, 0..59)        little-endian, each field 4 bytes
  +4  u32  rtc_minutes      (M, 0..59)
  +8  u32  rtc_hours        (H, 0..23)
  +12 u32  rtc_days_low     (DL)
  +16 u32  rtc_days_high    (DH: bit0=day8, bit6=halt, bit7=carry)
  +20 u32  rtc_latched_seconds
  +24 u32  rtc_latched_minutes
  +28 u32  rtc_latched_hours
  +32 u32  rtc_latched_days_low
  +36 u32  rtc_latched_days_high
  +40 u64  unix_timestamp   (seconds since epoch, of last save)  little-endian
```
Total trailer = 48 bytes. Each RTC field stored as 32-bit LE even though hardware is 8-bit (only low 8 bits meaningful) for cross-emulator compatibility. On load, advance the clock by `(now - unix_timestamp)` seconds if HALT clear (§3.6).

### 5.3 REVENANT-native variant (optional, versioned)
If a richer container is wanted, wrap the above in a tagged header but keep the canonical RAM+48-byte-RTC blob importable/exportable:
```
magic   "RVNS"            4 bytes
version u16 LE            (=1)
flags   u16 LE            bit0=has_rtc
ram_len u32 LE
ram     [ram_len] bytes
rtc     [48] bytes        (present iff flags.has_rtc; identical layout to §5.2 trailer)
```
Always also support raw-`.sav` import/export (strip/append the 48-byte RTC trailer) for ecosystem compatibility. Endianness for all REVENANT multi-byte fields: little-endian.

### 5.4 Edge cases
- RAM-size `$00` battery carts (e.g. MBC3+TIMER+BATTERY `$0F`): no RAM image, write only the 48-byte RTC trailer.
- Loading a `.sav` whose length = `ram_size` (no trailer): treat RTC as fresh (zeroed, not halted) and timestamp = now.
- Loading length = `ram_size + 48`: parse trailer.
- Mask RAM reads/writes to `ram_size`; banks beyond `ram_bank_mask` wrap (mask = `num_ram_banks - 1`).

---

## 6. Test-ROM coverage checklist (acceptance)

| Quirk | Test source |
|---|---|
| MBC1 BANK1 5-bit, `==0→1`, masking | mooneye `emulator-only/mbc1/bits_bank1`, `rom_512kb`…`rom_2Mb` |
| MBC1 BANK2 composition + `$20/$40/$60` | mooneye `bits_bank2`, `bits_mode` |
| MBC1 MODE RAM/zero-bank switching | mooneye `ram_256kb`, `ram_64kb`, `multicart_rom_8Mb` |
| MBC1M base-bank wiring | mooneye `multicart_rom_8Mb` |
| MBC2 internal RAM nibble, `==0→1` | mooneye `mbc2/*` |
| MBC3 7-bit bank, RAM/RTC select, latch, day-carry/halt | mooneye `mbc3/*` (where present) + manual RTC checks |
| MBC5 9-bit bank, bank0=0, 4-bit RAM | mooneye `mbc5/*` |
| Header checksum lockup | boot-ROM behavior (Power-Up Sequence) |

Sources verified against Pan Docs (gbdev.io/pandocs: The Cartridge Header, MBC1, MBC3, MBC5) and Mooneye GB test-suite naming.