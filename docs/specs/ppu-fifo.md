# PPU pixel FIFO & timing

I have the authoritative numbers confirmed. Producing the spec now.

```markdown
# REVENANT PPU Pixel Pipeline — Implementation Spec (DMG/CGB)

## 0. Timebase & Frame Geometry

| Quantity | Value | Notes |
|---|---|---|
| Dot | 1 / 4.194304 MHz ≈ 1 / 2^22 s | Dots are **speed-independent**: 4 dots / M-cycle (normal), 2 dots / M-cycle (double-speed). |
| Dots per scanline | **456** | Fixed for every line 0–153. |
| Scanlines per frame | **154** | Lines 0–153. |
| Dots per frame | **70224** | 456 × 154. |
| Visible lines | 0–143 (144 lines) | Modes 2→3→0 cycle. |
| VBlank lines | 144–153 (10 lines) | Mode 1, **4560 dots** total. |
| Frame rate | 59.7275 Hz | ~16.74 ms/frame. |

T-cycle = dot. 1 M-cycle = 4 T-cycles (= 4 dots normal speed). All durations below are in **dots = T-cycles** unless stated.

---

## 1. Per-Scanline Mode Sequence (lines 0–143)

```
|<-------------------------- 456 dots ------------------------->|
| Mode 2 (OAM scan) | Mode 3 (Drawing) |     Mode 0 (HBlank)     |
|     80 dots       |  172..289 dots   |   376 - len(Mode3) dots |
```

| Mode | Name | Duration (dots) | VRAM | OAM | CGB Pal | STAT bits 1-0 |
|---|---|---|---|---|---|---|
| 2 | OAM Scan | **80** (fixed) | R/W | locked | R/W | `10` |
| 3 | Drawing | **172–289** (variable) | locked | locked | locked | `11` |
| 0 | HBlank | **87–204** = `376 − len(Mode3)` | R/W | R/W | R/W | `00` |
| 1 | VBlank | 456/line × 10 lines | R/W | R/W | R/W | `01` |

- Mode2 + Mode3 + Mode0 = 80 + L + (376 − L) = **456** always.
- `len(Mode0) = 456 − 80 − len(Mode3) = 376 − len(Mode3)`.
- Mode3 min 172 → Mode0 max 204. Mode3 max 289 → Mode0 min 87.
- On **line 0 after LCD enable**, the first frame's line 0 is short by a quirk (PPU starts mid-Mode-3-equivalent); STAT reads Mode 0 for a window, LY=0. (mooneye `lcdon_timing`, `ppu/intr_2_mode0_timing`, etc. — see §11.)

### Mode 1 (VBlank) detail
- Entered at **start of line 144** (dot 0 of LY=144). VBlank IRQ (IF bit 0) fires here.
- LY runs 144→153. **LY=153 quirk:** LY reads 153 for only the first 4 dots (≈1 M-cycle), then reads **0** for the remaining 452 dots of line 153 while still physically on line 153. LYC compare uses these reported values. (mooneye `ly_lyc_153`, `vblank_stat`.)
- During VBlank, modes 2/3/0 do NOT run; STAT mode stays `01`.

---

## 2. STAT Register ($FF41) & LY/LYC

```
FF41 STAT  bit7 = unused (reads 1)
           bit6 = LYC int select  (LY==LYC -> STAT IRQ)
           bit5 = Mode2 OAM int select
           bit4 = Mode1 VBlank int select
           bit3 = Mode0 HBlank int select
           bit2 = LYC==LY flag (read-only)
           bit1-0 = PPU mode (read-only): 0=HBlank 1=VBlank 2=OAM 3=Draw
