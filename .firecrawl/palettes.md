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

[Print this book](https://gbdev.io/pandocs/print.html "Print this book")[Git repository](https://github.com/gbdev/pandocs "Git repository")[Suggest an edit](https://github.com/gbdev/pandocs/edit/master/src/Palettes.md "Suggest an edit")

This register assigns gray shades to the [color indices](https://gbdev.io/pandocs/Tile_Data.html#data-format) of the BG and Window tiles.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **Color for...** | ID 3 | ID 2 | ID 1 | ID 0 |

Each of the two-bit values map to a color thusly:

| Value | Color |
| --- | --- |
| 0 | White |
| 1 | Light gray |
| 2 | Dark gray |
| 3 | Black |

In CGB Mode the color palettes are taken from [CGB palette memory](https://gbdev.io/pandocs/Palettes.html#lcd-color-palettes-cgb-only)
instead.

These registers assigns gray shades to the color indexes of the OBJs that use the corresponding palette.
They work exactly like [`BGP`](https://gbdev.io/pandocs/Palettes.html#ff47--bgp-non-cgb-mode-only-bg-palette-data), except that the lower two bits are ignored because color index 0 is transparent for OBJs.

The CGB has a small amount of RAM used to store its color palettes. Unlike most
of the hardware interface, palette RAM (or _CRAM_ for _Color RAM_) is not
accessed directly, but instead through the following registers:

This register is used to address a byte in the CGB’s background palette RAM.
Since there are 8 palettes, 8 palettes × 4 colors/palette × 2 bytes/color = 64 bytes
can be addressed.

First comes BGP0 color number 0, then BGP0 color number 1, BGP0 color number 2, BGP0 color number 3,
BGP1 color number 0, and so on. Thus, address $03 allows accessing the second (upper)
byte of BGP0 color #1 via BCPD, which contains the color’s blue and upper green bits.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **BCPS / OCPS** | Auto-increment |  | Address |

- **Auto-increment**: `0` = Disabled; `1` = Increment “Address” field after **writing** to
[`BCPD`](https://gbdev.io/pandocs/Palettes.html#ff69--bcpdbgpd-cgb-mode-only-background-color-palette-data--background-palette-data) /
[`OCPD`](https://gbdev.io/pandocs/Palettes.html#ff6aff6b--ocpsobpi-ocpdobpd-cgb-mode-only-obj-color-palette-specification--obj-palette-index-obj-color-palette-data--obj-palette-data)
(even during [Mode 3](https://gbdev.io/pandocs/Rendering.html#ppu-modes), despite the write itself failing), reads _never_ cause an increment
- **Address**: Specifies which byte of BG Palette Memory can be accessed through
[`BCPD`](https://gbdev.io/pandocs/Palettes.html#ff69--bcpdbgpd-cgb-mode-only-background-color-palette-data--background-palette-data)

Unlike BCPD, this register can be accessed outside VBlank and HBlank.

This register allows to read/write data to the CGBs background palette memory, addressed through [BCPS/BGPI](https://gbdev.io/pandocs/Palettes.html#ff68--bcpsbgpi-cgb-mode-only-background-color-palette-specification--background-palette-index).
Each color is stored as little-endian RGB555:

|  | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **One color** | Red intensity | Green intensity | Blue intensity |  |

Much like VRAM, data in palette memory cannot be read or written during the time
when the PPU is reading from it, that is, [Mode 3](https://gbdev.io/pandocs/Rendering.html#ppu-modes).

NOTE

All background colors are initialized as white by the boot ROM, however it is a
good idea to initialize all colors yourself, e.g. if implementing
a soft-reset mechanic.

These registers function exactly like BCPS and BCPD respectively; the 64 bytes
of OBJ palette memory are entirely separate from Background palette memory, but
function the same.

Note that while 4 colors are stored per OBJ palette, color #0 is never used, as
it’s always transparent. It’s thus fine to write garbage values, or even leave
color #0 uninitialized.

NOTE

In CGB mode, the boot ROM leaves all object colors uninitialized (and thus somewhat random/unreliable),
aside from setting the first byte of OBJ0 color #0 to $00, which is unused.

In DMG compatibility mode, the boot ROM sets the first 2 object palettes which are
used by OBP0/OBP1, [as explained here](https://gbdev.io/pandocs/Power_Up_Sequence.html#compatibility-palettes).

![sRGB versus CGB color mixing](https://gbdev.io/pandocs/imgs/VGA_versus_CGB.png)

When developing graphics on PCs, note that the RGB values will have
different appearance on CGB displays as on VGA/HDMI monitors calibrated
to sRGB color. Because the GBC is not lit, the highest intensity will
produce light gray rather than white. The intensities are not
linear; the values $10-$1F will all appear very bright, while medium and
darker colors are ranged at $00-0F.

The CGB display’s pigments aren’t perfectly saturated. This means the
colors mix quite oddly: increasing the intensity of only one R/G/B color
will also influence the other two R/G/B colors. For example, a color
setting of $03EF (Blue=$00, Green=$1F, Red=$0F) will appear as Neon Green
on VGA displays, but on the CGB it’ll produce a decently washed out
Yellow. See the image above.

Even though GBA is described to be compatible to CGB games, most CGB
games are completely unplayable on older GBAs because most colors are
invisible (black). Of course, colors such like Black and White will
appear the same on both CGB and GBA, but medium intensities are arranged
completely different. Intensities in range $00–07 are invisible/black
(unless eventually under best sunlight circumstances, and when gazing at
the screen under obscure viewing angles), unfortunately, these
intensities are regularly used by most existing CGB games for medium and
darker colors.

WORKAROUND

Newer CGB games may avoid this effect by changing palette data when
detecting GBA hardware ( [see how](https://gbdev.io/pandocs/CGB_Registers.html#detecting-cgb-and-gba-functions)).
Based on measurements of GBC and GBA palettes using the
[144p Test Suite](https://github.com/pinobatch/240p-test-mini/tree/master/gameboy),
a fairly close approximation is `GBA = GBC × 3/4 + $08` for each R/G/B
component. The result isn’t quite perfect, and it may turn
out that the color mixing is different also; anyways, it’d be still
ways better than no conversion.

This problem with low brightness levels does not affect later GBA SP
units and Game Boy Player. Thus ideally, the player should have control
of this brightness correction.