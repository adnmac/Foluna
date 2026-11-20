#!/usr/bin/env python3
"""Icône de Foluna pour Windows : les calques SVG de l'icône Mac (Icon/AppIcon.icon) posés sur son fond bleu
nuit, en carré arrondi, sans les effets de verre de macOS. Écrit desktop/app/icon/foluna.png (1024 px) ;
`npx tauri icon icon/foluna.png -o src-tauri/icons` en tire les tailles de Windows.

Nécessite rsvg-convert (librsvg) et Pillow."""
import pathlib
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[2]
ASSETS = ROOT / "Icon" / "AppIcon.icon" / "Assets"
OUT = ROOT / "desktop" / "app" / "icon" / "foluna.png"
SIZE = 1024

# Couleurs de Icon/AppIcon.icon/icon.json (apparence claire), opacité des étoiles comprise.
LAYERS = [("moon.svg", (248, 249, 182), 1.0), ("book.svg", (199, 212, 255), 1.0), ("stars.svg", (199, 212, 255), 0.6)]
BACKGROUND = (33, 41, 107)


def layer(svg: pathlib.Path, color, opacity, tmp: pathlib.Path) -> Image.Image:
    png = tmp / (svg.stem + ".png")
    subprocess.run(["rsvg-convert", "-w", str(SIZE), "-h", str(SIZE), "-o", str(png), str(svg)], check=True)
    mask = Image.open(png).convert("RGBA").getchannel("A").point(lambda a: int(a * opacity))
    tinted = Image.new("RGBA", (SIZE, SIZE), color + (255,))
    tinted.putalpha(mask)
    return tinted


def main() -> int:
    # Fond : dégradé vertical du bleu nuit de l'icône, un peu plus clair en haut.
    top = tuple(min(255, int(c * 1.45)) for c in BACKGROUND)
    background = Image.new("RGBA", (SIZE, SIZE))
    draw = ImageDraw.Draw(background)
    for y in range(SIZE):
        t = y / (SIZE - 1)
        draw.line([(0, y), (SIZE, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(top, BACKGROUND)) + (255,))
    shape = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(shape).rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=SIZE // 5, fill=255)
    icon = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    icon.paste(background, mask=shape)
    with tempfile.TemporaryDirectory() as tmp:
        for name, color, opacity in LAYERS:
            icon = Image.alpha_composite(icon, layer(ASSETS / name, color, opacity, pathlib.Path(tmp)))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    icon.save(OUT)
    print(f"écrit : {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