```
- STAT IRQ (IF bit 1) is edge-triggered off the **OR of all enabled sources** ("STAT blocking"): IF bit 1 set only on a rising edge of the combined signal. Multiple simultaneous sources do not re-trigger.
- LYC compare: `STAT.2 = (LY == LYC)`. Compare timing has 1-dot delays the tests probe; on line 153 the compare uses the 153→0 reported value (see §1).

---

## 3. LCDC Register ($FF40)

```
bit7 LCD & PPU enable           (0 = off; resets LY=0, PPU to Mode0-ish, VRAM/OAM free)
bit6 Window tilemap area        (0=$9800, 1=$9C00)
bit5 Window enable
bit4 BG & Window tile data area (0=$8800 signed, 1=$8000 unsigned)
bit3 BG tilemap area            (0=$9800, 1=$9C00)
bit2 OBJ size                   (0=8x8, 1=8x16)
bit1 OBJ enable                 (DMG: master OBJ on/off; CGB: see note)
bit0 BG/Window enable/priority  (DMG: 0=BG+Win blank white; CGB: 0=BG loses priority, OBJ always on top)
```
- **LCDC.0 DMG:** if 0, BG and Window are not drawn (forced color 0 through BGP). 
- **LCDC.0 CGB:** BG/Win always drawn; if 0, BG-to-OBJ priority is **disabled** (objects always over BG regardless of priority bits) — the "master priority" override.
- **LCDC.1 CGB:** OBJ enable; condition is *ignored* for object-fetch gating in some Pan Docs phrasing — on CGB objects are fetched even when disabled mid-scanline in certain cases. Standard behavior: 0 hides all objects.
- LCDC.4 selects tile-data addressing: `1`→base $8000 unsigned index 0..255; `0`→base $9000 signed index −128..127 (i.e. $8800 region). **Objects always use $8000 unsigned** regardless of LCDC.4.

---

## 4. OAM Scan (Mode 2) — 80 dots

- 40 OAM entries (4 bytes each: Y, X, Tile, Attr). Scanned 2 dots per entry → 80 dots.
- An object is **selected** for the line if all hold (using sprite height H = 8 or 16 per LCDC.2):
  - `OBJ.Y` is the on-screen top + 16 bias. Condition: `LY + 16 >= OBJ.Y` **and** `LY + 16 < OBJ.Y + H`.
  - i.e. line in `[OBJ.Y − 16, OBJ.Y − 16 + H)`. X is **not** filtered here (X=0..167 off-screen still counts toward the 10 limit if Y matches).
- **Max 10 objects per line.** First 10 (in OAM index order) that match Y are kept; further matches dropped. (This buffer is the candidate list for Mode 3.)
- Selection order stored = OAM index order. Drawing priority resolved later (see §7).
- OAM Bug: writes during Mode2 corrupt OAM on DMG (not modeled in pixel pipeline core; note for OAM block). CGB unaffected.

OAM entry attribute byte (byte 3):
```
bit7 OBJ-to-BG priority (BG Priority): 0=OBJ above BG colors 1-3; 1=BG colors 1-3 above OBJ (OBJ behind non-zero BG)
bit6 Y flip
bit5 X flip
bit4 DMG palette (0=OBP0, 1=OBP1)
bit3 CGB VRAM bank (tile fetched from bank 0/1)
bit2-0 CGB OBJ palette (OBP0..7)
```

---

## 5. Background/Window Pixel Fetcher State Machine

Two independent FIFOs, each capacity **16 px**, 4 properties per pixel:

| Property | DMG | CGB |
|---|---|---|
| Color | 0–3 | 0–3 |
| Palette | (OBJ only: OBP0/1) | 0–7 (BG: BGP idx; OBJ: OBP idx) |
| Sprite Priority | n/a | OAM index of the object |
| BG Priority | OAM attr bit7 (OBJ FIFO) / BG attr bit7 (CGB BG FIFO) | same |

### 5.1 Fetcher steps (BG/Window fetcher)
5 steps; **steps 1–4 take 2 dots each**, step 5 (Push) retried every dot until it succeeds:

| # | Step | Dots | Action |
|---|---|---|---|
| 1 | Get Tile | 2 | Compute tilemap address, read tile index byte. CGB reads attribute byte (bank/flip/priority/palette) in the **same dot**. |
| 2 | Get Tile Data Low | 2 | Read low bitplane byte of the tile row. |
| 3 | Get Tile Data High | 2 | Read high bitplane byte (= low addr + 1). **Also performs an opportunistic push** if BG FIFO empty (extra push → 3 push chances per full cycle). |
| 4 | Sleep | 2 | Do nothing. |
| 5 | Push | ≥1 | Push 8 px to BG FIFO **only if FIFO empty**. If FIFO not empty, stall (retry next dot). |

Full cycle = 8 dots minimum when not stalled. Push order: **MSB-first** normally; **LSB-first if tile X-flipped** (CGB only for BG flips; DMG BG cannot flip).

### 5.2 Tilemap address computation (Get Tile)
- BG tilemap base: LCDC.3 → $9800 or $9C00. Window base: LCDC.6 → $9800 or $9C00 (used when X is inside window).
- Fetcher X coordinate `fetcherX ∈ 0..31`:
  - BG tile: `tileX = ((SCX / 8) + fetcherX) & 0x1F`
  - Window tile: window's own X counter (starts 0 at window left edge).
- Y coordinate:
  - BG: `tileY = (LY + SCY) & 0xFF`; row within tile = `tileY & 7`.
  - Window: uses **window internal line counter** `WLY` (see §6); row = `WLY & 7`.
- Tilemap byte address: `base + ((tileY/8) * 32) + tileX` (BG) / `base + ((WLY/8)*32) + winX` (Window).
- Tile data address: 
  - LCDC.4=1: `$8000 + tileIndex*16 + rowOffset*2`
  - LCDC.4=0: `$9000 + (int8)tileIndex*16 + rowOffset*2`
  - `rowOffset = (Yflip ? 7 − (Y&7) : (Y&7))` (CGB vertical flip from BG attr).
- If PPU's VRAM access is blocked, tile index / data read as **$FF**.

### 5.3 SCX fine-scroll discard (start of line)
- At the very start of Mode 3, the first BG tile is fetched, then **the first `SCX & 7` pixels are discarded** from the BG FIFO before any pixel is shipped to the LCD. This is the source of the SCX%8 penalty (§8).
- The 12-dot fixed Mode3 startup overhead = two initial tile fetches (first visible tile + one discarded warm-up fetch).

---

## 6. Window Trigger & Internal Line Counter

Registers: `WY` ($FF4A), `WX` ($FF4B). Window left pixel column = `WX − 7`.

### 6.1 WY trigger (per frame)
- A latch `window_y_triggered` is set on any line where `LY == WY` (checked each line during the frame, even if window not currently visible). Once `WY == LY` has been satisfied at least once this frame, the window is *armed*.

### 6.2 Per-scanline window activation
Window pixels begin on a scanline when **all** hold:
1. LCDC.5 (window enable) = 1.
2. `window_y_triggered` is set (WY condition met this frame, at or before current line).
3. Current output X has reached `WX − 7` (the fetcher's X reaches the WX trigger; `WX=7` → window at x=0).

On activation mid-line:
- BG FIFO is **cleared**, BG fetcher **reset to step 1**, fetcher switches to window tilemap/coords (winX starts at 0).
- **Window internal line counter `WLY`** is **incremented once per scanline on which window pixels are actually rendered** (not tied to LY). `WLY` starts at 0 (its first rendered window line uses WLY row before/at the increment — implement: WLY initialized to −1/0 at frame start, incremented when the window is rendered on a line; row used = current WLY). Tests rely on WLY only advancing on lines where window actually draws.

### 6.3 WX edge cases / quirks
- **`WX == 0` with `SCX & 7 > 0`:** Mode 3 is **shortened by 1 dot**. (Pan Docs: "When WX is 0 and SCX&7 > 0, mode 3 is shortened by 1 dot.")
- **`WX == 166`** (`WX−7 = 159`): window appears at last pixel.
- **`WX` in 0..6:** window covers from x=0 with off-screen left portion; behaves as left edge clipped.
- **`WX == 165/166` glitches** and `WX>166` → window not shown that line.
- **Mid-scanline WX change after window started:** if WX is changed after the window began rendering and the new WX value is later reached, a pixel with **color 0 and lowest priority** is pushed onto the BG FIFO (Pan Docs documented glitch). Model this for accuracy ROMs that poke WX mid-line.
- WY is latched per-line as above; changing WY after it has triggered does not un-trigger for the frame.

---

## 7. Sprite (OBJ) Fetching During Mode 3

When the current output X reaches an X position where one or more selected objects have `OBJ.X == currentX` (OBJ.X is screen X + 8 bias; pixel column = `OBJ.X − 8`), BG pixel shipping **pauses** and object fetch runs:

### 7.1 Object fetch procedure (per object at this X)
1. Gate (DMG): requires LCDC.1 enabled and an object present at X; else fetch canceled (§7.3). On CGB the enable condition is ignored for this gating.
2. **Advance BG fetcher** one step at a time until it reaches step 5 (Push) **or** BG FIFO is non-empty. Each forced advance here **lengthens Mode 3 by 1 dot**. (Ensures a BG pixel exists to mix.)
3. **X=0 SCX penalty:** if `SCX & 7 > 0` **and** an object sits at scanline X coordinate 0, Mode 3 is lengthened by `SCX & 7` dots. (Timing position of this penalty unconfirmed; may be before/after the fetcher wait.)
4. Advance BG fetcher **two steps**: first advance **+1 dot**, second advance **+3 dots**.
5. **Get OBJ tile data low:** **+1 dot** (last cancel opportunity). 
6. **Exit object fetch:** **+1 dot**. Get OBJ tile data high: **+0 dots** (does not lengthen).
7. Mix into OBJ FIFO: if OBJ FIFO has <8 px, pad with transparent lowest-priority pixels first. Then for each of the 8 object pixels (CGB horizontal flip applied here): replace the OBJ-FIFO pixel **iff** (new px is non-transparent AND existing FIFO px is transparent) **OR** (existing FIFO px has lower priority than new px). Priority tie-break = OAM index (lower index wins on CGB; on DMG, lower X wins, then lower OAM index).
8. Render one pixel + advance BG fetcher one step: **+1 dot** if current X ≠ 160; if X==160, stop processing objects.

### 7.2 Mode 3 OBJ penalty — Pan Docs canonical algorithm
For each object drawn (even partially), penalty = **6 to 11 dots**, computed via "The Pixel" (the object's leftmost pixel, transparent or not):

```
penalty = 0
1. Determine the BG/Window tile that The Pixel lies within
   (account for SCX fine-scroll and window!).
