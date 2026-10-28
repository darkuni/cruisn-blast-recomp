# cruisn-blast-recomp

Static recompilation of **Cruis'n Blast** (Nintendo Switch, Raw Thrills 2021) to a native Windows x64 build.
Delisted from the Switch eShop on June 30, 2026.

Tools and runtime only. Bring your own dump and your own console keys. Nothing from the game is stored in this repo.

## Target

| | |
|---|---|
| Title ID | `0100B41013C82000` (base), `0100B41013C82800` (update) |
| Target version | v131072 (2.0.0) |
| Engine | Unity 2019.4.18f1, IL2CPP scripting backend |
| IL2CPP metadata | v24 (`Data/Managed/Metadata/global-metadata.dat`, 8,527,172 bytes in 2.0.0) |
| Graphics | NVN |
| Asset loading | Addressables (`Data/StreamingAssets/aa/`) |

## ExeFS (v2.0.0)

| Module | Notes |
|---|---|
| `main` | Unity player + all IL2CPP game code. text 0x2CF3328, rodata 0x1841C64, data 0x5025B0 (decompressed) |
| `sdk` | Nintendo SDK. Identical between v0 and v2.0.0 |
| `subsdk0` | Identical between v0 and v2.0.0 |
| `rtld` | Dynamic loader |

## Tools

`tools/nsp_extract.py` - decrypts an NSP with your `prod.keys` and pulls ExeFS and RomFS.
Handles standard crypto, common tickets, and update patch RomFS (BKTR indirect + AES-CTR-EX, layered over the base RomFS). Only needs the `cryptography` package.

```
python3 tools/nsp_extract.py "base.nsp" --keys prod.keys --out extracted
python3 tools/nsp_extract.py "update.nsp" --keys prod.keys --base "base.nsp" --out extracted_v2 --romfs-filter global-metadata
python3 tools/nsp_extract.py "base.nsp" --keys prod.keys --info
```

## Plan

1. Extract base + update (ExeFS, RomFS incl. patch RomFS)
2. IL2CPP dump from `global-metadata.dat` + `main` for full class/method symbols
3. Identify Unity engine and Nintendo SDK functions
4. Static recompile AArch64 -> native x64
5. Runtime: Horizon OS services, NVN -> Vulkan, audio, input, filesystem
