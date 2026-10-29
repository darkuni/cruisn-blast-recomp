#!/usr/bin/env python3
"""Pull every compiled Switch (NVN / Maxwell) shader program out of the game's Unity data.

The runtime translates and compiles all of them at startup, so nothing has to compile while
the game is running. The output is derived from the game files: it stays next to the build,
never in the repo.

    python3 tools/extract_shaders.py extracted_v2/romfs/Data build/cruisn-blast.shaders

Needs: pip install UnityPy lz4

Format (little endian): b"KSH1", u32 count, then per program u32 length + bytes. Each program
starts at its Maxwell shader program header (SPH) and runs to the end of its Unity subprogram;
the translator stops at the program's own end.
"""

import glob
import hashlib
import os
import struct
import sys
import time

import lz4.block
import UnityPy

SWITCH = 19                     # Unity ShaderCompilerPlatform for the Switch
SUBPROGRAM_VERSION = 201806140  # Unity 2019.4 serialized subprogram
NVN_MAGIC = b"\x78\x56\x34\x12"  # starts the 0x30-byte header in front of each program
HEADER = 0x30
MAX_PROGRAM = 0x20000


def unity_files(data):
    names = sorted(os.listdir(data))
    out = [n for n in names if (n.startswith("level") or n.endswith(".assets") or n.startswith("globalgamemanagers")) and not n.endswith(".resS")]
    out += sorted(os.path.relpath(p, data) for p in glob.glob(os.path.join(data, "StreamingAssets", "aa", "*", "*.bundle")))
    out += sorted(os.path.relpath(p, data) for p in glob.glob(os.path.join(data, "Resources", "*")) if not p.endswith(".resS"))
    return out


def find_blob(raw):
    """Locate platforms [19], offsets, compressed and decompressed lengths and the blob in a
    raw Shader object (UnityPy's typetree turns the blob into a huge int list)."""
    pos = 0
    while True:
        i = raw.find(struct.pack("<iii", 1, SWITCH, 1), pos)
        if i < 0:
            return None
        try:
            p = i + 12
            k = struct.unpack_from("<i", raw, p)[0]
            p += 4
            if not 0 < k < 4096:
                raise ValueError
            offs = struct.unpack_from("<%dI" % k, raw, p)
            p += 4 * k
            if struct.unpack_from("<ii", raw, p) != (1, k):
                raise ValueError
            p += 8
            comp = struct.unpack_from("<%dI" % k, raw, p)
            p += 4 * k
            if struct.unpack_from("<ii", raw, p) != (1, k):
                raise ValueError
            p += 8
            dec = struct.unpack_from("<%dI" % k, raw, p)
            p += 4 * k
            n = struct.unpack_from("<i", raw, p)[0]
            p += 4
            if n != max(o + c for o, c in zip(offs, comp)) or p + n > len(raw):
                raise ValueError
            return list(zip(offs, comp, dec)), raw[p:p + n]
        except (ValueError, struct.error):
            pos = i + 4


def programs_in(blob, segments):
    decs = [lz4.block.decompress(blob[o:o + c], uncompressed_size=d) for o, c, d in segments]
    count = struct.unpack_from("<i", decs[0], 0)[0]
    for i in range(count):
        off, size, seg = struct.unpack_from("<iii", decs[0], 4 + 12 * i)
        if not 0 <= seg < len(decs):
            continue
        sub = decs[seg][off:off + size]
        if len(sub) < 0x30 or struct.unpack_from("<I", sub, 0)[0] != SUBPROGRAM_VERSION:
            continue
        m = sub.find(NVN_MAGIC)
        while m >= 0:
            nxt = sub.find(NVN_MAGIC, m + 4)
            yield sub[m + HEADER:(nxt if nxt > 0 else len(sub))][:MAX_PROGRAM]
            m = nxt


def extract(sources, out, progress=None):
    """sources: (name, path or bytes) pairs of Unity files. Writes `out`, returns
    (shader objects, programs)."""
    seen_blobs, programs, order = set(), {}, []
    shaders = 0
    for name, data in sources:
        if progress:
            progress(name)
        env = UnityPy.load(data)
        for obj in env.objects:
            if obj.type.name != "Shader":
                continue
            found = find_blob(obj.get_raw_data())
            if not found:
                continue
            segments, blob = found
            key = hashlib.sha1(blob).digest()
            if key in seen_blobs:
                continue
            seen_blobs.add(key)
            shaders += 1
            for code in programs_in(blob, segments):
                h = hashlib.sha1(code).digest()
                if h not in programs:
                    programs[h] = code
                    order.append(h)
    with open(out + ".tmp", "wb") as f:
        f.write(b"KSH1" + struct.pack("<I", len(order)))
        for h in order:
            f.write(struct.pack("<I", len(programs[h])) + programs[h])
    os.replace(out + ".tmp", out)
    return shaders, len(order)


def is_unity_file(rel):
    """`rel`: path under Data/ with forward slashes (same selection as unity_files)."""
    parts = rel.split("/")
    if rel.endswith(".resS"):
        return False
    if len(parts) == 1:
        n = parts[0]
        return n.startswith("level") or n.endswith(".assets") or n.startswith("globalgamemanagers")
    if len(parts) == 4 and parts[:2] == ["StreamingAssets", "aa"]:
        return parts[3].endswith(".bundle")
    return len(parts) == 2 and parts[0] == "Resources"


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    data, out = sys.argv[1], sys.argv[2]
    start = time.time()
    shaders, count = extract(((n, os.path.join(data, n)) for n in unity_files(data)), out)
    print(f"{shaders} shaders, {count} programs -> {out} ({os.path.getsize(out) >> 20} MB, {time.time() - start:.0f} s)")


if __name__ == "__main__":
    main()