2. IF that tile was NOT already considered by a previous OBJ this line:
     a. n = count of that tile's pixels strictly to the RIGHT of The Pixel
            (i.e. n = 7 - (ThePixelXWithinTile))   [0..7]
     b. n = n - 2
     c. penalty += max(n, 0)        # 0..5 dots, waiting for BG fetch to finish
3. penalty += 6                     # flat OBJ tile fetch cost
```
- Range: 6 (n≤2 or tile reused) … 11 (n=5 max + 6).
- **Exception:** an object with **OAM X == 0** (fully off left) always incurs a flat **11-dot** penalty, regardless of SCX.
- Objects considered left→right by X; ties broken by OAM index (lowest first).

> The two formulations (per-step dot accounting in §7.1 vs the closed-form §7.2) describe the same hardware; implement §7.2 for the Mode-3-length total used by STAT/timing tests, and use §7.1's FIFO mechanics for correct pixel output. They must agree on total dots.

### 7.3 Object fetch canceling
- If LCDC.1 is disabled mid-fetch (DMG), the in-progress object fetch is canceled. This lengthens Mode 3 by (dots the interrupted step consumed + residual dots). On cancel: render a pixel and advance BG fetcher one step (**+1 dot** if X≠160; stop if X==160).

---

## 8. Mode 3 Length — Closed Form (for STAT/timing tests)

```
len(Mode3) = 172                                   # base (160 visible + 12 startup)
           + (SCX & 7)                             # fine scroll discard penalty
           + Σ_objects obj_penalty                 # 6..11 each, per §7.2 (max 10 objects)
           + window_penalty                        # 6 dots IF window rendered this line, else 0
           − ((WX==0 && (SCX&7)>0) ? 1 : 0)        # WX=0 shortening quirk
