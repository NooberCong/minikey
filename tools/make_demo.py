"""Draws the README's logo and typing demo.

Usage: python tools/make_demo.py
Writes docs/logo.png and docs/typing.gif. Requires Pillow and the Segoe UI and
Consolas fonts. The text in every frame comes from the real engine, by way of
examples/trace.rs, so the demo cannot drift from what minikey types.
"""
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from make_icons import BLUE, GREEN, glyph_e, glyph_v, render

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
FONTS = Path("C:/Windows/Fonts")

SS = 3  # supersampling factor
WIDTH, HEIGHT = 720, 216
PAD = 32
PAPER, INK, MUTED, RULE = (22, 27, 34), (240, 246, 252), (139, 148, 158), (48, 54, 61)
ACCENT = (62, 207, 104)

KEY_MS, HOLD_MS = 170, 2200
DEMOS = [("Telex", "telex", "Xin chaof Vieetj Nam"), ("VNI", "vni", "Xin chao2 Vie65t Nam")]


def font(name, size):
    return ImageFont.truetype(str(FONTS / name), size * SS)


LABEL, CHIP = font("segoeui.ttf", 15), font("segoeuib.ttf", 15)
KEYS, TEXT = font("consola.ttf", 24), font("segoeui.ttf", 42)


def trace(method, keys):
    """What the engine puts on screen after each key."""
    output = subprocess.run(
        ["cargo", "run", "-q", "--example", "trace", "--", method, keys],
        cwd=ROOT, capture_output=True, encoding="utf-8", check=True,
    ).stdout
    return [line.split("\t", 1)[1] for line in output.split("\n") if line]


def frame(title, keys, typed, screen):
    image = Image.new("RGB", (WIDTH * SS, HEIGHT * SS), PAPER)
    draw = ImageDraw.Draw(image)

    def at(x, y):
        return x * SS, y * SS

    # Which method this is.
    chip = draw.textlength(title, font=CHIP) / SS + 24
    draw.rounded_rectangle([*at(WIDTH - PAD - chip, 24), *at(WIDTH - PAD, 52)], 14 * SS, fill=ACCENT)
    draw.text(at(WIDTH - PAD - chip / 2, 38), title, font=CHIP, fill=PAPER, anchor="mm")

    # The keys pressed so far, with the latest one lit.
    draw.text(at(PAD, 24), "YOU PRESS", font=LABEL, fill=MUTED)
    done = keys[:typed]
    draw.text(at(PAD, 50), done[:-1], font=KEYS, fill=MUTED)
    draw.text((PAD * SS + draw.textlength(done[:-1], font=KEYS), 50 * SS), done[-1:], font=KEYS, fill=ACCENT)

    draw.line([*at(PAD, 96), *at(WIDTH - PAD, 96)], fill=RULE, width=SS)

    # What the app shows, with a caret.
    draw.text(at(PAD, 108), "YOU GET", font=LABEL, fill=MUTED)
    draw.text(at(PAD, 134), screen, font=TEXT, fill=INK)
    caret = PAD * SS + draw.textlength(screen, font=TEXT) + 3 * SS
    draw.rectangle([caret, 144 * SS, caret + 2 * SS, 190 * SS], fill=ACCENT)

    return image.resize((WIDTH, HEIGHT), Image.LANCZOS)


def typing_gif(path):
    frames, durations = [], []
    for title, method, keys in DEMOS:
        steps = trace(method, keys)
        assert len(steps) == len(keys)
        for typed, screen in enumerate(steps, 1):
            frames.append(frame(title, keys, typed, screen))
            durations.append(KEY_MS)
        durations[-1] = HOLD_MS
    # One shared palette keeps the file small and the colors steady.
    palette = frames[-1].quantize(colors=64, method=Image.MEDIANCUT)
    frames = [f.quantize(palette=palette, dither=Image.NONE) for f in frames]
    frames[0].save(path, save_all=True, append_images=frames[1:], duration=durations, loop=0, optimize=True)


def logo(path):
    size, gap = 160, 20
    sheet = Image.new("RGBA", (2 * size + gap, size), (0, 0, 0, 0))
    for x, (palette, glyph) in zip((0, size + gap), ((GREEN, glyph_v), (BLUE, glyph_e))):
        cap = render(256, palette, glyph).resize((size, size), Image.LANCZOS)
        sheet.paste(cap, (x, 0), cap)
    sheet.save(path, optimize=True)


if __name__ == "__main__":
    DOCS.mkdir(exist_ok=True)
    logo(DOCS / "logo.png")
    typing_gif(DOCS / "typing.gif")
    for name in ("logo.png", "typing.gif"):
        print(name, (DOCS / name).stat().st_size, "bytes")
