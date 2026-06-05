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

[Print this book](https://gbdev.io/pandocs/print.html "Print this book")[Git repository](https://github.com/gbdev/pandocs "Git repository")[Suggest an edit](https://github.com/gbdev/pandocs/edit/master/src/Joypad_Input.md "Suggest an edit")

The eight Game Boy action/direction buttons are arranged as a 2×4
matrix. Select either action or direction buttons by writing to this
register, then read out the bits 0-3.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **P1** |  | Select buttons | Select d-pad | Start / Down | Select / Up | B / Left | A / Right |

- **Select buttons**: If this bit is `0`, then buttons (SsBA) can be read from the lower nibble.

- **Select d-pad**: If this bit is `0`, then directional keys can be read from the lower nibble.

- The lower nibble is _Read-only_.
Note that, rather unconventionally for the Game Boy, a button being pressed is seen as the corresponding bit being **`0`**, not `1`.

If neither buttons nor d-pad is selected (`$30` was written), then the low nibble reads `$F` (all buttons released).


NOTE

Most programs read from this port several times in a row
(the first reads are used as a short delay, allowing the inputs to stabilize,
and only the value from the last read is actually used).

Beside for normal joypad input, SGB games misuse the joypad register to
output SGB command packets to the SNES, also, SGB programs may read out
gamepad states from up to four different joypads which can be connected
to the SNES. See SGB description for details.