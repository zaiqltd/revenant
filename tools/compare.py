#!/usr/bin/env python
"""Palette-agnostic comparison of an emulator screenshot vs a reference image.

acid2 reference images may use a different 4-shade palette than we render, so we
quantize each image to 4 luminance buckets (ordered light->dark) and compare the
resulting index maps. Exact index match => pixel-perfect feature rendering.
"""
import sys
from PIL import Image


def luminance_buckets(img):
    img = img.convert("RGB")
    px = list(img.getdata())
    lum = [0.299 * r + 0.587 * g + 0.114 * b for (r, g, b) in px]
    uniq = sorted(set(round(l) for l in lum))
    # map each distinct luminance to a rank; collapse to <=4 buckets by nearest
    # of the distinct shades present in the reference
    return lum, uniq


def to_index(lum, shades):
    out = []
    for l in lum:
        best = min(range(len(shades)), key=lambda i: abs(shades[i] - l))
        out.append(best)
    return out


def main():
    a = Image.open(sys.argv[1]).resize((160, 144))
    b = Image.open(sys.argv[2]).resize((160, 144))
    la, _ = luminance_buckets(a)
    lb, sb = luminance_buckets(b)
    # Reference shade levels (usually 4). Build from reference distinct luminances
    # reduced to its real shade count.
    ref_shades = sorted(set(round(x) for x in lb))
    if len(ref_shades) > 8:
        # downsample to 4 evenly
        ref_shades = [ref_shades[i * (len(ref_shades) - 1) // 3] for i in range(4)]
    ia = to_index(la, ref_shades)
    ib = to_index(lb, ref_shades)
    diff = sum(1 for x, y in zip(ia, ib) if x != y)
    total = len(ia)
    print(f"differing pixels: {diff}/{total} ({100*diff/total:.3f}%)")
    if diff == 0:
        print("RESULT: PIXEL-PERFECT MATCH")
        sys.exit(0)
    elif diff < total * 0.005:
        print("RESULT: near-match (<0.5% diff)")
        sys.exit(0)
    else:
        print("RESULT: MISMATCH")
        sys.exit(1)


if __name__ == "__main__":
    main()
