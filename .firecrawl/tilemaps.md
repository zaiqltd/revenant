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

[Print this book](https://gbdev.io/pandocs/print.html "Print this book")[Git repository](https://github.com/gbdev/pandocs "Git repository")[Suggest an edit](https://github.com/gbdev/pandocs/edit/master/src/Tile_Maps.md "Suggest an edit")

The Game Boy contains two 32×32 tile maps in VRAM at
the memory areas `$9800-$9BFF` and `$9C00-$9FFF`. Any of these maps can be used to
display the Background or the Window.

Each tile map contains the 1-byte indexes of the
tiles to be displayed.

Tiles are obtained from the Tile Data Table using either of the two
addressing modes (described in [VRAM Tile Data](https://gbdev.io/pandocs/Tile_Data.html#vram-tile-data)), which
can be selected via [the LCDC register](https://gbdev.io/pandocs/LCDC.html#ff40--lcdc-lcd-control).

Since one tile has 8×8 pixels, each map holds a 256×256 pixels picture.
Only 160×144 of those pixels are displayed on the LCD at any given time.

In CGB Mode, an additional map of 32×32 bytes is stored in VRAM Bank 1
(each byte defines attributes for the corresponding tile-number map
entry in VRAM Bank 0, that is, 1:9800 defines the attributes for the tile at
0:9800):

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **BG attributes** | Priority | Y flip | X flip |  | Bank | Color palette |

- **Priority**: `0` = No; `1` = [Color indices](https://gbdev.io/pandocs/Tile_Data.html#data-format) 1–3 of the corresponding BG/Window tile are drawn over OBJ, regardless of [OBJ priority](https://gbdev.io/pandocs/OAM.html#byte-3--attributesflags)
- **Y flip**: `0` = Normal; `1` = Tile is drawn vertically mirrored
- **X flip**: `0` = Normal; `1` = Tile is drawn horizontally mirrored
- **Bank**: `0` = Fetch tile from VRAM bank 0; `1` = Fetch tile from VRAM bank 1
- **Color palette**: Which of BGP0–7 to use

Bit 4 is ignored by the hardware, but can be written to and read from normally.

Note that, for example, if the byte at `0:9800` is $2A, the attribute at `1:9800` doesn’t define properties for ALL tiles $2A on-screen, but only the one at `0:9800`!

In CGB Mode, the priority between the BG (and window) layer and the OBJ layer is declared in three different places:

- [BG Map Attribute bit 7](https://gbdev.io/pandocs/Tile_Maps.html#bg-map-attributes-cgb-mode-only)
- [LCDC bit 0](https://gbdev.io/pandocs/LCDC.html#lcdc0--bg-and-window-enablepriority)
- [OAM Attributes bit 7](https://gbdev.io/pandocs/OAM.html#byte-3--attributesflags)

We can infer the following rules from the table below:

- If the BG color index is 0, the OBJ will always have priority;
- Otherwise, if LCDC bit 0 is clear, the OBJ will always have priority;
- Otherwise, if both the BG Attributes and the OAM Attributes have bit 7 clear, the OBJ will have priority;
- Otherwise, BG will have priority.

The following table shows the relations between the 3 flags:

| LCDC bit 0 | OAM attr bit 7 | BG attr bit 7 | Priority |
| --- | --- | --- | --- |
| 0 | 0 | 0 | OBJ |
| 0 | 0 | 1 | OBJ |
| 0 | 1 | 0 | OBJ |
| 0 | 1 | 1 | OBJ |
| 1 | 0 | 0 | OBJ |
| 1 | 0 | 1 | BG color 1–3, otherwise OBJ |
| 1 | 1 | 0 | BG color 1–3, otherwise OBJ |
| 1 | 1 | 1 | BG color 1–3, otherwise OBJ |

[This test ROM](https://github.com/alloncm/MagenTests) can be used to observe the above.

Keep in mind that:

- OAM Attributes bit 7 will grant OBJ priority when **clear**, not when **set**.
- Priority between all OBJs is resolved **before** priority with the BG layer is considered.
Please refer [to this page](https://gbdev.io/pandocs/OAM.html#drawing-priority) for more details.

The [SCY and SCX](https://gbdev.io/pandocs/Scrolling.html#ff42ff43--scy-scx-background-viewport-y-position-x-position)
registers can be used to scroll the Background, specifying the origin of the visible
160×144 pixel area within the total 256×256 pixel Background map.
The visible area of the Background wraps around the Background map (that is, when part of
the visible area goes beyond the map edge, it starts displaying the opposite side of the map).

In Non-CGB mode, the Background (and the Window) can be disabled using
[LCDC bit 0](https://gbdev.io/pandocs/LCDC.html#lcdc0--bg-and-window-enablepriority).

Besides the Background, there is also a Window overlaying it.
The content of the Window is not scrollable; it is always
displayed starting at the top left tile of its tile map. The only way to adjust the Window
is by modifying its position on the screen, which is done via the WX and WY registers. The screen
coordinates of the top left corner of the Window are (WX-7,WY). The tiles
for the Window are stored in the Tile Data Table. Both the Background
and the Window share the same Tile Data Table.

Whether the Window is displayed can be toggled using
[LCDC bit 5](https://gbdev.io/pandocs/LCDC.html#lcdc5--window-enable). But in Non-CGB mode this bit is only
functional as long as [LCDC bit 0](https://gbdev.io/pandocs/LCDC.html#lcdc0--bg-and-window-enablepriority) is set.
Enabling the Window makes
[Mode 3](https://gbdev.io/pandocs/Rendering.html#ppu-modes) slightly longer on scanlines where it’s visible.
(See [WX and WY](https://gbdev.io/pandocs/Window.html#ff4aff4b--wy-wx-window-y-position-x-position-plus-7)
for the definition of “Window visibility”.)

Window Internal Line Counter

The window keeps an internal line counter that’s functionally similar to `LY`, and increments alongside it. However, it only gets incremented when the window is visible, as described [here](https://gbdev.io/pandocs/Window.html#window-rendering-criteria).

This line counter determines what window line is to be rendered on the current scanline.