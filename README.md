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

- Recompiles: 247,852 functions, 95.2% of guest instructions reachable, 587 MB x86 text, links.
  Seeded with IL2CPP method entries plus addresses earlier runs interpreted; at runtime the
  interpreter is effectively unused (hundreds of instructions per session)
- Playable on Windows (i9-13900K / RTX 4070 Ti) at 60 fps: boot, menus, car select with the
  3D car and the rendered car icons in the selector, races with the full 3D scene,
  post-processing and HUD; FMOD audio works
- Kirikae patches 0001-0012 (see `patches/kirikae/README.md`): function discovery, bindless
  textures (NVN texture|sampler handles), TLD/TXQ, compressed-texture aliasing, layout-aware
  inline uploads, MSAA resolve blits, render-target write-back for copy-engine reads, plus
  diagnostics
- Settings in `build\cruisn-blast.ini` (written on first start, patch 0012): window size,
  borderless fullscreen (F11 / Alt+Enter), vsync, aspect-correct scaling, forced 16x
  anisotropic filtering, render resolution and frame rate (both experimental above stock)
- Open issues:
  - Hitches the first time new materials appear (driver pipeline compiles); a second pass
    over the same content is much smoother thanks to the driver pipeline cache
  - 8 of 1,332 shaders fail naga validation (BreakOutsideOfLoopOrSwitch), same on
    unpatched Kirikae

## Plan

1. Extract base + update (ExeFS, RomFS incl. patch RomFS)
2. IL2CPP dump from `global-metadata.dat` + `main` for full class/method symbols
3. Identify Unity engine and Nintendo SDK functions
4. Static recompile AArch64 -> native x64
5. Runtime: Horizon OS services, NVN -> Vulkan, audio, input, filesystem
