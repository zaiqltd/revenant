## Keyboard shortcuts

Press `←` or `→` to navigate between chapters

Press `S` or `/` to search in the book

Press `?` to show this help

Press `Esc` to hide this help

- Auto
- Light
- Rust
- Coal
- Navy
- Ayu

# Pan Docs

[Print this book](https://gbdev.io/pandocs/print.html "Print this book")[Git repository](https://github.com/gbdev/pandocs "Git repository")[Suggest an edit](https://github.com/gbdev/pandocs/edit/master/src/Rendering.md "Suggest an edit")

The entire frame is not drawn atomically; instead, the image is drawn by the **PPU** (Pixel-Processing Unit) progressively, **directly to the screen**.
A frame consists of 154 **scanlines**; during the first 144, the screen is drawn top to bottom, left to right.

The main implication of this rendering process is the existence of **raster effects**: modifying some rendering parameters in the middle of rendering.
The most famous raster effect is modifying the [scrolling registers](https://gbdev.io/pandocs/Scrolling.html#viewport-position-scrolling) between scanlines to create a [“wavy” effect](https://gbdev.io/guides/deadcscroll#effects).

A “ **dot**” = one 222 Hz (≅ 4.194 MHz) time unit.
Dots remain the same regardless of whether the CPU is in [Double Speed mode](https://gbdev.io/pandocs/CGB_Registers.html#ff4d--key1spd-cgb-mode-only-prepare-speed-switch), so there are 4 dots per Normal Speed M-cycle, and 2 per Double Speed M-cycle.

NOTE

A frame is not exactly one 60th of a second: the Game Boy runs slightly slower than 60 Hz, as one frame takes ~16.74 ms instead of ~16.67 (the error is 0.45%).

During a frame, the Game Boy’s PPU cycles between four modes as follows:

Mode 2OAM scan80 dotsMode 3Drawing pixels172–289 dotsMode 0Horizontal blank87–204 dotsMode 1Vertical blankVRAM ($8000–9FFF) inaccessibleCGB palettes inaccessibleOAM inaccessible (except by DMA)EverythingaccessibleLY = 0SHORTESTLONGEST456 dots1LONGESTSHORTEST456 dots2...3...4.........14314414515310 "scanlines"4560 dotsOne frame:70224 dots@ 59.7 fps

While the PPU is accessing some video-related memory, [that memory is inaccessible to the CPU](https://gbdev.io/pandocs/Accessing_VRAM_and_OAM.html#accessing-vram-and-oam) (writes are ignored, and reads return garbage values, usually $FF).

| Mode | Action | Duration | Accessible video memory |
| --- | --- | --- | --- |
| 2 | Searching for OBJs which overlap this line | 80 dots | VRAM, CGB palettes |
| 3 | Sending pixels to the LCD | Between 172 and 289 dots, see below | None |
| 0 | Waiting until the end of the scanline | 376 - mode 3’s duration | VRAM, OAM, CGB palettes |
| 1 | Waiting until the next frame | 4560 dots (10 scanlines) | VRAM, OAM, CGB palettes |

During Mode 3, by default the PPU outputs one pixel to the screen per dot, from left to right; the screen is 160 pixels wide, so the minimum Mode 3 length is 160 + 12[1](https://gbdev.io/pandocs/Rendering.html#footnote-first12) = 172 dots.

Unlike most game consoles, the Game Boy does not always output pixels steadily[2](https://gbdev.io/pandocs/Rendering.html#footnote-crt): some features cause the rendering process to stall for a couple dots.
Any extra time spent stalling _lengthens_ Mode 3; but since scanlines last for a fixed number of dots, Mode 0 is therefore shortened by that same amount of time.

Three things can cause Mode 3 “penalties”:

- **Background scrolling**: At the very beginning of Mode 3, rendering is paused for [`SCX`](https://gbdev.io/pandocs/Scrolling.html#ff42ff43--scy-scx-background-viewport-y-position-x-position) % 8 dots while the same number of pixels are discarded from the leftmost tile.
- **Window**: After the last non-window pixel is emitted, a 6-dot penalty is incurred while the BG fetcher is being set up for the window.
- **Objects**: Each object drawn during the scanline (even partially) incurs a 6- to 11-dot penalty ( [see below](https://gbdev.io/pandocs/Rendering.html#obj-penalty-algorithm)).

On DMG and GBC in DMG mode, mid-scanline writes to [`BGP`](https://gbdev.io/pandocs/Palettes.html#ff47--bgp-non-cgb-mode-only-bg-palette-data) allow observing this behavior precisely, as any delay shifts the write’s effect to the left by that many dots.

Only the OBJ’s leftmost pixel matters here, transparent or not; it is designated as “The Pixel” in the following.

1. Determine the tile (background or window) that The Pixel is within. (This is affected by horizontal scrolling and/or the window!)
2. If that tile has **not** been considered by a previous OBJ yet[3](https://gbdev.io/pandocs/Rendering.html#footnote-order):

1. Count how many of that tile’s pixels are strictly to the right of The Pixel.
2. Subtract 2.
3. Incur this many dots of penalty, or zero if negative (from waiting for the BG fetch to finish).
3. Incur a flat, 6-dot penalty (from fetching the OBJ’s tile).

**Exception**: an OBJ with an OAM X position of 0 (thus, completely off the left side of the screen) always incurs a 11-dot penalty, regardless of `SCX`.

* * *

1. The 12 extra dots of penalty come from two tile fetches at the beginning of Mode 3. One is the first tile in the scanline (the one that gets shifted by `SCX` % 8 pixels), the other is simply discarded. [↩](https://gbdev.io/pandocs/Rendering.html#fr-first12-1)

2. The Game Boy can afford to “take pauses”, because it writes to a LCD it fully controls; by contrast, home consoles like the NES or SNES are on a schedule imposed by the screen they are hooked up to. Taking pauses arguably simplified the PPU’s design while allowing greater flexibility to game developers. [↩](https://gbdev.io/pandocs/Rendering.html#fr-crt-1)

3. Since pixels are emitted from left to right, OBJs overlapping the scanline are considered from [leftmost](https://gbdev.io/pandocs/OAM.html#byte-1--x-position) to rightmost, with ties broken by their index / OAM address (lowest first). [↩](https://gbdev.io/pandocs/Rendering.html#fr-order-1)