"""
Generate classic microphone tray icons for wispr-local.

Variants:
  tray_idle.png   -- soft light gray, "armed / waiting"
  tray_active.png -- warm red, "recording"

The icon uses the familiar vocal-mic silhouette:
    rounded capsule (top) with grille lines -> U-shaped holder -> stem -> base
Drawn at high supersample, downscaled with Lanczos for crisp edges at 32x32.
"""

from PIL import Image, ImageDraw
import os

OUT_DIR = "assets"
os.makedirs(OUT_DIR, exist_ok=True)

SCALE = 8     # 32 * 8 = 256 supersample
FINAL = 32    # final output size


def _pt(x, y):
    return (x * SCALE, y * SCALE)


def _rect(x0, y0, x1, y1):
    return [_pt(x0, y0), _pt(x1, y1)]


def draw_mic(fill_rgba, bg_rgba=None):
    """Classic microphone silhouette for idle state.

    If bg_rgba is provided, draws a rounded dark square behind the mic so the
    icon stays visible against both light and dark tray backgrounds.
    """
    s = FINAL * SCALE
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Dark rounded background (leaving 1px margin for anti-aliasing).
    if bg_rgba is not None:
        d.rounded_rectangle(_rect(1, 1, 31, 31),
                            radius=7 * SCALE, fill=bg_rgba)

    # Capsule (mic head).
    d.rounded_rectangle(_rect(10, 3, 22, 19), radius=6 * SCALE, fill=fill_rgba)

    # Grille.
    grille = (
        max(0, fill_rgba[0] - 70),
        max(0, fill_rgba[1] - 45),
        max(0, fill_rgba[2] - 45),
        fill_rgba[3],
    )
    for gy in (7, 11, 15):
        d.rectangle(_rect(12, gy, 20, gy + 1), fill=grille)

    # U-shaped holder.
    d.arc(_rect(7, 14, 25, 24), start=20, end=160,
          fill=fill_rgba, width=2 * SCALE)

    # Stem.
    d.rectangle(_rect(15, 23, 17, 27), fill=fill_rgba)

    # Base.
    d.rounded_rectangle(_rect(10, 27, 22, 29),
                        radius=1 * SCALE, fill=fill_rgba)

    return img.resize((FINAL, FINAL), Image.LANCZOS)


def draw_waveform(fill_rgba):
    """Audio waveform bars with flanking dots for active/recording state."""
    s = FINAL * SCALE
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    cy = 16  # vertical center
    bar_radius = 1.2 * SCALE

    # 7 bars, center tallest, symmetric decay toward the ends.
    # (x_center, half_height) pairs on a 32-grid.
    bars = [
        (7,  4),
        (11, 7),
        (14, 10),
        (17, 12),   # tallest (center-right)
        (20, 10),
        (23, 7),
        (26, 4),
    ]
    for cx, hh in bars:
        x0, x1 = cx - 1.2, cx + 1.2
        y0, y1 = cy - hh, cy + hh
        d.rounded_rectangle(_rect(x0, y0, x1, y1),
                            radius=bar_radius, fill=fill_rgba)

    # Flanking dots.
    dot_r = 1.3 * SCALE
    for cx in (3, 30):
        d.ellipse(
            [
                ((cx - 1.3) * SCALE, (cy - 1.3) * SCALE),
                ((cx + 1.3) * SCALE, (cy + 1.3) * SCALE),
            ],
            fill=fill_rgba,
        )

    return img.resize((FINAL, FINAL), Image.LANCZOS)


# Idle: light mic on dark rounded background. Active: warm red waveform bars.
idle = draw_mic((240, 240, 245, 255), bg_rgba=(24, 24, 28, 230))
active = draw_waveform((235, 55, 55, 255))

idle.save(os.path.join(OUT_DIR, "tray_idle.png"))
active.save(os.path.join(OUT_DIR, "tray_active.png"))


def draw_mic_at(size, fill_rgba, bg_rgba):
    """Re-render the mic silhouette at an arbitrary output size."""
    s = size * SCALE
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    def pt(x, y):
        return (x * SCALE, y * SCALE)

    def rect(x0, y0, x1, y1):
        return [pt(x0, y0), pt(x1, y1)]

    # Scale coords relative to `size`-grid.
    m = size / 32.0
    d.rounded_rectangle(
        rect(1 * m, 1 * m, 31 * m, 31 * m),
        radius=int(7 * m * SCALE),
        fill=bg_rgba,
    )
    d.rounded_rectangle(
        rect(10 * m, 3 * m, 22 * m, 19 * m),
        radius=int(6 * m * SCALE),
        fill=fill_rgba,
    )
    grille = (max(0, fill_rgba[0] - 70),
              max(0, fill_rgba[1] - 45),
              max(0, fill_rgba[2] - 45),
              fill_rgba[3])
    for gy in (7, 11, 15):
        d.rectangle(rect(12 * m, gy * m, 20 * m, (gy + 1) * m), fill=grille)
    d.arc(rect(7 * m, 14 * m, 25 * m, 24 * m),
          start=20, end=160, fill=fill_rgba, width=int(2 * m * SCALE))
    d.rectangle(rect(15 * m, 23 * m, 17 * m, 27 * m), fill=fill_rgba)
    d.rounded_rectangle(rect(10 * m, 27 * m, 22 * m, 29 * m),
                        radius=int(1 * m * SCALE), fill=fill_rgba)
    return img.resize((size, size), Image.LANCZOS)


# Multi-resolution .ico for the exe/taskbar/window.
ico_sizes = [16, 24, 32, 48, 64, 128, 256]
ico_frames = [
    draw_mic_at(s, (240, 240, 245, 255), (24, 24, 28, 255))
    for s in ico_sizes
]
ico_frames[0].save(
    os.path.join(OUT_DIR, "app.ico"),
    sizes=[(s, s) for s in ico_sizes],
    append_images=ico_frames[1:],
)

# Also save a 256x256 PNG for loading as a window icon at runtime (egui takes RGBA).
draw_mic_at(256, (240, 240, 245, 255), (24, 24, 28, 255)).save(
    os.path.join(OUT_DIR, "app_icon.png")
)

print("wrote tray_idle.png, tray_active.png, app.ico, app_icon.png to", OUT_DIR)
