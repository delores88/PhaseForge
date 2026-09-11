"""Convert approved raster exports to Windows ICO containers without redrawing them."""
import io
import pathlib
import struct
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
PUBLIC = ROOT / 'frontend' / 'public'


def write_ico(target, entries):
    images = []
    for size, source in entries:
        with Image.open(source) as original:
            rgba = original.convert('RGBA')
            if rgba.size != (size, size):
                rgba = rgba.resize((size, size), Image.Resampling.LANCZOS)
            stream = io.BytesIO()
            rgba.save(stream, format='PNG')
            images.append((size, stream.getvalue()))
    offset = 6 + 16 * len(images)
    headers = []
    for size, data in images:
        headers.append(struct.pack('<BBBBHHII', size if size < 256 else 0,
                                   size if size < 256 else 0, 0, 0, 1, 32, len(data), offset))
        offset += len(data)
    target.write_bytes(struct.pack('<HHH', 0, 1, len(images)) + b''.join(headers)
                       + b''.join(data for _, data in images))


if __name__ == '__main__':
    # Keep the owner's shaded identity at every size, including browser/tray icons.
    write_ico(PUBLIC / 'icon.ico', [(size, PUBLIC / 'icon.png') for size in (16, 32, 48, 64, 128, 256)])
    write_ico(PUBLIC / 'favicon.ico', [(size, PUBLIC / 'icon.png') for size in (16, 32, 64)])
