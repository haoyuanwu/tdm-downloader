"""Generate Tauri application icons (pure stdlib: zlib + struct). No Pillow needed."""
import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icons")
os.makedirs(OUT_DIR, exist_ok=True)


def lerp(a, b, t):
    return int(a + (b - a) * t)


def make_pixels(size):
    """Build RGBA pixel matrix: rounded-ish gradient square with a white download arrow."""
    px = [[(0, 0, 0, 0) for _ in range(size)] for _ in range(size)]

    # gradient background: top-left #4F8CFF -> bottom-right #1B3B8F
    c0 = (0x4F, 0x8C, 0xFF)
    c1 = (0x1B, 0x3B, 0x8F)
    for y in range(size):
        for x in range(size):
            t = (x / max(size - 1, 1) * 0.5) + (y / max(size - 1, 1) * 0.5)
            px[y][x] = (lerp(c0[0], c1[0], t), lerp(c0[1], c1[1], t),
                        lerp(c0[2], c1[2], t), 255)

    # rounded corners: mask out pixels outside radius
    r = max(1, size // 8)
    for y in range(size):
        for x in range(size):
            cx = min(x, size - 1 - x)
            cy = min(y, size - 1 - y)
            if cx < r and cy < r:
                dx, dy = r - cx, r - cy
                if dx * dx + dy * dy > r * r:
                    px[y][x] = (0, 0, 0, 0)

    # white download arrow: vertical shaft + downward head + baseline
    cx = size / 2.0
    shaft_w = max(1.0, size * 0.10)
    shaft_top = size * 0.22
    shaft_bottom = size * 0.60
    head_top = size * 0.48
    head_bottom = size * 0.70
    base_y = size * 0.76
    base_h = max(1.0, size * 0.07)

    for y in range(size):
        for x in range(size):
            fx, fy = x + 0.5, y + 0.5

            # shaft
            if shaft_top <= fy <= shaft_bottom and abs(fx - cx) <= shaft_w / 2:
                px[y][x] = (255, 255, 255, 255)
                continue

            # arrow head (triangle)
            if head_top <= fy <= head_bottom:
                t = (fy - head_top) / max(head_bottom - head_top, 1e-6)
                half = shaft_w / 2 + t * (size * 0.22)
                if abs(fx - cx) <= half:
                    px[y][x] = (255, 255, 255, 255)
                    continue

            # baseline
            if base_y <= fy <= base_y + base_h and abs(fx - cx) <= size * 0.24:
                px[y][x] = (255, 255, 255, 255)
                continue

    return px


def write_png(path, size):
    px = make_pixels(size)
    raw = bytearray()
    for y in range(size):
        raw.append(0)  # filter type: None
        for x in range(size):
            raw.extend(px[y][x])

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        c += struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        return c

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", ihdr)
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)
    return png


def write_ico(path, png_bytes, size):
    # ICO can embed PNG payload directly (supported since Vista)
    header = struct.pack("<HHH", 0, 1, 1)
    entry = struct.pack("<BBBBHHII",
                        size if size < 256 else 0,
                        size if size < 256 else 0,
                        0, 0, 1, 32,
                        len(png_bytes), 6 + 16)
    with open(path, "wb") as f:
        f.write(header + entry + png_bytes)


def main():
    targets = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
        "Square30x30Logo.png": 30,
        "Square44x44Logo.png": 44,
        "Square71x71Logo.png": 71,
        "Square89x89Logo.png": 89,
        "Square107x107Logo.png": 107,
        "Square142x142Logo.png": 142,
        "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284,
        "Square310x310Logo.png": 310,
        "StoreLogo.png": 50,
    }
    for name, size in targets.items():
        write_png(os.path.join(OUT_DIR, name), size)

    ico_png = write_png(os.path.join(OUT_DIR, "_ico_tmp.png"), 256)
    write_ico(os.path.join(OUT_DIR, "icon.ico"), ico_png, 256)
    os.remove(os.path.join(OUT_DIR, "_ico_tmp.png"))

    print("icons generated in:", OUT_DIR)
    for f in sorted(os.listdir(OUT_DIR)):
        p = os.path.join(OUT_DIR, f)
        print(f"  {f:26s} {os.path.getsize(p):>8d} bytes")


if __name__ == "__main__":
    main()
