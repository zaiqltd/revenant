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

[Print this book](https://gbdev.io/pandocs/print.html "Print this book")[Git repository](https://github.com/gbdev/pandocs "Git repository")[Suggest an edit](https://github.com/gbdev/pandocs/edit/master/src/halt.md "Suggest an edit")

`halt` is an instruction that pauses the CPU (during which [less power is\\
consumed](https://gbdev.io/pandocs/Reducing_Power_Consumption.html#using-the-halt-instruction)) when executed. The CPU wakes up as soon as an interrupt is pending,
that is, when the bitwise AND of [`IE`](https://gbdev.io/pandocs/Interrupts.html#ffff--ie-interrupt-enable)
and [`IF`](https://gbdev.io/pandocs/Interrupts.html#ff0f--if-interrupt-flag) is non-zero.

Most commonly, [`IME`](https://gbdev.io/pandocs/Interrupts.html#ime-interrupt-master-enable-flag-write-only) is
set. In this case, the CPU simply wakes up, and before executing the instruction
after the `halt`, the [interrupt handler is called](https://gbdev.io/pandocs/Interrupts.html#interrupt-handling)
normally.

If `IME` is _not_ set, there are two distinct cases, depending on whether an
interrupt is pending as the `halt` instruction is first executed.

- If no interrupt is pending, `halt` executes as normal, and the CPU resumes
regular execution as soon as an interrupt becomes pending. However, since
`IME`=0, the interrupt is not handled.
- If an interrupt is pending, `halt` immediately exits, as expected, however
the “`halt` bug”, explained below, is triggered.

When a `halt` instruction is executed with `IME = 0` and `[IE] & [IF] != 0`, the `halt` instruction ends immediately, but [`pc` fails to be normally incremented](https://github.com/nitro2k01/little-things-gb/tree/main/double-halt-cancel).

Under most circumstances, this causes the byte after the `halt` to be read a second time (and this behaviour can repeat if said byte executes another `halt` instruction).
But, if the `halt` is immediately followed by a jump to elsewhere, then the behaviour will be slightly different; this is possible in only one of two ways:

- The `halt` comes immediately after a `ei` instruction (whose effect is typically delayed by one instruction, hence `IME` still being zero for the `halt`): the interrupt is serviced and the handler called, but the interrupt returns to the `halt`, which is executed again, and thus
waits for another interrupt.
( [Source](https://github.com/LIJI32/SameSuite/blob/master/interrupt/ei_delay_halt.asm))
- The `halt` is immediately followed by a `rst` instruction: the `rst` instruction’s return address will point at the `rst` itself, instead of the byte after it.
Notably, a `ret` would return to the `rst` an execute it again.

If the bugged `halt` is preceded by a `ei` and followed by a `rst`, the former “wins”.