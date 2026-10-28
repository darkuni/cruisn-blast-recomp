#!/usr/bin/env python3
"""Extract ExeFS and RomFS from a Cruis'n Blast (Switch) NSP using your own console keys.

Handles standard-crypto NSPs (no rights ID), which is what the v0 dump is.
Only dependency: the `cryptography` package.

Usage:
  python3 tools/nsp_extract.py GAME.nsp --keys prod.keys --out extracted
  python3 tools/nsp_extract.py GAME.nsp --keys prod.keys --out extracted --romfs-filter global-metadata
  python3 tools/nsp_extract.py GAME.nsp --keys prod.keys --info
"""
import argparse
import os
import struct
import sys

from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

CHUNK = 16 * 1024 * 1024


def load_keys(path):
    keys = {}
    with open(path) as f:
        for line in f:
            if '=' in line:
                k, v = (x.strip() for x in line.split('=', 1))
                try:
                    keys[k] = bytes.fromhex(v)
                except ValueError:
                    pass
    return keys


def read_pfs0(read, base=0):
    """Return {name: (absolute_offset, size)} for a PFS0 starting at `base`."""
    magic, n, strsz, _ = struct.unpack('<4sIII', read(base, 16))
    if magic != b'PFS0':
        raise ValueError(f'not a PFS0 at {base:#x}: {magic!r}')
    ents = [struct.unpack('<QQII', read(base + 16 + 24 * i, 24)) for i in range(n)]
    strs = read(base + 16 + 24 * n, strsz)
    data = base + 16 + 24 * n + strsz
    out = {}
    for off, size, stroff, _ in ents:
        out[strs[stroff:strs.index(b'\0', stroff)].decode()] = (data + off, size)
    return out


