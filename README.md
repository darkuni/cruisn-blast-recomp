# cruisn-blast-recomp

Static recompilation of **Cruis'n Blast** (Nintendo Switch, Raw Thrills 2021) to a native Windows x64 build.
Delisted from the Switch eShop on June 30, 2026.

Tools and runtime only. Bring your own dump and your own console keys. No game files and no game code are stored in
this repo or its releases: the player's `setup` recompiles their own copy of the game on their own computer.

## Download

Get `CruisnBlast-windows.zip` (or `CruisnBlast-linux.tar.xz` for the Steam Deck and Linux) from
[Releases](https://github.com/darkuni/cruisn-blast-recomp/releases), unpack it, and drag your base NSP, the v2.0.0 update
NSP and your `prod.keys` onto `setup.exe` (Linux: `./setup <files>`). Nothing to install. Setup writes the `game` folder
and the shader file, then recompiles the game's program into `cruisn-blast.exe` on your computer (about five minutes).
Then run `cruisn-blast.exe`. Needs a Vulkan 1.3 GPU. The package's `README.txt` has the controls.

The downloads hold only our own code: `setup` (`tools/setup`), the Kirikae recompiler with `patches/kirikae` applied, the
prebuilt runtime library (`runtime/`), the Zig toolchain as the linker, and `tools/cruisn-blast.profile` (a list of code
addresses the recompiler starts from). `.github/workflows/release.yml` builds them on GitHub and attaches them to each
release; it checks the recompile-and-link path on a test program on both Windows and Linux.

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

`tools/extract_shaders.py` - pulls every compiled Switch shader program out of the game's Unity
data (`pip install UnityPy lz4`); the build runs it.

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

## Packaging for testers

```
powershell -ExecutionPolicy Bypass -File tools\package.ps1
```

Makes `dist\CruisnBlast-<date>.zip` after a build: `cruisn-blast.exe`, `setup.exe`,
`cruisn-blast.shaders-used` and a readme. The packaged exe carries the recompiled code and the
runtime but none of the game's files: it reads `game\exefs\` (checked against v2.0.0) and
`game\romfs.bin` next to itself, runs without a console and logs to `cruisn-blast.log`.
`setup.exe` (`tools/setup`, Rust, built by `package.ps1`; testers need nothing installed) makes `game\` and
`cruisn-blast.shaders` from the player's own base and update NSPs and `prod.keys`, without
writing the RomFS out as loose files (30 s on the development PC; every RomFS file and the
shader set checked identical to the Python tools' output). The tester instructions
are `tools/README-testers.txt`.

## Status

- Recompiles: 247,852 functions, 95.2% of guest instructions reachable, 587 MB x86 text, links.
  Seeded with IL2CPP method entries plus addresses earlier runs interpreted; at runtime the
  interpreter is effectively unused (hundreds of instructions per session)
- Playable on Windows (i9-13900K / RTX 4070 Ti) at 60 fps: boot, menus, car select with the
  3D car and the rendered car icons in the selector, races with the full 3D scene,
  post-processing and HUD; FMOD audio works
- Kirikae patches 0001-0023 (see `patches/kirikae/README.md`): function discovery, bindless
  textures (NVN texture|sampler handles), TLD/TXQ, compressed-texture aliasing, layout-aware
  inline uploads, MSAA resolve blits, render-target write-back for copy-engine reads, plus
  diagnostics
- In-game settings menu (F1 / Esc / Start+Back), same as the other native ports: resolution,
  aspect ratio, VSync, fullscreen, scaling, anisotropic filtering, frame rate, vibration,
  A/B swap, frame-rate display, keyboard and controller rebinding. Saved to
  `build\cruisn-blast.ini`
- Every shader in the game is prepared at startup: `tools/extract_shaders.py` (run by the build,
  and by `setup.exe` for testers) pulls all 32,501 Switch shader programs out of the game's
  Unity data into `cruisn-blast.shaders`, and the runtime compiles them as Vulkan pipeline
  libraries. The parts the game is known to draw with (`tools/cruisn-blast.shaders-used`,
  hashes only) compile behind a "Preparing shaders" screen (2.7 s on an i9-13900K / RTX 4070 Ti),
  the rest in the background at low priority (5 s). The `.pipelines` recording (0013) remains
  the fallback for drivers without pipeline library support
- Profanity filter (`nn::ngc`) replaced, which fixed the crash after a race (0016)
- Steam Deck: native Linux build (`tools/build_linux.sh`, links against glibc 2.31 with
  cargo-zigbuild) runs at 42-45 fps at 720p (main thread about 20.5 ms a frame); under Proton
  it was 28-32 fps. Patches 0024-0026 took the PC main thread from 10-14 ms to 6-7 ms a frame.
  What is left for 60 on the Deck is guest register traffic around the game's many small
  calls (registers go through the CPU state in memory at every call): passing guest registers
  in host registers between compiled functions is the next large step
- Open issues:
  - 8 of 1,332 shaders fail naga validation (BreakOutsideOfLoopOrSwitch), same on
    unpatched Kirikae

## Plan

1. Extract base + update (ExeFS, RomFS incl. patch RomFS)
2. IL2CPP dump from `global-metadata.dat` + `main` for full class/method symbols
3. Identify Unity engine and Nintendo SDK functions
4. Static recompile AArch64 -> native x64
5. Runtime: Horizon OS services, NVN -> Vulkan, audio, input, filesystem
