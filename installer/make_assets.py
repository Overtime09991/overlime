"""Erzeugt das App-Symbol (.ico) und die Installer-Bilder. Benötigt Pillow: pip install pillow"""
import math
import os
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "assets")
os.makedirs(OUT, exist_ok=True)

BG1, BG2 = (10, 16, 36), (22, 40, 92)
ACCENT, LIGHT = (0, 112, 224), (80, 165, 255)


def icon_image(size=512):
    """Dunkle, abgerundete Kachel mit leuchtendem Ring und Play-Dreieck."""
    s = size
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    # Verlauf
    grad = Image.new("RGB", (s, s))
    gp = grad.load()
    for y in range(s):
        for x in range(s):
            t = (x + y) / (2 * s)
            gp[x, y] = tuple(int(BG1[i] + (BG2[i] - BG1[i]) * t) for i in range(3))
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, s - 1, s - 1), radius=int(s * 0.23), fill=255)
    img.paste(grad, (0, 0), mask)

    layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    cx = cy = s / 2
    r = s * 0.31
    w = int(s * 0.075)
    # Ring (Lücke unten links = Fortschrittsgefühl)
    d.arc((cx - r, cy - r, cx + r, cy + r), start=-60, end=250, fill=LIGHT + (255,), width=w)
    d.arc((cx - r, cy - r, cx + r, cy + r), start=250, end=300, fill=LIGHT + (90,), width=w)
    # Glühen
    glow = layer.filter(ImageFilter.GaussianBlur(s * 0.025))
    img = Image.alpha_composite(img, glow)
    img = Image.alpha_composite(img, layer)
    # Play-Dreieck
    d2 = ImageDraw.Draw(img)
    k = s * 0.15
    pts = [(cx - k * 0.55, cy - k), (cx - k * 0.55, cy + k), (cx + k * 1.05, cy)]
    d2.polygon(pts, fill=(240, 246, 255, 255))
    # Rand klar halten
    out = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    return out


def font(name, size):
    for p in (rf"C:\Windows\Fonts\{name}", name):
        try:
            return ImageFont.truetype(p, size)
        except Exception:
            pass
    return ImageFont.load_default()


def wizard_image(w=328, h=628):
    img = Image.new("RGB", (w, h))
    px = img.load()
    for y in range(h):
        for x in range(w):
            t = y / h
            px[x, y] = tuple(int(BG1[i] + (BG2[i] - BG1[i]) * (0.25 + 0.75 * t)) for i in range(3))
    glow = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse((-120, h * 0.55, w * 0.9, h * 1.15), fill=ACCENT + (90,))
    gd.ellipse((w * 0.2, -140, w * 1.3, h * 0.25), fill=(110, 70, 240, 60))
    glow = glow.filter(ImageFilter.GaussianBlur(60))
    img = Image.alpha_composite(img.convert("RGBA"), glow)
    ic = icon_image(200)
    img.alpha_composite(ic, ((w - 200) // 2, 80))
    d = ImageDraw.Draw(img)
    f1 = font("segoeuib.ttf", 40)
    f2 = font("segoeui.ttf", 19)
    t1 = "OVERLIME"
    tw = d.textlength(t1, font=f1)
    d.text(((w - tw) / 2, 310), t1, font=f1, fill=(255, 255, 255))
    t2 = "REMOTE PLAY"
    tw = d.textlength(t2, font=f2)
    d.text(((w - tw) / 2, 362), t2, font=f2, fill=LIGHT)
    f3 = font("segoeui.ttf", 17)
    for i, line in enumerate(["PS4 & PS5 auf dem PC", "bis 1080p bei 60 FPS", "Controller & Tastatur"]):
        tw = d.textlength(line, font=f3)
        d.text(((w - tw) / 2, 470 + i * 30), line, font=f3, fill=(170, 190, 225))
    return img.convert("RGB")


def small_image(s=110):
    bg = Image.new("RGBA", (s, s), (255, 255, 255, 0))
    ic = icon_image(s - 10)
    bg.alpha_composite(ic, (5, 5))
    out = Image.new("RGB", (s, s), (255, 255, 255))
    out.paste(bg, (0, 0), bg)
    return out


if __name__ == "__main__":
    base = icon_image(512)
    base.save(os.path.join(OUT, "overlime.png"))
    base.save(os.path.join(OUT, "overlime.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    wizard_image().save(os.path.join(OUT, "wizard.bmp"))
    small_image().save(os.path.join(OUT, "wizard-small.bmp"))
    print("Assets erzeugt in", OUT)
