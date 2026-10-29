#!/usr/bin/env python3
"""Set up the game files for the native PC build of Cruis'n Blast from your own dump.

Put these next to cruisn-blast.exe (or in a subfolder) and run setup.exe, or drag them onto
setup.exe:
  - the base game NSP      (title 0100B41013C82000)
  - the v2.0.0 update NSP  (title 0100B41013C82800, version 131072)
  - your prod.keys         (also found in %USERPROFILE%\\.switch\\)

It writes, next to the exe:
  game\\exefs\\...            the game's program files (checked against v2.0.0)
  game\\romfs.bin           the game's data with the update applied (about 3.6 GB)
  cruisn-blast.shaders     the game's shader programs, compiled once on the first launch

Nothing is downloaded and nothing leaves your PC. The NSPs are only read.

    setup.exe [NSP, prod.keys or folder ...]    (or: python tools/setup_game.py ...)
The files are written next to setup.exe (or into the current folder when run as a script).
"""

import hashlib
import os
import sys
import time

import nsp_extract as nx

BASE_TITLE = 0x0100B41013C82000
# SHA-256 of the v2.0.0 program files the PC build was recompiled from.
EXPECTED = {
    "main": "e1d8295bb659f7b88bcbc33f9cbf52926db8d7112daa97ad6458ddb49acb9829",
    "main.npdm": "f56ae1f58a4bbc3976b6eba8608eb4fdca628e0ef0c7cc845fbd7121ff432e18",
    "rtld": "e1b018d97186c07e69d1203326183dddcd575af8fe192433b848c30ca650eb82",
    "sdk": "c54bf6080c82b1946574714e7bac2a1d2b15cc7e1198785bde0b95b393ead722",
    "subsdk0": "efc5c92a4b55ff050d040f1236f02663ccfdfab14a80e59f312c38e23449860f",
}
CHUNK = 16 << 20


class Stop(Exception):
    pass


def say(msg=""):
    print(msg, flush=True)


def here():
    if getattr(sys, "frozen", False):
        return os.path.dirname(os.path.abspath(sys.executable))
    return os.getcwd()


def candidates(folder, ext):
    """Files with this extension in the folder and one level of subfolders."""
    out = []
    for entry in sorted(os.listdir(folder)):
        p = os.path.join(folder, entry)
        if os.path.isfile(p) and entry.lower().endswith(ext):
            out.append(p)
        elif os.path.isdir(p) and entry.lower() not in ("game", "build", "kirikae"):
            try:
                out += [os.path.join(p, n) for n in sorted(os.listdir(p)) if n.lower().endswith(ext)]
            except OSError:
                pass
    return out


def search(places, ext):
    out = []
    for p in places:
        if os.path.isdir(p):
            out += candidates(p, ext)
        elif os.path.isfile(p) and p.lower().endswith(ext):
            out.append(p)
    return out


def find_keys(places):
    found = search(places, "prod.keys")
    home = os.path.join(os.path.expanduser("~"), ".switch", "prod.keys")
    if os.path.isfile(home):
        found.append(home)
    if not found:
        raise Stop("prod.keys not found. Put the prod.keys dumped from your Switch next to setup.exe.")
    return found[0]


def program_nca(path, keys):
    f = open(path, "rb")

    def raw(off, n):
        f.seek(off)
        return f.read(n)

    ents = nx.read_pfs0(raw)
    tks = nx.ticket_titlekeys(f, ents, keys)
    for name, (off, _) in ents.items():
        if name.endswith(".nca"):
            nca = nx.Nca(f, off, keys, tks)
            if nca.content_type == 0:
                return nca
    return None


def find_games(places, keys):
    base = update = None
    for path in search(places, ".nsp"):
        try:
            nca = program_nca(path, keys)
        except Exception as e:  # wrong keys, not an NSP, damaged file
            say(f"  skipping {os.path.basename(path)}: {e}")
            continue
        if nca is None:
            continue
        if nca.title_id & ~0xFFF != BASE_TITLE:
            continue
        # The update's program carries the game's own title ID; its RomFS is a patch (BKTR).
        romfs = nca.section(1)
        if romfs is not None and romfs.enc_type == 4:
            update = update or (path, nca)
        else:
            base = base or (path, nca)
    if base is None:
        raise Stop("The Cruis'n Blast base game NSP (0100B41013C82000) was not found next to setup.exe.")
    if update is None:
        raise Stop("The Cruis'n Blast v2.0.0 update NSP (0100B41013C82800) was not found next to setup.exe.\n"
                   "The PC build is made from version 2.0.0, so the update is required.")
    return base, update


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            b = f.read(CHUNK)
            if not b:
                return h.hexdigest()
            h.update(b)