def xts_decrypt(data, key, sector0=0):
    """Nintendo AES-XTS: 0x200 sectors, big-endian sector-number tweak."""
    out = bytearray()
    for i in range(0, len(data), 0x200):
        tweak = (sector0 + i // 0x200).to_bytes(16, 'big')
        d = Cipher(algorithms.AES(key), modes.XTS(tweak)).decryptor()
        out += d.update(data[i:i + 0x200]) + d.finalize()
    return bytes(out)


def ticket_titlekeys(f, ents, keys):
    """Common tickets (key type 0) in the NSP -> {rights_id: decrypted title key}."""
    out = {}
    for name, (off, size) in ents.items():
        if not name.endswith('.tik'):
            continue
        f.seek(off)
        t = f.read(size)
        if t[0x281] != 0:
            print(f'{name}: personalized ticket, skipped')
            continue
        rights = t[0x2a0:0x2b0]
        mkrev = rights[15]
        kek = keys[f'titlekek_{(mkrev - 1 if mkrev else 0):02x}']
        d = Cipher(algorithms.AES(kek), modes.ECB()).decryptor()
        out[rights] = d.update(t[0x180:0x190]) + d.finalize()
    return out


class Nca:
    def __init__(self, f, offset, keys, titlekeys=None):
        self.f, self.offset = f, offset
        f.seek(offset)
        self.h = xts_decrypt(f.read(0xC00), keys['header_key'])
        if self.h[0x200:0x204] != b'NCA3':
            raise ValueError('bad NCA header (wrong header_key?)')
        self.content_type = self.h[0x205]
        self.keygen = max(self.h[0x206], self.h[0x220])
        self.title_id = struct.unpack('<Q', self.h[0x210:0x218])[0]
        self.rights_id = self.h[0x230:0x240]
        idx = self.keygen - 1 if self.keygen > 0 else 0
        if any(self.rights_id):
            tk = (titlekeys or {}).get(self.rights_id)
            if tk is None:
                raise KeyError(f'no ticket for rights ID {self.rights_id.hex()}')
            self.ctr_key = tk
            return
        kak = keys[f'key_area_key_application_{idx:02x}']
        d = Cipher(algorithms.AES(kak), modes.ECB()).decryptor()
        key_area = d.update(self.h[0x300:0x340]) + d.finalize()
        self.ctr_key = key_area[0x20:0x30]

    def section(self, i):
        start, end, _, _ = struct.unpack('<IIII', self.h[0x240 + i * 16:0x250 + i * 16])
        if end == 0:
            return None
        return Section(self, i, start * 0x200, (end - start) * 0x200,
                       self.h[0x400 + i * 0x200:0x600 + i * 0x200])


class Section:
    # FS header: 0x2 fs type, 0x3 hash type, 0x4 encryption type, 0x140 ctr upper
    def __init__(self, nca, idx, base, size, fsh):
        self.nca, self.idx, self.base, self.size, self.fsh = nca, idx, base, size, fsh
        self.hash_type = fsh[3]
        self.enc_type = fsh[4]
        self.ctr_upper = fsh[0x140:0x148][::-1]

    def read(self, off, n, generation=None):
        a = off & ~0xF
        pad = off - a
        ln = (n + pad + 0xF) & ~0xF
        self.nca.f.seek(self.nca.offset + self.base + a)
        data = self.nca.f.read(ln)
        if self.enc_type == 1:  # none
            return data[pad:pad + n]
        upper = self.ctr_upper
        if generation is not None:  # AesCtrEx: generation replaces the low 4 bytes
            upper = upper[:4] + generation.to_bytes(4, 'big')
        ctr = upper + ((self.base + a) >> 4).to_bytes(8, 'big')
        d = Cipher(algorithms.AES(self.nca.ctr_key), modes.CTR(ctr)).decryptor()
        return (d.update(data) + d.finalize())[pad:pad + n]

    def read_plain_ctr(self, off, n):
        return self.read(off, n)

    def copy_out(self, off, size, dst):
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        with open(dst, 'wb') as o:
            done = 0
            while done < size:
                n = min(CHUNK, size - done)
                o.write(self.read(off + done, n))
                done += n

    def data_region(self):
        """Offset of the payload past the hash tree."""
        if self.hash_type == 2:   # HierarchicalSha256 -> PFS0 (ExeFS)
            return struct.unpack('<QQ', self.fsh[0x40:0x50])
        if self.hash_type == 3:   # HierarchicalIntegrity (IVFC) -> RomFS
            iv = self.fsh[0x8:]
            if iv[:4] != b'IVFC':
                raise ValueError('expected IVFC')
            off, size = struct.unpack('<QQ', iv[0x10 + 5 * 0x18:0x10 + 5 * 0x18 + 16])
            return off, size
        raise ValueError(f'unknown hash type {self.hash_type}')


def bucket_entries(read, off, entry_size, parse):
    """Read a BucketTree (0x4000-byte nodes) at `off` -> (flat entry list, end offset)."""
    _, set_count, end = struct.unpack('<IIQ', read(off, 0x10))
    out = []
    for i in range(set_count):
        node = read(off + 0x4000 * (i + 1), 0x4000)
        _, cnt, _ = struct.unpack('<IIQ', node[:0x10])
        for j in range(cnt):
            out.append(parse(node[0x10 + j * entry_size:0x10 + (j + 1) * entry_size]))
    return out, end


class PatchedSection:
    """Virtual view of an update's RomFS section: BKTR indirect table over base + patch data."""

    def __init__(self, patch_sec, base_sec):
        import bisect
        self._bisect = bisect
        self.patch, self.base, self.fsh = patch_sec, base_sec, patch_sec.fsh
        self.hash_type = patch_sec.hash_type
        plain = patch_sec.read_plain_ctr
        ind_off, ind_size = struct.unpack('<QQ', self.fsh[0x100:0x110])
        ctr_off, ctr_size = struct.unpack('<QQ', self.fsh[0x120:0x130])
        if self.fsh[0x110:0x114] != b'BKTR' or self.fsh[0x130:0x134] != b'BKTR':
            raise ValueError('patch section has no BKTR tables')
        # (virtual offset, physical offset, storage index 0=base 1=patch)
        self.ind, self.ind_end = bucket_entries(plain, ind_off, 0x14,
                                                lambda b: struct.unpack('<QQi', b))
        self.ind_keys = [e[0] for e in self.ind]
        # (patch offset, generation)
        self.ctr, self.ctr_end = bucket_entries(
            plain, ctr_off, 0x10,
            lambda b: (struct.unpack('<Q', b[:8])[0], struct.unpack('<I', b[12:16])[0]))
        self.ctr_keys = [e[0] for e in self.ctr]

    def _patch_read(self, off, n):
        out = bytearray()
        while n > 0:
            i = self._bisect.bisect_right(self.ctr_keys, off) - 1
            start, gen = self.ctr[i]
            end = self.ctr[i + 1][0] if i + 1 < len(self.ctr) else self.ctr_end
            take = min(n, end - off)
            if take <= 0:
                raise ValueError(f'patch read past AesCtrEx table at {off:#x}')
            out += self.patch.read(off, take, generation=gen)
            off += take
            n -= take
        return bytes(out)

    def read(self, off, n):
        out = bytearray()
        while n > 0:
            i = self._bisect.bisect_right(self.ind_keys, off) - 1
            virt, phys, storage = self.ind[i]
            end = self.ind[i + 1][0] if i + 1 < len(self.ind) else self.ind_end
            take = min(n, end - off)
            if take <= 0:
                raise ValueError(f'virtual read past indirect table at {off:#x}')
            p = phys + (off - virt)
            out += self.base.read(p, take) if storage == 0 else self._patch_read(p, take)
            off += take
            n -= take
        return bytes(out)

    copy_out = Section.copy_out
    data_region = Section.data_region


def romfs_files(sec, ro):
    hdr = struct.unpack('<10Q', sec.read(ro, 0x50))
    dmo, dms, fmo, fms, data = hdr[3], hdr[4], hdr[7], hdr[8], hdr[9]
    dm, fm = sec.read(ro + dmo, dms), sec.read(ro + fmo, fms)
    files = []

    def walk(doff, path):
        _, _, child_dir, child_file, _, nl = struct.unpack('<6I', dm[doff:doff + 24])
        name = dm[doff + 24:doff + 24 + nl].decode()
        path = f'{path}/{name}' if doff else ''
        x = child_file
        while x != 0xFFFFFFFF:
            _, sib, fo, fz, _, fl = struct.unpack('<IIQQII', fm[x:x + 32])
            files.append((f'{path}/{fm[x + 32:x + 32 + fl].decode()}', ro + data + fo, fz))
            x = sib
        x = child_dir
        while x != 0xFFFFFFFF:
            walk(x, path)
            x = struct.unpack('<I', dm[x + 4:x + 8])[0]

    walk(0, '')
    return files


def find_program(path, keys):
    f = open(path, 'rb')

    def raw(off, n):
        f.seek(off)
        return f.read(n)

    ents = read_pfs0(raw)
    tks = ticket_titlekeys(f, ents, keys)
    for name, (off, _) in ents.items():
        if name.endswith('.nca'):
            nca = Nca(f, off, keys, tks)
            if nca.content_type == 0:
                return nca
    raise ValueError(f'no Program NCA in {path}')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('nsp')
    ap.add_argument('--keys', required=True, help='your prod.keys')
    ap.add_argument('--base', help='base game NSP (needed for an update\'s RomFS)')
    ap.add_argument('--out', default='extracted')
    ap.add_argument('--info', action='store_true', help='print layout only')
    ap.add_argument('--no-exefs', action='store_true')
    ap.add_argument('--no-romfs', action='store_true')
    ap.add_argument('--romfs-filter', action='append', default=[],
                    help='only extract RomFS paths containing this text (repeatable)')
    a = ap.parse_args()

    keys = load_keys(a.keys)
    f = open(a.nsp, 'rb')

    def raw(off, n):
        f.seek(off)
        return f.read(n)

    ents = read_pfs0(raw)
    titlekeys = ticket_titlekeys(f, ents, keys)
    program = None
    for name, (off, size) in ents.items():
        if not name.endswith('.nca'):
            print(f'{name:48} {size:>14,}')
            continue
        nca = Nca(f, off, keys, titlekeys)
        print(f'{name:48} {size:>14,}  type={nca.content_type} keygen={nca.keygen} '
              f'tid={nca.title_id:016x}')
        for i in range(4):
            s = nca.section(i)
            if s:
                print(f'    section {i}: off={s.base:#x} size={s.size:#x} '
                      f'hash={s.hash_type} enc={s.enc_type}')
        if nca.content_type == 0:
            program = nca
    if a.info:
        return
    if program is None:
        sys.exit('no Program NCA found')

    if not a.no_exefs:
        sec = program.section(0)
        po, _ = sec.data_region()
        for name, (off, size) in read_pfs0(sec.read, po).items():
            dst = os.path.join(a.out, 'exefs', name)
            sec.copy_out(off, size, dst)
            print(f'exefs/{name} {size:,}')

    if not a.no_romfs:
        sec = program.section(1)
        if sec is not None and sec.enc_type == 4:
            if not a.base:
                print('romfs: this is an update (patch RomFS); pass --base BASE.nsp to extract it')
                return
            sec = PatchedSection(sec, find_program(a.base, keys).section(1))
        ro, _ = sec.data_region()
        files = romfs_files(sec, ro)
        os.makedirs(a.out, exist_ok=True)
        with open(os.path.join(a.out, 'romfs_listing.txt'), 'w') as lst:
            lst.write('\n'.join(f'{z:>14} {p}' for p, _, z in files) + '\n')
        print(f'romfs: {len(files)} files, {sum(z for _, _, z in files) / 1e9:.2f} GB')
        for path, off, size in files:
            if a.romfs_filter and not any(t in path for t in a.romfs_filter):
                continue
            sec.copy_out(off, size, os.path.join(a.out, 'romfs', path.lstrip('/')))
            print(f'romfs{path} {size:,}')


if __name__ == '__main__':
    main()