```
- **Base 172** = 160 (one px/dot) + 12 (two startup tile fetches).
- **SCX penalty** = `SCX & 7` (0–7 dots).
- **Window penalty** = **6 dots** flat, charged after the last non-window pixel, while the fetcher is reset for the window (only if window actually rendered on this line).
- **OBJ penalty** = sum over each drawn object of its 6–11 dot penalty (§7.2). With 10 objects all worst-case (11) you approach the high end.
- **Min Mode3** = 172 (SCX&7=0, no objects, no window) → Mode0 = 204.
- **Max Mode3** = 289 (documented cap) → Mode0 = 87. (160 + SCX 7 + window 6 + heavy object load reach 289; Pan Docs states the 172–289 envelope.)

len(Mode0) = `376 − len(Mode3)` (range 87..204).

### 8.1 Worked examples
| SCX&7 | Window? | Objects (penalties) | Mode3 | Mode0 |
|---|---|---|---|---|
| 0 | no | none | 172 | 204 |
| 3 | no | none | 175 | 201 |
| 0 | yes | none | 178 | 198 |
| 0 | no | 1 obj, tile fresh, n=5 → 11 | 183 | 193 |
| 5 | no | 2 obj @11 each | 199 | 177 |
| 7 | yes | 10 obj worst-case (approach cap) | →289 (clamped envelope) | 87 |

---

## 9. FIFO Mixing & Pixel Output Rules

Pixel shipped to LCD each dot when BG FIFO non-empty and not paused. Pop one BG px (always) and one OBJ px (if OBJ FIFO non-empty):

### 9.1 DMG mixing
```
bg = pop(BG_FIFO)            # color 0..3
obj = pop(OBJ_FIFO) if any  # color 0..3, palette OBP0/1, bg_priority bit

