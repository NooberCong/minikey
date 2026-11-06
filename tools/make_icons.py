"""Draws minikey's icons from geometry (no fonts or source art).

Usage: python tools/make_icons.py [preview.png]
Writes assets/vietnamese.ico and assets/english.ico. Requires Pillow.
"""
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

SIZES = [16, 20, 24, 32, 48, 256]
SS = 8  # supersampling factor
ASSETS = Path(__file__).resolve().parent.parent / "assets"

GREEN = dict(top=(62, 207, 104), bottom=(22, 138, 70), lip=(9, 82, 42), glyph=(255, 255, 255))
BLUE = dict(top=(72, 156, 255), bottom=(22, 92, 208), lip=(10, 50, 124), glyph=(255, 255, 255))


def box(canvas, x0, y0, x1, y1):
    """Scales a box in unit coordinates to canvas pixels."""
    return [x0 * canvas, y0 * canvas, x1 * canvas, y1 * canvas]


def glyph_v(canvas, weight):
    mask = Image.new("L", (canvas, canvas), 0)
    left, right, top, bottom = 0.20, 0.80, 0.20, 0.74
    stroke = weight * 1.3
    centre = (left + right) / 2
    # The inner edges run parallel to the outer ones, meeting on the centre line.
    apex = top + (bottom - top) * (centre - left - stroke) / (centre - left - stroke / 2)
    points = [(left, top), (left + stroke, top), (centre, apex), (right - stroke, top), (right, top),
              (centre + stroke / 2, bottom), (centre - stroke / 2, bottom)]
    ImageDraw.Draw(mask).polygon([(x * canvas, y * canvas) for x, y in points], fill=255)
    return mask


def glyph_e(canvas, weight):
    mask = Image.new("L", (canvas, canvas), 0)
    draw = ImageDraw.Draw(mask)
    left, right, top, bottom = 0.29, 0.73, 0.20, 0.74
    bar = weight * 0.9
    middle = (top + bottom) / 2
    draw.rectangle(box(canvas, left, top, left + weight, bottom), fill=255)
    draw.rectangle(box(canvas, left, top, right, top + bar), fill=255)
    draw.rectangle(box(canvas, left, bottom - bar, right, bottom), fill=255)
    draw.rectangle(box(canvas, left, middle - bar / 2, right - 0.05, middle + bar / 2), fill=255)
    return mask


def gradient(canvas, top, bottom):
    column = Image.new("RGB", (1, canvas))
    for y in range(canvas):
        t = y / (canvas - 1)
        column.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
    return column.resize((canvas, canvas))


def rounded_mask(canvas, rect, radius):
    mask = Image.new("L", (canvas, canvas), 0)
    ImageDraw.Draw(mask).rounded_rectangle(box(canvas, *rect), radius * canvas, fill=255)
    return mask


def render(size, palette, glyph):
    canvas = size * SS
    small = size <= 24  # no room for depth: flat tile, heavier letter
    image = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))

    if small:
        tile = rounded_mask(canvas, (0.02, 0.02, 0.98, 0.98), 0.20)
        image.paste(gradient(canvas, palette["top"], palette["bottom"]), (0, 0), tile)
        weight, shift = 0.145, 0.03
    else:
        body = rounded_mask(canvas, (0.03, 0.03, 0.97, 0.97), 0.21)
        image.paste(Image.new("RGB", (canvas, canvas), palette["lip"]), (0, 0), body)
        face = rounded_mask(canvas, (0.03, 0.03, 0.97, 0.90), 0.20)
        image.paste(gradient(canvas, palette["top"], palette["bottom"]), (0, 0), face)
        sheen = rounded_mask(canvas, (0.05, 0.05, 0.95, 0.88), 0.185)
        sheen = ImageChops.subtract(face, sheen).point(lambda v: v * 0.16)
        image.paste(Image.new("RGB", (canvas, canvas), (255, 255, 255)), (0, 0), sheen)
        weight, shift = 0.125, 0.0

    letter = glyph(canvas, weight)
    letter = ImageChops.offset(letter, 0, round(shift * canvas))
    if not small:
        shadow = ImageChops.offset(letter, 0, round(0.018 * canvas)).point(lambda v: v * 0.45)
        image.paste(Image.new("RGB", (canvas, canvas), palette["lip"]), (0, 0), shadow)
    image.paste(Image.new("RGB", (canvas, canvas), palette["glyph"]), (0, 0), letter)
    return image.resize((size, size), Image.LANCZOS)


def write_ico(name, palette, glyph):
    frames = {size: render(size, palette, glyph) for size in SIZES}
    frames[256].save(ASSETS / name, format="ICO", sizes=[(s, s) for s in SIZES],
                     append_images=[frames[s] for s in SIZES[:-1]])
    return frames


def preview(path, sets):
    backgrounds = [(243, 243, 243), (32, 32, 32)]
    width, height = 900, 2 * 300
    sheet = Image.new("RGB", (width, height))
    for row, background in enumerate(backgrounds):
        sheet.paste(background, (0, row * 300, width, (row + 1) * 300))
        x = 16
        for frames in sets:
            sheet.paste(frames[256].resize((128, 128), Image.LANCZOS), (x, row * 300 + 16),
                        frames[256].resize((128, 128), Image.LANCZOS))
            x += 144
        x = 16
        for frames in sets:
            for size in (16, 24, 32, 48):
                # Real pixels on the left, 4x nearest-neighbour zoom beneath them.
                sheet.paste(frames[size], (x, row * 300 + 156), frames[size])
                zoom = frames[size].resize((size * 3, size * 3), Image.NEAREST)
                sheet.paste(zoom, (x, row * 300 + 156 + size + 4), zoom) if size <= 24 else None
                x += max(size, 16) * (3 if size <= 24 else 1) + 16
    sheet.save(path)


if __name__ == "__main__":
    ASSETS.mkdir(exist_ok=True)
    vietnamese = write_ico("vietnamese.ico", GREEN, glyph_v)
    english = write_ico("english.ico", BLUE, glyph_e)
    if len(sys.argv) > 1:
        preview(sys.argv[1], [vietnamese, english])
    for name in ("vietnamese.ico", "english.ico"):
        print(name, (ASSETS / name).stat().st_size, "bytes")
