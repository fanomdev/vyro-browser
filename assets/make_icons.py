"""Generate original VYRO raster icons from the logo geometry (Pillow only)."""
from PIL import Image, ImageDraw, ImageFilter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "src-tauri" / "icons"
OUT.mkdir(parents=True, exist_ok=True)

S = 1024  # supersample


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def make_base():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # diagonal gradient background
    grad = Image.new("RGBA", (S, S))
    px = grad.load()
    c0, c1 = (20, 18, 39), (10, 10, 18)
    for y in range(S):
        for x in range(S):
            px[x, y] = (*lerp(c0, c1, (x + y) / (2 * S)), 255)
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle((16, 16, S - 16, S - 16), radius=224, fill=255)
    img.paste(grad, (0, 0), mask)

    d = ImageDraw.Draw(img)
    d.rounded_rectangle((18, 18, S - 18, S - 18), radius=222,
                        outline=(124, 92, 255, 90), width=3)

    # gradient for the V
    vg = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    vp = vg.load()
    a, b = (155, 123, 255), (34, 211, 238)
    for y in range(S):
        for x in range(S):
            vp[x, y] = (*lerp(a, b, (x + y) / (2 * S)), 255)
    vmask = Image.new("L", (S, S), 0)
    vd = ImageDraw.Draw(vmask)
    # left shard, right shard
    vd.polygon([(224, 256), (392, 256), (512, 760), (352, 760)], fill=255)
    vd.polygon([(632, 256), (800, 256), (672, 760), (512, 760)], fill=255)
    # spark
    vd.polygon([(512, 150), (540, 200), (512, 250), (484, 200)], fill=255)

    # glow
    glow = vg.copy()
    glow.putalpha(vmask.point(lambda v: int(v * 0.55)))
    glow = glow.filter(ImageFilter.GaussianBlur(28))
    img.alpha_composite(glow)
    img.paste(vg, (0, 0), vmask)
    return img


base = make_base()
sizes = [16, 24, 32, 48, 64, 128, 256, 512, 1024]
for sz in sizes:
    base.resize((sz, sz), Image.LANCZOS).save(OUT / f"icon-{sz}.png")
base.resize((512, 512), Image.LANCZOS).save(OUT / "icon.png")
base.resize((256, 256), Image.LANCZOS).save(OUT / "icon.ico",
                                           sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("Generated icons in", OUT)