if LCDC.0 == 0:  bg.color = 0          # BG disabled -> white
draw_obj =
    obj exists AND
    LCDC.1 == 1 AND
    obj.color != 0 AND
    NOT (obj.bg_priority == 1 AND bg.color != 0)

if draw_obj:
    final = OBPx[obj.color]            # x = obj.palette (OBP0/OBP1)
else:
    final = BGP[bg.color]
```
- OBJ-to-BG priority bit (OBJ attr bit7): 1 → OBJ hidden behind BG colors 1–3 (BG color 0 still lets OBJ show).
- OBJ-vs-OBJ on DMG: lower X wins; equal X → lower OAM index wins (resolved during OAM FIFO merge in §7.1 step 7).

### 9.2 CGB mixing
```
bg  = pop(BG_FIFO)   # color, palette 0..7, bg_attr_priority (BG attr bit7)
obj = pop(OBJ_FIFO)  # color, palette 0..7, oam_priority, obj_bg_priority

bg_has_priority =
    LCDC.0 == 1 AND
    ( bg.bg_attr_priority == 1 OR obj.obj_bg_priority == 1 ) AND
    bg.color != 0

if obj exists AND obj.color != 0 AND NOT bg_has_priority:
    final = OBJ_PAL[obj.palette][obj.color]
else:
    final = BG_PAL[bg.palette][bg.color]
```
- **Master priority (LCDC.0 on CGB):** if 0, `bg_has_priority` is forced false → **objects always drawn over BG** (BG attr bit7 and OBJ attr bit7 both ignored). This is the "BG-to-OAM priority master switch."
- **BG attr bit7 (BG-to-OBJ priority):** if set, that BG tile's colors 1–3 sit above objects (subject to master bit).
- **OBJ attr bit7 (OBJ-to-BG priority):** if set, object sits behind BG colors 1–3.
- Either priority bit set (and master on, BG color≠0) → BG wins. Both clear → OBJ wins (if non-transparent).
- OBJ-vs-OBJ on CGB: **lowest OAM index wins** regardless of X (resolved in OAM FIFO merge).

### 9.3 Output gating
- A pixel is **not** shipped if BG FIFO is empty or current output X ≥ 160.
- During SCX discard (§5.3), the first `SCX&7` popped BG pixels are discarded (not shipped, OBJ FIFO not popped for them).
- CGB palette-access-blocked windows push a **black** pixel instead of the palette color (see Pan Docs CGB palette access list).

---

## 10. 8×8 vs 8×16 Objects (LCDC.2)

| Mode | Height | Tile selection |
|---|---|---|
| 8×8 | 8 | Use OBJ tile index as-is. |
| 8×16 | 16 | **Bit 0 of tile index forced to 0** for top tile; `index | 1` for bottom tile. Top = rows 0–7, bottom = rows 8–15. |

- Y-flip (attr bit6) in 8×16 flips the whole 16-px sprite: swaps top/bottom tiles AND flips rows within each.
- Row within sprite: `row = LY + 16 − OBJ.Y` (0..H−1); if Y-flip, `row = H − 1 − row`. Then top/bottom tile and `row & 7` chosen.
- Object tile data always from $8000 (unsigned), VRAM bank per attr bit3 (CGB).

---

## 11. Test-ROM Coverage Map (what to validate)

| Test set | Checks |
|---|---|
| Mooneye `ppu/intr_2_mode0_timing`, `intr_2_mode3_timing`, `intr_2_0_timing`, `intr_2_mode0_timing_sprites` | Mode2/3/0 boundary dot timing; sprite penalty dot counts. |
| Mooneye `lcdon_timing`, `lcdon_write_timing` | PPU restart timing after LCDC.7 enable; first line short. |
| Mooneye `ly_lyc`, `ly_lyc_153`, `vblank_stat`, `stat_irq_blocking`, `stat_lyc_onoff` | LY=153→0 quirk, LYC compare timing, STAT IRQ edge/blocking. |
| Mooneye `hblank_ly_scx_timing` | SCX&7 Mode3 lengthening (BGP-write left-shift observation). |
| Blargg `dmg-acid2` / `cgb-acid2` (matt currie) | BG/Win/OBJ priority, 8×16, flips, window, master priority bit. |
| Mealybug Tearoom `m3_*` (e.g. `m3_bgp_change`, `m3_scx_*`, `m3_window_timing`, `m3_lcdc_obj_*`, `m3_wx_*`) | Mid-Mode3 register writes landing at exact dot; the core of pixel-pipeline timing. |

### 11.1 Mealybug timing principle
Mid-scanline writes to BGP/OBP/LCDC/SCX/WX take effect at the **exact dot** the pixel pipeline consumes them. Any Mode3 lengthening shifts a `BGP` write's visible effect **left by that many dots** (the write "lands" earlier in the rendered line). Implement the pipeline dot-by-dot so these writes resolve at pixel granularity.

---

## 12. VRAM/Palette Access Blocking (affects fetcher reads)

VRAM read returns **$FF** (fetcher sees $FF for tile index/data) when blocked:
- LCD turning off
- Scanline 0 on CGB (not double-speed)
- Switching Mode 3 → Mode 0
- CGB during OAM search when index 37 reached

VRAM access restored: scanline 0 on DMG (and CGB double-speed); DMG OAM search index 37; after Mode 2 → Mode 3.

CGB palette read blocked (push **black** pixel): LCD off; first HBlank of frame; OAM search index 37; after Mode2→Mode3; entering HBlank (non-double-speed) blocked 2 dots later. Restored: end of Mode 2; 2 dots when entering HBlank in double-speed.

(These conditions are evaluated when entering STOP; access always restored on leaving STOP.)

---

## 13. Implementation Skeleton (dot-stepped Mode 3)

```
on enter Mode3:
    clear BG_FIFO, OBJ_FIFO
    fetcher.reset(step=GetTile); fetcherX=0; winX=0
    x_out = 0
    to_discard = SCX & 7              # fine scroll
    mode3_dots = 0
    window_active = false

