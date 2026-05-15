#!/usr/bin/env python3

import struct
import sys


def read_u16le(data, offset):
    return struct.unpack_from("<H", data, offset)[0]


def read_u32le(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def read_s32le(data, offset):
    return struct.unpack_from("<i", data, offset)[0]


def convert_bmp24_to_bgrx8888(src_path, dst_path):
    with open(src_path, "rb") as f:
        header = f.read(54)
        if len(header) < 54 or header[0:2] != b"BM":
            raise RuntimeError("Unsupported BMP header")

        pixel_offset = read_u32le(header, 10)
        width = read_s32le(header, 18)
        height = read_s32le(header, 22)
        bpp = read_u16le(header, 28)
        compression = read_u32le(header, 30)

        if width <= 0 or height == 0:
            raise RuntimeError("Unsupported BMP dimensions")
        if bpp != 24 or compression != 0:
            raise RuntimeError("Only uncompressed 24bpp BMP is supported")

        row_count = abs(height)
        row_stride = ((width * 3 + 3) // 4) * 4
        f.seek(pixel_offset)
        rows = [f.read(row_stride)[: width * 3] for _ in range(row_count)]
        if height > 0:
            rows.reverse()

    with open(dst_path, "wb") as out:
        for row in rows:
            for i in range(0, len(row), 3):
                b = row[i]
                g = row[i + 1]
                r = row[i + 2]
                out.write(bytes([b, g, r, 0]))


def main():
    if len(sys.argv) != 3:
        print("Usage: bmp_to_fb.py <src.bmp> <dst.fb>", file=sys.stderr)
        return 1
    convert_bmp24_to_bgrx8888(sys.argv[1], sys.argv[2])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
