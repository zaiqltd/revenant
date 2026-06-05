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

[Print this book](https://gbdev.io/pandocs/print.html "Print this book")[Git repository](https://github.com/gbdev/pandocs "Git repository")[Suggest an edit](https://github.com/gbdev/pandocs/edit/master/src/Audio_Registers.md "Suggest an edit")

Audio registers are named following a `NRxy` scheme, where `x` is the channel number (or `5` for “global” registers), and `y` is the register’s ID within the channel.
Since many registers share common properties, a notation is often used where e.g. `NRx2` is used to designate `NR12`, `NR22`, `NR32`, and `NR42` at the same time, for simplicity.

As a rule of thumb, for any `x` in `1`, `2`, `3`, `4`:

- `NRx0` is some channel-specific feature (if present),
- `NRx1` controls the length timer,
- `NRx2` controls the volume and envelope,
- `NRx3` controls the period (maybe only partially),
- `NRx4` has the channel’s trigger and length timer enable bits, as well as any leftover bits of period;

…but there are some exceptions.

One of the pitfalls of the `NRxy` naming convention is that the register’s purpose is not immediately clear from its name, so some alternative `AUD*` names have been proposed, [such as `AUDENA` for `NR52`](https://github.com/gbdev/hardware.inc/blob/8d4432e5796bffe2e13c438013285c5f11c37b99/hardware.inc#L910).

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR52** | Audio on/off |  | CH4 on? | CH3 on? | CH2 on? | CH1 on? |

- **Audio on/off** ( _Read/Write_): This controls whether the APU is powered on at all (akin to [`LCDC` bit 7](https://gbdev.io/pandocs/LCDC.html#lcdc7--lcd-enable)).
Turning the APU off drains less power (around 16%), but clears all APU registers and makes them read-only until turned back on, except `NR52`[1](https://gbdev.io/pandocs/Audio_Registers.html#footnote-dmg_apu_off).
Turning the APU off, however, does not affect [Wave RAM](https://gbdev.io/pandocs/Audio_Registers.html#ff30ff3f--wave-pattern-ram), which can always be read/written, nor the [DIV-APU](https://gbdev.io/pandocs/Audio_details.html#div-apu) counter.

- **CHn on?** ( _Read-only_): Each of these four bits allows checking whether channels are active[2](https://gbdev.io/pandocs/Audio_Registers.html#footnote-nr52_dac).
Writing to those does **not** enable or disable the channels, despite many emulators behaving as if.

A channel is turned on by triggering it (i.e. setting bit 7 of `NRx4`)[3](https://gbdev.io/pandocs/Audio_Registers.html#footnote-dac_off).
A channel is turned off when any of the following occurs:
  - The channel’s [length timer](https://gbdev.io/pandocs/Audio.html#length-timer) is enabled in `NRx4` and expires, or
  - _For CH1 only_: when the [period sweep](https://gbdev.io/pandocs/Audio_Registers.html#ff10--nr10-channel-1-sweep) overflows[4](https://gbdev.io/pandocs/Audio_Registers.html#footnote-freq_sweep_underflow), or
  - [The channel’s DAC](https://gbdev.io/pandocs/Audio_details.html#dacs) is turned off.
    The envelope reaching a volume of 0 does NOT turn the channel off!

* * *

Each channel can be panned hard left, center, hard right, or ignored entirely.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR51** | CH4 left | CH3 left | CH2 left | CH1 left | CH4 right | CH3 right | CH2 right | CH1 right |

Setting a bit to 1 enables the channel to go into the selected output.

Note that selecting or de-selecting a channel whose [DAC](https://gbdev.io/pandocs/Audio_details.html#dacs) is enabled will [cause an audio pop](https://gbdev.io/pandocs/Audio_details.html#mixer).

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR50** | VIN left | Left volume | VIN right | Right volume |

- **VIN left/right**: These work exactly like the bits in [`NR51`](https://gbdev.io/pandocs/Audio_Registers.html#ff25--nr51-sound-panning). They should be set at 0 if external sound hardware is not being used.

- **Left/right volume**: These specify the master volume, i.e. how much each output should be scaled.

A value of 0 is treated as a volume of 1 (very quiet), and a value of 7 is treated as a volume of 8 (no volume reduction).
Importantly, the amplifier **never mutes** a non-silent input.


This register controls CH1’s period sweep functionality.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR10** |  | Pace | Direction | Individual step |

- **Pace**: This dictates how often sweep “iterations” happen, in units of 128 Hz ticks[5](https://gbdev.io/pandocs/Audio_Registers.html#footnote-div_apu) (7.8 ms).
Note that the value written to this field is not re-read by the hardware until a sweep iteration completes, or the channel is [(re)triggered](https://gbdev.io/pandocs/Audio.html#triggering).

However, if `0` is written to this field, then iterations are instantly disabled (but see below), and it will be reloaded as soon as it’s set to something else.

- **Direction**: `0` = Addition (period increases); `1` = Subtraction (period decreases)

- **Individual step**: On each iteration, the new period Lt+1 is computed from the current one Lt as follows:
Lt+1=Lt±Lt2step

On each sweep iteration, the period in [`NR13`](https://gbdev.io/pandocs/Audio_Registers.html#ff13--nr13-channel-1-period-low-write-only) and [`NR14`](https://gbdev.io/pandocs/Audio_Registers.html#ff14--nr14-channel-1-period-high--control) is modified and written back.

In addition mode, if the period value would overflow (i.e. Lt+1 is strictly more than $7FF), the channel is turned off instead.
**This occurs even if sweep iterations are disabled** by the pace being 0.

Note that if the period ever becomes 0, the period sweep will never be able to change it.
For the same reason, the period sweep cannot underflow the period (which would turn the channel off).

This register controls both the channel’s [length timer](https://gbdev.io/pandocs/Audio.html#length-timer) and [duty cycle](https://en.wikipedia.org/wiki/Duty_cycle) (the ratio of the time spent low vs. high).
The selected duty cycle also alters the phase, although the effect is hardly noticeable except in combination with other channels.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR11** | Wave duty | Initial length timer |

- **Duty cycle** ( _Read/Write_): Controls the output waveform as follows:




| Value (binary) | Duty cycle | Waveform |
| --- | --- | --- |
| 00 | 12.5 % |  |
| 01 | 25 % |  |
| 10 | 50 % |  |
| 11 | 75 % |  |



It’s worth noting that there is no audible difference between the 25 % and 75 % duty cycle settings.

- **Initial length timer** ( _Write-only_): The higher this field is, [the shorter the time before the channel is cut](https://gbdev.io/pandocs/Audio.html#length-timer).


This register controls the digital amplitude of the “high” part of the pulse, and the sweep applied to that setting.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR12** | Initial volume | Env dir | Sweep pace |

- **Initial volume**: How loud the channel initially is. Note that these bits are readable, but are **not** updated by the envelope functionality!
- **Env dir**: The envelope’s direction; `0` = decrease volume over time, `1` = increase volume over time.
- **Sweep pace**: The envelope ticks at 64 Hz, and the channel’s envelope will be increased / decreased (depending on bit 3) every Sweep pace of those ticks. A setting of 0 disables the envelope.

Setting bits 3-7 of this register all to 0 (initial volume = 0, envelope = decreasing) turns the DAC off (and thus, the channel as well), which [may cause an audio pop](https://gbdev.io/pandocs/Audio_details.html#mixer).

Writes to this register while the channel is on require retriggering it afterwards.
If the write turns the channel off, retriggering is not necessary (it would do nothing).

This register stores the low 8 bits of the channel’s 11-bit “ [period value](https://gbdev.io/pandocs/Audio.html#frequency)”.
The upper 3 bits are stored in the low 3 bits of `NR14`.

The period divider of pulse and wave channels is an up counter.
Each time it is clocked, its value increases by 1; **when it overflows** (being clocked when it’s already 2047, or $7FF), **its value is set from the contents of `NR13` and `NR14`**.
This means it treats the value in the period as a _negative_ number in 11-bit two’s complement.
The higher the period value in the register, the lower the period, and the higher the frequency.
For example:

- Period value $500 means -$300, or 1 sample per 768 input cycles
- Period value $740 means -$C0, or 1 sample per 192 input cycles

The pulse channels’ period dividers are clocked at 1048576 Hz, once per four dots, and their waveform is 8 samples long.
This makes their sample rate equal to 10485762048-period\_value Hz.
with a resulting tone frequency equal to 1310722048-period\_value Hz.

- Period value $500 means -$300, or 1 sample per 768 input cycles

or (1048576 ÷ 768) = 1365.3 Hz sample rate

or (1048576 ÷ 768 ÷ 8) = 170.67 Hz tone frequency
- Period value $740 means -$C0, or 1 sample per 192 input cycles

or (1048576 ÷ 192) = 5461.3 Hz sample rate

or (1048576 ÷ 192 ÷ 8) = 682.67 Hz tone frequency

Period value $740 produces a higher frequency than $500.
Even though the period value $740 is not four times $500, $740 produces a frequency that is four times that of $500, or two octaves higher, because ($800 - $740) or 192 is one-quarter of ($800 - $500) or 768.

DELAY

Period changes (written to `NR13` or `NR14`) only take effect after the current “sample” ends; see description above.
( [Source](https://github.com/LIJI32/SameSuite/blob/master/apu/channel_1/channel_1_freq_change.asm))

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR14** | Trigger | Length enable |  | Period |

- **Trigger** ( _Write-only_): Writing any value to `NR14` with this bit set [triggers](https://gbdev.io/pandocs/Audio.html#triggering) the channel, causing the
following to occur:
  - Ch1 is enabled.
  - If length timer expired it is reset.
  - The period divider is set to the contents of `NR13` and `NR14`.
  - Envelope timer is reset.
  - Volume is set to contents of `NR12` initial volume.
  - Sweep does [several things](https://gbdev.io/pandocs/Audio_details.html#pulse-channel-with-sweep-ch1).
- **[Length](https://gbdev.io/pandocs/Audio.html#length-timer) enable** ( _Read/Write_): Takes effect immediately upon writing to this register.

- **Period** ( _Write-only_): The upper 3 bits of the period value; the lower 8 bits are stored in [`NR13`](https://gbdev.io/pandocs/Audio_Registers.html#ff13--nr13-channel-1-period-low-write-only).


This sound channel works exactly like channel 1, except that it lacks a period sweep (and thus an equivalent to [`NR10`](https://gbdev.io/pandocs/Audio_Registers.html#ff10--nr10-channel-1-sweep)).
Please refer to the corresponding CH1 register:

- `NR21` ($FF16) → [`NR11`](https://gbdev.io/pandocs/Audio_Registers.html#ff11--nr11-channel-1-length-timer--duty-cycle)
- `NR22` ($FF17) → [`NR12`](https://gbdev.io/pandocs/Audio_Registers.html#ff12--nr12-channel-1-volume--envelope)
- `NR23` ($FF18) → [`NR13`](https://gbdev.io/pandocs/Audio_Registers.html#ff13--nr13-channel-1-period-low-write-only)
- `NR24` ($FF19) → [`NR14`](https://gbdev.io/pandocs/Audio_Registers.html#ff14--nr14-channel-1-period-high--control)

While other channels only offer limited control over the waveform they generate, this channel allows outputting any wave.
It’s thus sometimes called a “voluntary wave” channel.

While the “length” of the wave is fixed at 32 “samples”, 4-bit each, the speed at which it is read can be customized.
It’s possible to “shorten” the wave by either feeding it a repeating pattern, or doubling each sample and doubling the read rate.
It’s also possible to artificially “increase” the wave’s length by loading a new wave as soon as the whole buffer has been read; this is sometimes used for full-on sample playback.

This register controls CH3’s [DAC](https://gbdev.io/pandocs/Audio_details.html#dacs).
Like other channels, turning the DAC off immediately turns the channel off as well.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR30** | DAC on/off |  |

The DAC is often turned off just before writing to [wave RAM](https://gbdev.io/pandocs/Audio_Registers.html#ff30ff3f--wave-pattern-ram) to avoid issues with accessing it; see further below for more info.

Turning the DAC off [may cause an audio pop](https://gbdev.io/pandocs/Audio_details.html#mixer).

This register controls the channel’s [length timer](https://gbdev.io/pandocs/Audio.html#length-timer).

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR31** | Initial length timer |

The higher the [length timer](https://gbdev.io/pandocs/Audio.html#length-timer), the shorter the time before the channel is cut.

This channel lacks the envelope functionality that the other three channels have, and has a much coarser volume control.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR32** |  | Output level |  |

- **Output level**: Controls the channel’s volume as follows:




| Bits 6-5 (binary) | Output level |
| --- | --- |
| 00 | Mute (No sound) |
| 01 | 100% volume (use samples read from Wave RAM as-is) |
| 10 | 50% volume (shift samples read from Wave RAM right once) |
| 11 | 25% volume (shift samples read from Wave RAM right twice) |


This register stores the low 8 bits of the channel’s 11-bit “ [period value](https://gbdev.io/pandocs/Audio.html#frequency)”.
The upper 3 bits are stored in the low 3 bits of `NR34`.

The wave channel’s period divider is clocked at 2097152 Hz, once per two dots, and its waveform is 32 samples long.
This makes their sample rate equal to 20971522048-period\_value Hz.
with a resulting tone frequency equal to 655362048-period\_value Hz.

- Period value $500 means -$300, or 1 sample per 768 input cycles

or (2097152 ÷ 768) = 2730.7 Hz sample rate

or (2097152 ÷ 768 ÷ 32) = 85.333 Hz tone frequency
- Period value $740 means -$C0, or 1 sample per 192 input cycles

or (2097152 ÷ 192) = 10923 Hz sample rate

or (2097152 ÷ 192 ÷ 32) = 341.33 Hz tone frequency

Given the same period value, the tone frequency of the wave channel is generally half that of a pulse channel, or one octave lower.

DELAY

Period changes (written to `NR33` or `NR34`) only take effect after the following time wave RAM is read.
( [Source](https://github.com/LIJI32/SameSuite/blob/master/apu/channel_3/channel_3_freq_change_delay.asm))

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR34** | Trigger | Length enable |  | Period |

- **Trigger** ( _Write-only_): Writing any value to `NR34` with this bit set [triggers](https://gbdev.io/pandocs/Audio.html#triggering) the channel, causing the
following to occur:
  - Ch3 is enabled.

  - If the length timer expired it is reset.

  - The period divider is set to the contents of `NR33` and `NR34`.

  - Volume is set to contents of `NR32` initial volume.

  - Wave RAM index is reset, but its _not_ refilled.



    RETRIGGERING CAUTION



    On monochrome consoles only, retriggering CH3 while it’s about to read a byte from wave RAM causes wave RAM to be corrupted in a generally unpredictable manner.





    PLAYBACK DELAY



    Triggering the wave channel does not immediately start playing wave RAM; instead, the _last_ sample ever read (which is reset to 0 when the APU is off) is output until the channel next reads a sample.
    ( [Source](https://github.com/LIJI32/SameSuite/blob/master/apu/channel_3/channel_3_delay.asm))
- **[Length](https://gbdev.io/pandocs/Audio.html#length-timer) enable** ( _Read/Write_): Takes effect immediately upon writing to this register.

- **Period** ( _Write-only_): The upper 3 bits of the period value; the lower 8 bits are stored in [`NR33`](https://gbdev.io/pandocs/Audio_Registers.html#ff1d--nr33-channel-3-period-low-write-only).


Wave RAM is 16 bytes long; each byte holds two “samples”, each 4 bits.

As CH3 plays, it reads wave RAM left to right, upper nibble first.
That is, $FF30’s upper nibble, $FF30’s lower nibble, $FF31’s upper nibble, and so on.

ACCESS ORDER

When CH3 is started, the first sample read is the one at index _1_, i.e. the lower nibble of the first byte, NOT the upper nibble.
( [Source](https://github.com/LIJI32/SameSuite/blob/master/apu/channel_3/channel_3_first_sample.asm))

Accessing wave RAM while CH3 is **active** (i.e. playing) causes accesses to misbehave:

- On AGB, reads return $FF, and writes are ignored. ( [Source](https://github.com/LIJI32/SameSuite/blob/master/apu/channel_3/channel_3_wave_ram_locked_write.asm))
- On monochrome consoles, wave RAM can only be accessed on the same cycle that CH3 does.
Otherwise, reads return $FF, and writes are ignored.
- On other consoles, the byte accessed will be the one CH3 is currently reading[6](https://gbdev.io/pandocs/Audio_Registers.html#footnote-wave_access); that is, if CH3 is currently reading one of the first two samples, the CPU will really access $FF30, regardless of the address being used. ( [Source](https://github.com/LIJI32/SameSuite/blob/master/apu/channel_3/channel_3_wave_ram_locked_write.asm))

Wave RAM _can_ be accessed normally even if the DAC is on, as long as the channel is not active. ( [Source](https://github.com/LIJI32/SameSuite/blob/master/apu/channel_3/channel_3_wave_ram_dac_on_rw.asm))
This is especially relevant on GBA, whose [mixer behaves as if DACs are always enabled](https://gbdev.io/pandocs/Audio_details.html#game-boy-advance-audio).

This channel is used to output white noise[7](https://gbdev.io/pandocs/Audio_Registers.html#footnote-not_white), which is done by randomly switching the amplitude between two levels fairly fast.

The frequency can be adjusted in order to make the noise appear “harder” (lower frequency) or “softer” (higher frequency).

The random function that switches the output level can also be manipulated.
Certain settings can cause the wave to be more regular, sounding closer to a pulse than noise.

This register controls the channel’s [length timer](https://gbdev.io/pandocs/Audio.html#length-timer).

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR41** |  | Initial length timer |

The higher the [length timer](https://gbdev.io/pandocs/Audio.html#length-timer), the shorter the time before the channel is cut.

This register functions exactly like [`NR12`](https://gbdev.io/pandocs/Audio_Registers.html#ff12--nr12-channel-1-volume--envelope), so please refer to its documentation.

This register allows controlling the way the amplitude is randomly switched.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR43** | Clock shift | LFSR width | Clock divider |

- **Clock shift**: See the frequency formula below.

- **[LFSR](https://gbdev.io/pandocs/Audio_details.html#noise-channel-ch4) width**: `0` = 15-bit, `1` = 7-bit (more regular output; some frequencies sound more like pulse than noise).



LFSR lockup



Switching from 15- to 7-bit mode when the LFSR [is in a certain state](https://gbdev.io/pandocs/Audio_details.html#noise-channel-ch4) can “lock it up”, which essentially silences CH4; this can be avoided by retriggering CH4, which resets the LFSR.

- **Clock divider**: See the frequency formula below.
Note that divider = 0 is treated as divider = 0.5 instead.


The frequency at which the LFSR is clocked is 262144divider×2shift Hz, except that shift being equal to 14 or 15 stops the channel from being clocked entirely.

If the bit shifted out is a 0, the channel emits a 0; otherwise, it emits the volume selected in `NR42`.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **NR44** | Trigger | Length enable |  |

- **Trigger** ( _Write-only_): Writing any value to `NR44` with this bit set [triggers](https://gbdev.io/pandocs/Audio.html#triggering) the channel, causing the
following to occur:
  - Ch4 is enabled.
  - If the length timer expired it is reset.
  - Envelope timer is reset.
  - Volume is set to contents of `NR42` initial volume.
  - [LFSR bits](https://gbdev.io/pandocs/Audio_details.html#noise-channel-ch4) are reset.
- **[Length](https://gbdev.io/pandocs/Audio.html#length-timer) enable** ( _Read/Write_): Takes effect immediately upon writing to this register.


* * *

1. …and the length timers (in `NRx1`) on monochrome models. [↩](https://gbdev.io/pandocs/Audio_Registers.html#fr-dmg_apu_off-1)

2. Actually, only the status of the channels’ _generation_ circuits is reported, not the status of [the DACs](https://gbdev.io/pandocs/Audio_details.html#dacs). A channel can only be ON if its corresponding DAC is, though. [↩](https://gbdev.io/pandocs/Audio_Registers.html#fr-nr52_dac-1)

3. If [the channel’s DAC](https://gbdev.io/pandocs/Audio_details.html#dacs) is off, then the write to NRx4 will be ineffective and won’t turn the channel on. [↩](https://gbdev.io/pandocs/Audio_Registers.html#fr-dac_off-1)

4. The period sweep cannot normally underflow, so a “decreasing” sweep (`NR10` bit 3 set) cannot turn the channel off. [↩](https://gbdev.io/pandocs/Audio_Registers.html#fr-freq_sweep_underflow-1)

5. [As long as `DIV` is not written to](https://gbdev.io/pandocs/Audio_details.html#div-apu). [↩](https://gbdev.io/pandocs/Audio_Registers.html#fr-div_apu-1)

6. The way it works is that wave RAM is a 16-byte memory buffer, and while it’s playing, CH3 has priority over the CPU when choosing which of those 16 bytes is accessed.
So, from the CPU’s point of view, wave RAM reads out the same byte, regardless of the address. [↩](https://gbdev.io/pandocs/Audio_Registers.html#fr-wave_access-1)

7. By default, the noise will sound close to white; but it can be manipulated to sound differently. [↩](https://gbdev.io/pandocs/Audio_Registers.html#fr-not_white-1)