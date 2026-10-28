#!/usr/bin/env python3
"""Turn an Il2CppDumper script.json into a Kirikae profile so every IL2CPP method
gets compiled natively on the first build instead of being found one run at a time.

Kirikae lays modules out from 0x08000000 in the order rtld, main, subsdk*, sdk,
each aligned to 0x1000. For Cruis'n Blast 2.0.0, rtld is 0x4000 bytes, so main
loads at 0x08004000.

Usage:
  python3 tools/il2cpp_profile.py il2cpp_dump/script.json -o cruisn-blast.profile
"""
import argparse
import json

MAIN_BASE = 0x08004000


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('script_json')
    ap.add_argument('-o', '--out', required=True)
    ap.add_argument('--base', type=lambda s: int(s, 0), default=MAIN_BASE,
                    help='guest address of main (default 0x08004000)')
    a = ap.parse_args()

    with open(a.script_json, encoding='utf-8') as f:
        methods = json.load(f)['ScriptMethod']
    addrs = sorted({a.base + m['Address'] for m in methods if m['Address']})
    with open(a.out, 'w') as o:
        o.write('# kirikae: IL2CPP method entries from Il2CppDumper script.json\n')
        for ad in addrs:
            o.write(f'{ad:x} 1\n')
    print(f'{len(addrs)} unique method entries -> {a.out}')


if __name__ == '__main__':
    main()