each dot while x_out < 160:
    fetcher.tick()                    # 2-dot steps; push when BG_FIFO empty

    # window activation
    if !window_active and LCDC.5 and window_y_triggered and x_out_reached(WX-7):
        BG_FIFO.clear(); fetcher.reset_to_window(); window_active=true
        WLY incremented (once this line)
        mode3_dots += 6               # window penalty

    # object fetch
    if object_at(x_out) and (CGB or LCDC.1):
        run_object_fetch()           # adds 6..11 dots per §7.2 / §7.1
        # X==0 obj + SCX&7>0 -> + (SCX&7) ; X==0 obj -> flat 11

    if BG_FIFO not empty and not paused:
        bg = BG_FIFO.pop()
        if to_discard > 0:
            to_discard--             # discard, do NOT pop OBJ FIFO, no ship
        else:
            obj = OBJ_FIFO.pop_if_any()
            ship(mix(bg, obj))       # §9
            x_out++
    mode3_dots++

# WX==0 && SCX&7>0 -> mode3_dots -= 1 (apply once)
enter Mode0 for (376 - mode3_dots) dots
```

---

## 14. Quick Reference Constants

```
DOTS_PER_LINE      = 456
LINES_PER_FRAME    = 154
VBLANK_START_LINE  = 144
DOTS_PER_FRAME     = 70224
MODE2_DOTS         = 80
MODE3_MIN          = 172
MODE3_MAX          = 289
MODE0_MIN          = 87
MODE0_MAX          = 204
MODE3_PLUS_MODE0   = 376            # = 456 - 80
OBJ_PENALTY_MIN    = 6
OBJ_PENALTY_MAX    = 11
OBJ_X0_PENALTY     = 11            # flat, OAM X==0
WINDOW_PENALTY     = 6
SCX_PENALTY        = SCX & 7       # 0..7
MAX_OBJ_PER_LINE   = 10
FIFO_CAPACITY      = 16
FETCH_STEP_DOTS    = 2             # steps 1-4; step 5 retried per-dot
```
```

Sources verified against Pan Docs `Rendering.html` (mode durations, 172–289 envelope, OBJ penalty algorithm, SCX/window penalties, WX=0 shortening) and `pixel_fifo.html` (fetcher steps, FIFO mixing, per-step object-fetch dot accounting, VRAM/palette blocking, mid-scanline WX glitch). Scraped copies saved at `C:\Users\ChadE\Tester Bridgeming Goat\.firecrawl\rendering.md` and `C:\Users\ChadE\Tester Bridgeming Goat\.firecrawl\pixel_fifo.md`.