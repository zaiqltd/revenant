# REVENANT — Accuracy Scorecard

**Canonical gate set: passes 127/279**

| Category | Pass | Total |
|---|---:|---:|
| acid2 | 3 | 3 |
| age | 2 | 47 |
| blargg | 17 | 20 |
| mealybug | 3 | 24 |
| mooneye/acceptance | 56 | 75 |
| mooneye/emulator-only | 28 | 28 |
| same-suite | 14 | 78 |
| scribbl | 4 | 4 |

<details><summary>Per-ROM detail</summary>

| Category | ROM | Status | Detail |
|---|---|---|---|
| acid2 | cgb-acid2 | ✅ PASS | differing pixels: 0/23040 (0.000%) |
| acid2 | dmg-acid2 | ✅ PASS | differing pixels: 0/23040 (0.000%) |
| acid2 | hell/cgb-acid-hell | ✅ PASS | differing pixels: 2/23040 (0.009%) |
| age | halt/ei-halt-dmgC-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | halt/halt-m0-interrupt-dmgC-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | halt/halt-prefetch-dmgC-cgbBCE | ✅ PASS | magic |
| age | lcd-align-ly/lcd-align-ly-cgbBC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | lcd-align-ly/lcd-align-ly-cgbE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | ly/ly-cgbE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | ly/ly-dmgC-cgbBC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | ly/ly-ncmBC | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| age | ly/ly-ncmE | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| age | m3-bg-bgp/m3-bg-bgp | ❌ FAIL | regs b=e4 c=47 d=00 e=d8 h=1d l=f0 |
| age | m3-bg-lcdc/m3-bg-lcdc | ❌ FAIL | regs b=10 c=40 d=33 e=0f h=1e l=4a |
| age | m3-bg-lcdc/m3-bg-lcdc-ds | ❌ FAIL | regs b=10 c=40 d=33 e=0f h=1e l=6d |
| age | m3-bg-lcdc/m3-bg-lcdc-nocgb | ❌ FAIL | regs b=10 c=40 d=33 e=0f h=1e l=4a |
| age | m3-bg-scx/m3-bg-scx | ❌ FAIL | regs b=47 c=00 d=02 e=56 h=1e l=35 |
| age | m3-bg-scx/m3-bg-scx-ds | ❌ FAIL | regs b=47 c=00 d=02 e=56 h=1e l=3f |
| age | m3-bg-scx/m3-bg-scx-nocgb | ❌ FAIL | regs b=47 c=00 d=02 e=d8 h=1e l=35 |
| age | oam/oam-read-cgbE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | oam/oam-read-dmgC-cgbBC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | oam/oam-read-ncmBC | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| age | oam/oam-read-ncmE | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| age | oam/oam-write-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | oam/oam-write-dmgC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | oam/oam-write-ncmBCE | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| age | speed-switch/caution/spsw-interrupts-cgbBC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | speed-switch/caution/spsw-interrupts-cgbE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | speed-switch/spsw-ch2-lc-delay-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | speed-switch/spsw-div-cgbBCE | ✅ PASS | magic |
| age | speed-switch/spsw-mode0-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | speed-switch/spsw-stop-prefetch-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | speed-switch/spsw-tima-cgbBC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | speed-switch/spsw-tima-cgbE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-interrupt/stat-int-dmgC-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-interrupt/stat-int-ncmBCE | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| age | stat-mode-sprites/stat-mode-sprites-dmgC-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode-sprites/stat-mode-sprites-ds-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode-window/stat-mode-window-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode-window/stat-mode-window-dmgC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode-window/stat-mode-window-ds-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode-window/stat-mode-window-ncmBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode/stat-mode-cgbE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode/stat-mode-dmgC-cgbBC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode/stat-mode-ds-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | stat-mode/stat-mode-ncmBC | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| age | stat-mode/stat-mode-ncmE | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| age | vram/vram-read-cgbBCE | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | vram/vram-read-dmgC | ❌ FAIL | regs b=00 c=6b d=14 e=06 h=98 l=10 |
| age | vram/vram-read-ncmBCE | ❌ FAIL | regs b=00 c=6b d=14 e=19 h=98 l=10 |
| blargg | cgb_sound | ❌ FAIL | Failed #8 |
| blargg | cpu_instrs | ✅ PASS | done |
| blargg | cpu_instrs/01-special | ✅ PASS | done |
| blargg | cpu_instrs/02-interrupts | ✅ PASS | done |
| blargg | cpu_instrs/03-op sp,hl | ✅ PASS | done |
| blargg | cpu_instrs/04-op r,imm | ✅ PASS | done |
| blargg | cpu_instrs/05-op rp | ✅ PASS | done |
| blargg | cpu_instrs/06-ld r,r | ✅ PASS | done |
| blargg | cpu_instrs/07-jr,jp,call,ret,rst | ✅ PASS | done |
| blargg | cpu_instrs/08-misc instrs | ✅ PASS | done |
| blargg | cpu_instrs/09-op r,r | ✅ PASS | done |
| blargg | cpu_instrs/10-bit ops | ✅ PASS | done |
| blargg | cpu_instrs/11-op a,(hl) | ✅ PASS | done |
| blargg | dmg_sound | ❌ FAIL | Failed #9 |
| blargg | halt_bug | ✅ PASS | done |
| blargg | instr_timing | ✅ PASS | done |
| blargg | interrupt_time | ✅ PASS | done |
| blargg | mem_timing | ✅ PASS | done |
| blargg | mem_timing-2 | ✅ PASS | done |
| blargg | oam_bug | ❌ FAIL | Failed |
| mealybug | m2_win_en_toggle | ✅ PASS | differing pixels: 0/23040 (0.000%) |
| mealybug | m3_bgp_change | ❌ FAIL | differing pixels: 8684/23040 (37.691%) |
| mealybug | m3_bgp_change_sprites | ❌ FAIL | differing pixels: 9808/23040 (42.569%) |
| mealybug | m3_lcdc_bg_en_change | ❌ FAIL | differing pixels: 3413/23040 (14.813%) |
| mealybug | m3_lcdc_bg_map_change | ❌ FAIL | differing pixels: 978/23040 (4.245%) |
| mealybug | m3_lcdc_obj_en_change | ❌ FAIL | differing pixels: 146/23040 (0.634%) |
| mealybug | m3_lcdc_obj_en_change_variant | ❌ FAIL | differing pixels: 1334/23040 (5.790%) |
| mealybug | m3_lcdc_obj_size_change | ❌ FAIL | differing pixels: 350/23040 (1.519%) |
| mealybug | m3_lcdc_obj_size_change_scx | ❌ FAIL | differing pixels: 190/23040 (0.825%) |
| mealybug | m3_lcdc_tile_sel_change | ❌ FAIL | differing pixels: 2062/23040 (8.950%) |
| mealybug | m3_lcdc_tile_sel_win_change | ❌ FAIL | differing pixels: 2754/23040 (11.953%) |
| mealybug | m3_lcdc_win_en_change_multiple | ❌ FAIL | differing pixels: 8316/23040 (36.094%) |
| mealybug | m3_lcdc_win_en_change_multiple_wx | ❌ FAIL | differing pixels: 6041/23040 (26.220%) |
| mealybug | m3_lcdc_win_map_change | ❌ FAIL | differing pixels: 2180/23040 (9.462%) |
| mealybug | m3_obp0_change | ❌ FAIL | differing pixels: 432/23040 (1.875%) |
| mealybug | m3_scx_high_5_bits | ✅ PASS | differing pixels: 84/23040 (0.365%) |
| mealybug | m3_scx_low_3_bits | ❌ FAIL | differing pixels: 540/23040 (2.344%) |
| mealybug | m3_scy_change | ❌ FAIL | differing pixels: 11332/23040 (49.184%) |
| mealybug | m3_window_timing | ❌ FAIL | differing pixels: 2787/23040 (12.096%) |
| mealybug | m3_window_timing_wx_0 | ❌ FAIL | differing pixels: 3228/23040 (14.010%) |
| mealybug | m3_wx_4_change | ❌ FAIL | differing pixels: 10138/23040 (44.002%) |
| mealybug | m3_wx_4_change_sprites | ✅ PASS | differing pixels: 10/23040 (0.043%) |
| mealybug | m3_wx_5_change | ❌ FAIL | differing pixels: 9521/23040 (41.324%) |
| mealybug | m3_wx_6_change | ❌ FAIL | differing pixels: 13281/23040 (57.643%) |
| mooneye/acceptance | add_sp_e_timing | ✅ PASS | magic |
| mooneye/acceptance | bits/mem_oam | ✅ PASS | magic |
| mooneye/acceptance | bits/reg_f | ✅ PASS | magic |
| mooneye/acceptance | bits/unused_hwio-GS | ✅ PASS | magic |
| mooneye/acceptance | boot_div-S | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_div-dmg0 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_div-dmgABCmgb | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_div2-S | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_hwio-S | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_hwio-dmg0 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_hwio-dmgABCmgb | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_regs-dmg0 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_regs-dmgABC | ✅ PASS | magic |
| mooneye/acceptance | boot_regs-mgb | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_regs-sgb | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | boot_regs-sgb2 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | call_cc_timing | ✅ PASS | magic |
| mooneye/acceptance | call_cc_timing2 | ✅ PASS | magic |
| mooneye/acceptance | call_timing | ✅ PASS | magic |
| mooneye/acceptance | call_timing2 | ✅ PASS | magic |
| mooneye/acceptance | di_timing-GS | ✅ PASS | magic |
| mooneye/acceptance | div_timing | ✅ PASS | magic |
| mooneye/acceptance | ei_sequence | ✅ PASS | magic |
| mooneye/acceptance | ei_timing | ✅ PASS | magic |
| mooneye/acceptance | halt_ime0_ei | ✅ PASS | magic |
| mooneye/acceptance | halt_ime0_nointr_timing | ✅ PASS | magic |
| mooneye/acceptance | halt_ime1_timing | ✅ PASS | magic |
| mooneye/acceptance | halt_ime1_timing2-GS | ✅ PASS | magic |
| mooneye/acceptance | if_ie_registers | ✅ PASS | magic |
| mooneye/acceptance | instr/daa | ✅ PASS | magic |
| mooneye/acceptance | interrupts/ie_push | ✅ PASS | magic |
| mooneye/acceptance | intr_timing | ✅ PASS | magic |
| mooneye/acceptance | jp_cc_timing | ✅ PASS | magic |
| mooneye/acceptance | jp_timing | ✅ PASS | magic |
| mooneye/acceptance | ld_hl_sp_e_timing | ✅ PASS | magic |
| mooneye/acceptance | oam_dma/basic | ✅ PASS | magic |
| mooneye/acceptance | oam_dma/reg_read | ✅ PASS | magic |
| mooneye/acceptance | oam_dma/sources-GS | ✅ PASS | magic |
| mooneye/acceptance | oam_dma_restart | ✅ PASS | magic |
| mooneye/acceptance | oam_dma_start | ✅ PASS | magic |
| mooneye/acceptance | oam_dma_timing | ✅ PASS | magic |
| mooneye/acceptance | pop_timing | ✅ PASS | magic |
| mooneye/acceptance | ppu/hblank_ly_scx_timing-GS | ✅ PASS | magic |
| mooneye/acceptance | ppu/intr_1_2_timing-GS | ✅ PASS | magic |
| mooneye/acceptance | ppu/intr_2_0_timing | ✅ PASS | magic |
| mooneye/acceptance | ppu/intr_2_mode0_timing | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | ppu/intr_2_mode0_timing_sprites | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | ppu/intr_2_mode3_timing | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | ppu/intr_2_oam_ok_timing | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | ppu/lcdon_timing-GS | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | ppu/lcdon_write_timing-GS | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | ppu/stat_irq_blocking | ✅ PASS | magic |
| mooneye/acceptance | ppu/stat_lyc_onoff | ✅ PASS | magic |
| mooneye/acceptance | ppu/vblank_stat_intr-GS | ✅ PASS | magic |
| mooneye/acceptance | push_timing | ✅ PASS | magic |
| mooneye/acceptance | rapid_di_ei | ✅ PASS | magic |
| mooneye/acceptance | ret_cc_timing | ✅ PASS | magic |
| mooneye/acceptance | ret_timing | ✅ PASS | magic |
| mooneye/acceptance | reti_intr_timing | ✅ PASS | magic |
| mooneye/acceptance | reti_timing | ✅ PASS | magic |
| mooneye/acceptance | rst_timing | ✅ PASS | magic |
| mooneye/acceptance | serial/boot_sclk_align-dmgABCmgb | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | timer/div_write | ✅ PASS | magic |
| mooneye/acceptance | timer/rapid_toggle | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| mooneye/acceptance | timer/tim00 | ✅ PASS | magic |
| mooneye/acceptance | timer/tim00_div_trigger | ✅ PASS | magic |
| mooneye/acceptance | timer/tim01 | ✅ PASS | magic |
| mooneye/acceptance | timer/tim01_div_trigger | ✅ PASS | magic |
| mooneye/acceptance | timer/tim10 | ✅ PASS | magic |
| mooneye/acceptance | timer/tim10_div_trigger | ✅ PASS | magic |
| mooneye/acceptance | timer/tim11 | ✅ PASS | magic |
| mooneye/acceptance | timer/tim11_div_trigger | ✅ PASS | magic |
| mooneye/acceptance | timer/tima_reload | ✅ PASS | magic |
| mooneye/acceptance | timer/tima_write_reloading | ✅ PASS | magic |
| mooneye/acceptance | timer/tma_write_reloading | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/bits_bank1 | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/bits_bank2 | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/bits_mode | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/bits_ramg | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/multicart_rom_8Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/ram_256kb | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/ram_64kb | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/rom_16Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/rom_1Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/rom_2Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/rom_4Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/rom_512kb | ✅ PASS | magic |
| mooneye/emulator-only | mbc1/rom_8Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc2/bits_ramg | ✅ PASS | magic |
| mooneye/emulator-only | mbc2/bits_romb | ✅ PASS | magic |
| mooneye/emulator-only | mbc2/bits_unused | ✅ PASS | magic |
| mooneye/emulator-only | mbc2/ram | ✅ PASS | magic |
| mooneye/emulator-only | mbc2/rom_1Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc2/rom_2Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc2/rom_512kb | ✅ PASS | magic |
| mooneye/emulator-only | mbc5/rom_16Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc5/rom_1Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc5/rom_2Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc5/rom_32Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc5/rom_4Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc5/rom_512kb | ✅ PASS | magic |
| mooneye/emulator-only | mbc5/rom_64Mb | ✅ PASS | magic |
| mooneye/emulator-only | mbc5/rom_8Mb | ✅ PASS | magic |
| same-suite | apu/channel_1/channel_1_align | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_align_cpu | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_duty | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_duty_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_extra_length_clocking-cgb0B | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_freq_change | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_freq_change_timing-A | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_freq_change_timing-cgb0BC | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_freq_change_timing-cgbDE | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_nrx2_glitch | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_nrx2_speed_change | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_restart | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_restart_nrx2_glitch | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_stop_div | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_stop_restart | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_sweep | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_sweep_restart | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_sweep_restart_2 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_volume | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_1/channel_1_volume_div | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_align | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_align_cpu | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_duty | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_duty_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_extra_length_clocking-cgb0B | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_freq_change | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_nrx2_glitch | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_nrx2_speed_change | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_restart | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_restart_nrx2_glitch | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_stop_div | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_stop_restart | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_volume | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_2/channel_2_volume_div | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_and_glitch | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_extra_length_clocking-cgb0 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_extra_length_clocking-cgbB | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_first_sample | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_freq_change_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_restart_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_restart_during_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_restart_stop_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_shift_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_shift_skip_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_stop_delay | ✅ PASS | magic |
| same-suite | apu/channel_3/channel_3_stop_div | ✅ PASS | magic |
| same-suite | apu/channel_3/channel_3_wave_ram_dac_on_rw | ✅ PASS | magic |
| same-suite | apu/channel_3/channel_3_wave_ram_locked_write | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_3/channel_3_wave_ram_sync | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_align | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_delay | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_equivalent_frequencies | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_extra_length_clocking-cgb0B | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_freq_change | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_frequency_alignment | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_lfsr | ✅ PASS | magic |
| same-suite | apu/channel_4/channel_4_lfsr15 | ✅ PASS | magic |
| same-suite | apu/channel_4/channel_4_lfsr_15_7 | ✅ PASS | magic |
| same-suite | apu/channel_4/channel_4_lfsr_7_15 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_lfsr_restart | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_lfsr_restart_fast | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/channel_4/channel_4_volume_div | ✅ PASS | magic |
| same-suite | apu/div_trigger_volume_10 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/div_write_trigger | ✅ PASS | magic |
| same-suite | apu/div_write_trigger_10 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | apu/div_write_trigger_volume | ✅ PASS | magic |
| same-suite | apu/div_write_trigger_volume_10 | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | dma/gbc_dma_cont | ✅ PASS | magic |
| same-suite | dma/gdma_addr_mask | ✅ PASS | magic |
| same-suite | dma/hdma_lcd_off | ✅ PASS | magic |
| same-suite | dma/hdma_mode0 | ✅ PASS | magic |
| same-suite | interrupt/ei_delay_halt | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | ppu/blocking_bgpi_increase | ✅ PASS | magic |
| same-suite | sgb/command_mlt_req | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| same-suite | sgb/command_mlt_req_1_incrementing | ❌ FAIL | regs b=42 c=42 d=42 e=42 h=42 l=42 |
| scribbl | lycscx | ✅ PASS | differing pixels: 0/23040 (0.000%) |
| scribbl | lycscy | ✅ PASS | differing pixels: 0/23040 (0.000%) |
| scribbl | palettely | ✅ PASS | differing pixels: 0/23040 (0.000%) |
| scribbl | scxly | ✅ PASS | differing pixels: 0/23040 (0.000%) |

</details>