def write_exefs(update_nca, game):
    sec = update_nca.section(0)
    po, _ = sec.data_region()
    files = nx.read_pfs0(sec.read, po)
    exefs = os.path.join(game, "exefs")
    for name, (off, size) in files.items():
        sec.copy_out(off, size, os.path.join(exefs, name))
    for name, want in EXPECTED.items():
        p = os.path.join(exefs, name)
        if not os.path.isfile(p):
            raise Stop(f"The update has no {name}: this is not the v2.0.0 update.")
        if sha256(p) != want:
            raise Stop(f"{name} does not match v2.0.0. Use the v2.0.0 update (version 131072).")
    say(f"  program files OK ({len(files)} files, version 2.0.0)")


class Plain:
    """A RomFS image file read like a decrypted section."""

    def __init__(self, path):
        self.f = open(path, "rb")

    def read(self, off, n):
        self.f.seek(off)
        return self.f.read(n)


def write_romfs(update_nca, base_nca, game):
    sec = update_nca.section(1)
    if sec is None:
        raise Stop("The update has no RomFS.")
    if sec.enc_type == 4:
        sec = nx.PatchedSection(sec, base_nca.section(1))
    ro, size = sec.data_region()
    out = os.path.join(game, "romfs.bin")
    if os.path.isfile(out) and os.path.getsize(out) == size and romfs_ok(out):
        say("  game data already there")
        return out
    tmp = out + ".part"
    # An interrupted setup carries on where it stopped.
    done = os.path.getsize(tmp) // CHUNK * CHUNK if os.path.isfile(tmp) else 0
    if done:
        say(f"  carrying on from {done / 1e9:.2f} GB")
    start, last, first = time.time(), 0.0, done
    with open(tmp, "r+b" if done else "wb") as o:
        o.seek(done)
        o.truncate()
        while done < size:
            n = min(CHUNK, size - done)
            o.write(sec.read(ro + done, n))
            done += n
            if time.time() - last > 1 or done == size:
                last = time.time()
                rate = (done - first) / max(last - start, 0.001) / 1e6
                print(f"\r  game data: {done / 1e9:5.2f} of {size / 1e9:.2f} GB ({rate:.0f} MB/s)   ", end="", flush=True)
    say()
    os.replace(tmp, out)
    if not romfs_ok(out):
        raise Stop("The game data came out damaged. Check the NSPs (re-dump them if needed).")
    return out


def romfs_ok(path):
    try:
        files = nx.romfs_files(Plain(path), 0)
    except Exception:
        return False
    names = {p for p, _, _ in files}
    return "/Data/Managed/Metadata/global-metadata.dat" in names and len(files) > 100


def write_shaders(romfs, out):
    import extract_shaders as xs
    img = Plain(romfs)
    files = sorted((p[len("/Data/"):], off, size) for p, off, size in nx.romfs_files(img, 0) if p.startswith("/Data/"))
    picked = [(rel, off, size) for rel, off, size in files if xs.is_unity_file(rel)]
    total = len(picked)
    count = [0]

    def sources():
        for rel, off, size in picked:
            yield rel, img.read(off, size)

    def progress(name):
        count[0] += 1
        print(f"\r  shaders: file {count[0]} of {total}   ", end="", flush=True)

    shaders, programs = xs.extract(sources(), out, progress)
    say()
    say(f"  {programs} shader programs from {shaders} shaders")


def run(folder, places):
    say("Cruis'n Blast PC - game file setup")
    say(f"Setting up {folder}")
    keys_path = find_keys(places)
    say(f"  keys: {keys_path}")
    keys = nx.load_keys(keys_path)
    (base_path, base_nca), (update_path, update_nca) = find_games(places, keys)
    say(f"  base game: {os.path.basename(base_path)}")
    say(f"  update:    {os.path.basename(update_path)}")
    game = os.path.join(folder, "game")
    os.makedirs(game, exist_ok=True)
    t0 = time.time()
    say("Program files")
    write_exefs(update_nca, game)
    say("Game data (update applied)")
    romfs = write_romfs(update_nca, base_nca, game)
    say("Shader programs")
    shaders = os.path.join(folder, "cruisn-blast.shaders")
    write_shaders(romfs, shaders)
    say(f"\nDone in {time.time() - t0:.0f} s. Start cruisn-blast.exe.")
    say("The NSPs and prod.keys are no longer needed by the game; keep them somewhere safe.")


def main():
    folder = here()
    places = [os.path.abspath(a) for a in sys.argv[1:]] + [folder]
    try:
        run(folder, places)
        code = 0
    except Stop as e:
        say(f"\nSTOP: {e}")
        code = 1
    except KeyboardInterrupt:
        say("\nStopped.")
        code = 1
    except Exception as e:
        import traceback
        traceback.print_exc()
        say(f"\nSetup failed: {e}")
        code = 1
    if getattr(sys, "frozen", False):
        input("\nPress Enter to close.")
    sys.exit(code)


if __name__ == "__main__":
    main()
