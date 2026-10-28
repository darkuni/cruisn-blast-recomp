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

## IL2CPP dump (v2.0.0)

Il2CppDumper 6.7.46 runs clean against the v2.0.0 `main` NSO + metadata (no manual offsets needed):

- Metadata / IL2CPP version 24.4
- CodeRegistration `0x48FD9B8`, MetadataRegistration `0x48FF178`
- 104,127 methods with addresses, 73 assemblies, 1,963 types in Assembly-CSharp
- Middleware: FMOD (FMODUnity), Universal Render Pipeline, TextMeshPro, Odin serializer (Sirenix), ProBuilder

```
DOTNET_ROLL_FORWARD=Major dotnet Il2CppDumper.dll extracted_v2/exefs/main \
  extracted_v2/romfs/Data/Managed/Metadata/global-metadata.dat il2cpp_dump
```

Output stays local (gitignored).

## Tools

`tools/nsp_extract.py` - decrypts an NSP with your `prod.keys` and pulls ExeFS and RomFS.
Handles standard crypto, common tickets, and update patch RomFS (BKTR indirect + AES-CTR-EX, layered over the base RomFS). Only needs the `cryptography` package.

```
python3 tools/nsp_extract.py "base.nsp" --keys prod.keys --out extracted
python3 tools/nsp_extract.py "update.nsp" --keys prod.keys --base "base.nsp" --out extracted_v2 --romfs-filter global-metadata
python3 tools/nsp_extract.py "base.nsp" --keys prod.keys --info
```

## Building on Windows

Needs git, Rust (rustup), Visual Studio Build Tools (C++ workload) and a Vulkan 1.3 GPU.
From the folder holding `extracted_v2\` (full RomFS extracted) and `tools\`:

```
powershell -ExecutionPolicy Bypass -File tools\build_windows.ps1
powershell -ExecutionPolicy Bypass -File tools\run.ps1
```

After a patch that only touches the runtime or shader translator, add `-Relink` to skip
regenerating the game's code.

`build_windows.ps1` clones Kirikae at `patches/kirikae/BASE_COMMIT`, applies the patches,
builds it, and recompiles `extracted_v2` with `tools\cruisn-blast.profile`
(generate it with `tools/il2cpp_profile.py`). `run.ps1` runs the result with exit
backtraces and stats on, logging to `run.log`.

## Status

- Recompiles: 247,518 functions, 95.2% of guest instructions reachable, 587 MB x86 text, links
- Boots through Nintendo SDK init (about 100 threads, 17 services) under the recompiled code
- On Windows with the full RomFS: Unity boots, streams assets and presents frames (2.4 fps,
  black). 7,916 of 7,941 draws skipped: 4,939 with no color/depth target, about 2,970 using
  bindless textures (patch 0003 addresses those). Main thread spends most time in
  out-of-line SIMD/FP helpers (495M calls in 40 s)

## Plan

1. Extract base + update (ExeFS, RomFS incl. patch RomFS)
2. IL2CPP dump from `global-metadata.dat` + `main` for full class/method symbols
3. Identify Unity engine and Nintendo SDK functions
4. Static recompile AArch64 -> native x64
5. Runtime: Horizon OS services, NVN -> Vulkan, audio, input, filesystem
