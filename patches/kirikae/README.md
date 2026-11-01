# Kirikae patches

Applied on top of Cor3Duo/kirikae at the commit in `BASE_COMMIT`.

```
git clone https://github.com/Cor3Duo/kirikae && cd kirikae
git checkout $(cat ../patches/kirikae/BASE_COMMIT)
git apply ../patches/kirikae/*.patch
```

## 0001 - discover: stop at other function entries

`build_function` followed fall-through, conditional fall-through and post-call returns
without checking whether the address is another function's entry. IL2CPP methods often
end in a call to a throw helper that never returns, with the next method placed right
behind it, so each function absorbed the whole chain of methods after it.

On Cruis'n Blast 2.0.0 that produced 2.38 GB of x86 text (a 92-instruction
`InternalArray__set_Item<KeyValuePair<int,int>>` alone compiled to 1.35 MB) and the
link failed with R_X86_64_GOTPCREL out of range.

With the patch those edges leave through the dispatcher, which enters the compiled
function at that address, so control flow is unchanged. Kirikae's recompiler tests pass.

## 0002 - rt: KIRIKAE_TRACE_FS and KIRIKAE_TRACE_EXIT

Diagnostics. `KIRIKAE_TRACE_FS=1` logs each RomFS lookup (GetEntryType, OpenFile,
OpenDirectory) and its result. `KIRIKAE_TRACE_EXIT=1` prints a guest frame-pointer
backtrace when the game calls svcExitProcess. Off by default.

## 0003 - maxwell: bindless TEX (TEX.B)

TEX.B was rejected outright, so every Unity/NVN draw sampling a texture was skipped
(about 2,970 of 7,941 draws in the first 40 s of Cruis'n Blast). The handle register
(Rb) is now traced back to the constant buffer it was loaded from and the access becomes
a bound texture at that slot and word, which the runtime already supports.

- `cbuf_source` also accepts `LDC Rd, c[s][imm]` with no index register (including the
  later words of a 64/128-bit LDC) and follows plain register `MOV` copies.
- TEX.B field positions per envytools gm107: LOD mode in bits 37-39 (TEX: 55-57), AOFFI in
  bit 36 (TEX: 54); the remaining B operands start at Rb+1.
- Tests: handle from LDC, from the high word of LDC.64 with explicit LOD, through a register
  copy, and a computed handle that stays unsupported. All translator tests pass.

## 0004 - NVN texture | sampler handles

0003 alone resolved none of Cruis'n Blast's bindless samples. Disassembling the five
skipped shaders (148 TEX.B in all) showed every handle built the same way:

```
mov    Ra, c2[A]          ; texture handle
lop.or Rd, Ra, c2[B]      ; | sampler handle
(mov   Rx, Rd)            ; optional copy
```

The translator now resolves LOP.OR of two constant-buffer words (cbuf or register second
operand, no inversions) and records the second word in `TextureUse::or`; the runtime ORs
both words before splitting TIC/TSC indices. The stashed word is cleared per instruction so
it cannot leak into a later texture. TMML.B uses the same resolution.

Tests: the exact Cruis'n Blast pattern and a leak check. With `KIRIKAE_SHADER_DIR` pointed
at the five dumped shaders, `dumped_shaders` reports 5/5 translating completely.

## 0005 - maxwell: TLD and bindless TXQ

With 0004 the menus and HUD rendered but the 3D scene never reached the screen: Unity's
URP post-process filter (3x3 texel fetches) and MSAA resolve both use TLD.B, and Kirikae had
no TLD translation at all, plus the resolve sizes its target with TXQ.B.

- TLD, bound (0xdc38) and bindless (0xdd38), per envytools gm107 fields: A registers
  [layer], coords; B registers [handle], [lod], [offset], [sample]. Host render targets are
  single-sample, so the sample index is consumed and ignored.
- TXQ.B: handle from Ra (through the same NVN texture|sampler resolution), LOD from Ra+1.

All 1,332 shaders dumped from a Cruis'n Blast session: 753 translate completely on
unpatched Kirikae, 1,324 with 0003-0005; nothing is reported missing. The other 8 fail naga
validation with BreakOutsideOfLoopOrSwitch identically with and without these patches
(pre-existing control-flow structuring issue).

## 0006 - rt: explain "no color or depth target" skips (diagnostic)

The skip reason now says whether depth was off with no color target, the depth format was
unsupported, its address unmapped, its size zero, or its image not created, so the skip
statistics separate the cases. Added to find why Cruis'n Blast's car-selector thumbnails
(4,939 draws during loading) are blank.

## 0007 - compressed textures never alias render-target images

Cruis'n Blast's car-selector tiles are eight 500x300 BC1 textures streamed from asset
bundles. They sampled black (one striped). `find_image` matches any existing image at a
texture's address in any format, and Unity's memory pool reuses freed render-target memory
for textures, so the lookup could return a stale render target. Render targets are marked
GPU-written, which also stops the texture from ever being re-read from memory.

A GPU cannot render into BC/ETC2/EAC/ASTC formats, so a block-compressed texture now only
reuses an image of the same format class and is otherwise uploaded from memory. With
KIRIKAE_TRACE_GPU on, each texture bind also logs which image it resolved to and whether
that image is GPU-written. Runtime unit tests: 198 pass.

## 0008 - rt: texture dump with raw bytes, header, and size matching (diagnostic)

`KIRIKAE_TEX_DUMP=dir:` plus `KIRIKAE_TEX_DUMP_SIZE=WxH` dumps every texture of that size
on each upload: decoded bytes (.bin), raw guest memory (.raw) and the texture header (.txt),
so a bad texture can be decoded offline under other layouts.

## 0009 - inline-to-memory honors the destination layout; upload trace

Inline-to-memory (3D/compute/inline class, LAUNCH_DMA 0x6c + LOAD_INLINE_DATA 0x6d) wrote
its data as one linear run at the destination, ignoring LAUNCH_DMA bit 0 (1 pitch, 0
block-linear), PITCH_OUT, DST_BLOCK_SIZE, DST_WIDTH and the origin. It now writes pitch
lines PITCH_OUT apart and block-linear destinations through the same block-linear writer as
the copy engine.

`KIRIKAE_TRACE_UPLOADS=1` logs every inline, copy-engine and 2D transfer with its
destination and layout, without the cost of the full GPU trace.
Runtime unit tests: 198 pass (no dedicated test for the block-linear inline path yet).

## 0010 - MSAA resolve blits

The car-selector tiles come from `CarSelectScreen.RefreshIconRenderTexture`: an icon camera
renders each car into a shared 8x MSAA RenderTexture, then `Texture2D.ReadPixels` + `Apply`,
and later `Texture2D.Compress` (BC1). ReadPixels first resolves the MSAA target with a 2D
blit from the 2000x600 sample storage (4x2) to 500x300, source rect [1.5, 0.5, 2001.5, 600.5].

Host render targets are single-sample, so Kirikae draws the 500x300 viewport into the
top-left of a 2000x600 image; the resolve then shrank the whole image, giving a quarter-size
car in a mostly empty icon.

Render-target images now record the sample grid of MULTISAMPLE_MODE (method 0x574 / byte
0x15d0, envytools g80_msaa_mode) they were drawn with. A 2D blit from such an image whose
scale factors equal that grid and whose destination is exactly storage / grid copies the
top-left logical region 1:1. Other blits are unchanged. Runtime tests: 198 pass.

## 0011 - write render targets back for copy-engine reads (ReadPixels)

The car icons: render into an 8x MSAA target, resolve (0010), bloom, composite into a second
MSAA target, resolve, then a resolve-shader draw into a 500x300 staging target, and finally
`ReadPixels` reads that target with the copy engine (block-linear -> pitch, 4000 bytes x 300).
Kirikae keeps render targets in Vulkan images and had no path writing them back to guest
memory for copy-engine reads, so the readback returned zeros and every icon was black.

When a copy-engine transfer reads block-linear memory that starts at a GPU-written,
single-level color image, the image is now read back from the GPU and written into guest
memory with the transfer's own block-linear geometry before the copy. With
KIRIKAE_TRACE_UPLOADS set, each write-back is logged. Runtime tests: 198 pass.

## 0012 - player settings (`cruisn-blast.ini`)

New `settings` module: reads `<exe name>.ini` next to the executable and writes a commented
default file on first start. Options:

| Section | Key | Default | Effect |
|---|---|---|---|
| display | `window_width`, `window_height` | 1920 x 1080 | Window size in physical pixels (was a fixed 1280x720 logical window, so the game's 1080p frames were shown downscaled) |
| display | `fullscreen` | false | Start in borderless fullscreen. F11 or Alt+Enter toggles at runtime (Alt+Enter is not passed to the game as +) |
| display | `vsync` | false | FIFO present mode (same as `KIRIKAE_VSYNC`) |
| display | `keep_aspect` | true | Letterbox / pillarbox instead of stretching |
| display | `scale_filter` | linear | `linear` or `nearest` scaling of the frame to the window |
| graphics | `anisotropy` | 16 | Forced on linear, mipmapped, non-depth-compare samplers; never lower than the game asks |
| graphics | `render_width`, `render_height` | 1920 x 1080 | Docked display resolution reported to the game (am ICommonStateGetter 60). Experimental above 1080p |
| performance | `frame_rate` | 60 | Rate for the vsync event and frame pacing. Experimental above 60 |

Runtime tests: 201 pass (settings parsing, default file, letterbox math).

## 0013 - settings menu, controls, rumble and shader preloading

**Settings menu.** The same in-game menu as the other native ports (layout, colours and the
DejaVu Sans Mono Bold font atlas taken from the Einhander port): F1 or Esc, or Start+Back on a
controller. The game is held while it is open (the vi present path keeps re-showing its last
frame and polling the controller). Options: resolution (reported docked resolution, restart),
aspect ratio, VSync (swapchain is remade on change), fullscreen, scaling filter, anisotropic
filtering (restart), frame rate (restart), vibration and strength, swap A/B and X/Y, frame
rate display, keyboard and controller rebinding pages, close, quit (press twice). Everything is
saved to `<exe>.ini` immediately; `settings::boot()` keeps the values the game started with.

Drawn with a small Vulkan pipeline after the frame is blitted to the swapchain (swapchain
images get COLOR_ATTACHMENT usage and views when the surface allows it). Shaders are
`overlay.wgsl` compiled to SPIR-V with naga 30 and embedded (`overlay_vs.spv`, `overlay_fs.spv`).

**Bindings.** `bindings.rs`: one table of Switch controls with keyboard defaults equal to the old
fixed map, and controller defaults by label (Xbox A = Switch A); "Swap A/B and X/Y" gives the
old positional mapping. Stick directions stay analog when bound to stick directions. XInput
and gilrs both feed a raw XInput-layout pad.

**Rumble.** hid SendVibrationValue (201) and SendVibrationValues (206) drive the XInput motors
(low band -> left motor, high band -> right), scaled by the strength setting.

**Shader preloading.** Every graphics pipeline the game asks for is recorded (SPIR-V + state)
into `<exe>.pipelines` (deflate-compressed, rewritten with the pipeline cache every 30 s and on
exit). On start all recorded pipelines are built on spare cores while the game boots, with a
"Preloading shaders n / total" line in the corner. Translated shaders with identical SPIR-V
reuse the preloaded module, so the game's own lookups hit the preloaded pipelines directly:
no driver compile and no skipped draws the first time a track appears.
`KIRIKAE_NO_PRELOAD` turns it off; `KIRIKAE_PRELOAD_FILE` points somewhere else.

Runtime tests: 207 pass, plus an ignored offscreen render test of the menu
(`render_menu_offscreen`, writes `KIRIKAE_OVERLAY_PNG`).

## 0014 - compile every game shader at startup (pipeline libraries)

The game's own shader programs (all 32,501 Switch/Maxwell programs in its Unity data, pulled
out at build time by `tools/extract_shaders.py` into `build\cruisn-blast.shaders`) are
translated and compiled on spare cores when the game starts. On the first launch the game
waits on its first frame behind a "Preparing shaders" progress screen until they are done;
later launches hit the driver's caches.

How it can compile ahead without knowing draw state: with VK_EXT_graphics_pipeline_library
each pipeline is linked from four parts (vertex input, vertex shader, fragment shader,
fragment output). The two shader parts are the expensive ones, and they are made independent
of everything else:

- vertex resources are in descriptor set 0 and fragment resources in set 1, with
  INDEPENDENT_SETS layouts and per-stage set visibility (translator option `descriptor_set`)
- the viewport depth transform is a push constant instead of constants baked into the vertex
  shader (translator option `depth_push`)
- depth clamp is dynamic state (VK_EXT_extended_dynamic_state3) when available

A draw then only links the parts (measured 0.03-0.1 ms per pipeline on lavapipe). Shader
parts the preload did not cover (3 programs with indirect-branch tables that read constant
buffers, Unity's 2 built-in engine shaders, rare draw-state variants) compile when first
drawn, and are counted.

Without pipeline library support (or `KIRIKAE_NO_GPL`) everything works as before, including
the recorded `.pipelines` preload from 0013. `KIRIKAE_NO_SHADER_WAIT` skips the first-launch
wait; `KIRIKAE_SHADERS` points at another shader file.

Also fixed: the translator decorated the shared constant-buffer array type with ArrayStride
once per buffer, which is invalid SPIR-V (spirv-val rejected 294 of the first 300 game
programs; NVIDIA tolerated it).

New stats line (KIRIKAE_STATS): frames over 33 ms and over 100 ms, shader parts compiled ahead
vs while playing, pipelines linked.

Verified offline: every program translates; library compiles and links of the game programs
pass the Khronos validation layer on lavapipe; tests `game_shaders` (maxwell) and
`compile_and_link_game_shaders` (rt), both ignored by default and driven by KIRIKAE_SHADERS.

## 0015 - explain slow frames

With KIRIKAE_STATS set, every frame over 50 ms logs where the renderer's time went (drawing,
GPU waits, readbacks, shader translation, pipeline links, shader compiles, texture uploads,
new images, pipeline cache saves, streamed MiB); what is left is the game's own CPU time.
Pipelines that take over 5 ms and pipeline cache saves (size, time) are logged too.

## 0016 - profanity filter without the system word list, per-thread slow-frame report

After a race the game checks the typed initials with nn::ngc::ProfanityFilter. The filter needs
system data 0x0100000000000823 (the NG word list), which a game dump does not contain; with
the empty archive Kirikae serves, the SDK aborted (svcBreak from MaskProfanityWordsInTextImpl).
`ngc.rs` replaces CheckProfanityWords (all character types) and MaskProfanityWordsInText with
host functions that answer like a console whose list matches nothing: every word clean,
nothing masked, success.

Slow frames (0015) now also list the busiest game threads in that frame by category (native
code, SVCs, IPC, lock waits, ...), so stalls inside the game can be told apart.

## 0017 - environment variables for the game's getenv

The slow frames in races (0016's per-thread report) are the main game thread running 45-130 ms
of the game's own code where a normal frame takes ~7 ms, every few seconds, with no rendering,
file or lock activity: the pattern of garbage collections. Unity's IL2CPP runtime here uses the
Boehm collector, which reads its tuning from the environment (GC_FREE_SPACE_DIVISOR,
GC_INITIAL_HEAP_SIZE, GC_PRINT_STATS, GC_ENABLE_INCREMENTAL, GC_PAUSE_TIME_TARGET, ...).

`guest_env.rs` replaces libc `getenv` (when any values are configured) with a host function
answering from the `[game_environment]` section of the settings file, or
KIRIKAE_GUEST_ENV="NAME=VALUE;...". Value strings live in a read-only guest page at
0x7f_fe00_0000.

## 0018 - sampling profiler for slow frames (Windows, KIRIKAE_SAMPLE=1)

GC_FREE_SPACE_DIVISOR=1 barely changed the slow frames (86 vs 99) and GC_PRINT_STATS printed
nothing, so the collector theory is unconfirmed. With KIRIKAE_SAMPLE=1 a sampler thread
suspends each guest thread about every millisecond and reads its instruction pointer
(SuspendThread / GetThreadContext / ResumeThread, nothing allocated while stopped). Pointers in
recompiled code map back to the guest function (entries sorted by host address); each slow
frame logs the busiest thread's hottest functions, and the exit summary lists the 40 hottest
guest functions over all slow frames.

## 0019 - call stacks in the sampler

The leaf profile of slow frames was diffuse (allocation, memcpy, strcmp, mutexes). The sampler
now also copies the guest x29/x30 the thread last stored in its Cpu while it is suspended, walks
the x29 frame-record chain after resuming, and reports which guest functions were anywhere on
the stack (inclusive), per slow frame and in the exit summary, to find the game code that
triggers the work.

## 0020 - name host code in the sampler

In race slow frames about 18% of the main thread samples were in system code and 9% in the
runtime itself. Those addresses are now named with dbghelp (system DLL exports, and the
runtime functions from the PDB the build writes), per slow frame and in an exit summary of
host code.

## 0021 - compile the shaders the game uses first, the rest in the background

Compiling all 32,501 programs (21-26k unique parts) kept every part in memory: 108 s on the
first launch and 5-6 GB. Now every part the game draws with is recorded in
`<exe>.shaders-used` (one id per line: hashes, no game data). On start, only recorded parts are
compiled and kept behind the "Preparing shaders" screen; the rest compile afterwards on half
the threads at the lowest priority, straight into the driver's cache (not ours) and are then
dropped, so a later first use is a driver-cache hit. With no record yet, everything is compiled
as before. On lavapipe with 3,000 programs: 17.4 s / 1.9 GB without a record, 1.9 s / 110 MB
with one.

## 0022 - no heap allocation in the vector instruction interpreter

The named sampler (0020) put 14% of the main thread's slow-frame time in the Windows heap
(RtlAllocateHeap 7.7%, RtlFreeHeap 6.3%). The game does not use the host heap; the runtime
does: NEON instructions the recompiler does not inline run through `kirikae_simd` ->
`a64::simd::exec`, and several instruction families there collected lane values into a `Vec`
for every executed instruction (two-register misc collected the destination lanes
unconditionally; across-lanes reductions, narrowing shifts, high-half arithmetic, FP narrowing
and TBL too). These now use a 16-lane stack array (`Lanes`) and a fixed [u128; 4] table.
a64 tests pass. Also: sampler names the module of system symbols from the address, and the
Win32 declarations match the existing ones (no redeclaration warnings).

## 0023 - distribution builds without the game

A recompiled exe embeds `package.bin`: the game's ExeFS files and the absolute path of the
RomFS image. For testers the exe must carry neither. `kirikae dev external-package` turns
`package.bin` into a list of ExeFS file names, sizes and FNV-1a hashes plus a RomFS name
(`romfs.bin`); `tools/package.ps1` links the distribution exe with it. At startup that exe
reads `game\exefs\*` next to itself (or `KIRIKAE_GAME`), refuses files of another game
version, and opens `game\romfs.bin` (any valid RomFS image: the game mounts the raw storage).
Problems before the window opens are shown in a message box. When the process has no
stderr (GUI subsystem, started from Explorer), stdout and stderr go to `<exe>.log`, and a
panic shows a message box naming the log. Development builds are unchanged.

## 0024 - lift the hot vector instructions

On a Steam Deck the build was unplayable while Ryujinx runs the game smoothly. The cause is
CPU time in the runtime: the main thread ran about 7 million NEON instructions a second
through the vector interpreter (`Stmt::Simd` -> `kirikae_simd`), each one decoded and executed
through a `dyn Env`, and each one also flushing the recompiler's cached registers. A histogram
of one race (`KIRIKAE_SIMD_HIST`, 1.38 billion calls in 190 s) put 37% in bit counting (CNT,
UADDLP, UADDLV, ADDV), then lane and structure loads/stores, reciprocal square root
estimate/step, integer<->float conversions, vector divide, pairwise min/max, shifts.

`a64/src/lift_neon.rs` lifts those to IR the Cranelift backend compiles inline: CNT, NOT,
UADDLP, ABS/NEG, SCVTF/UCVTF, FCVTZS/FCVTZU, FRECPE/FRSQRTE, FSQRT (vector), UADDLV, ADDV,
S/U MAX/MIN (and pairwise), CMGT/CMHI/CMGE/CMHS/CMTST/CMEQ, MLA/MLS/MUL, SSHL/USHL, FDIV,
FABD, FRECPS/FRSQRTS, SSHR/USHR/SSRA/USRA, all modified immediates (MOVI/MVNI/ORR/BIC/FMOV),
FADDP (scalar), LD1-4/ST1-4 (multiple structures), LD1/ST1 (single lane) and LD1R, with
post-index writeback. The estimate and step instructions use four new ops that call the same
functions as the interpreter. That covers 99.3% of the recorded calls.

Tests: every form is compared with `simd::exec` on random registers, memory and FPCR values
(a64 `lift_neon` tests; four deliberate bugs were each caught), and
`recomp/tests/clif_neon.rs` compiles the lifted code with Cranelift and compares the result
with the interpreter. `cargo run --example simd_coverage <hist>` (a64) reports how much of a
histogram is still interpreted.

## 0025 - compares compile to native compare-and-branch

Every AArch64 flag-setting instruction (CMP, CMN, ADDS, SUBS, ANDS/TST, ADCS...) computed all
four flags, packed them into NZCV and stored it, and every `b.cond`/`csel` unpacked the one bit
it needed again: about 25 x86 instructions for a compare and branch. `recomp/src/flags.rs`
finds the flag-setting statements whose flags are only read as conditions (`Cond`) or the
carry inside their own block and are dead when the block ends (a function-wide liveness pass;
flags are dead after calls and at returns, as the AArch64 procedure call standard allows,
and stay live across indirect jumps and jumps out of the function). For those the code
generator keeps the operands instead of NZCV and emits one native comparison per condition
(the inverse comparison for negated conditions). 10% less code for the whole game (609 ->
549 MB); the hottest function shrinks by 28%.

Tests: `recomp/tests/flag_fusion.rs` compiles 600 random compare-heavy functions (32- and
64-bit compares, ADDS, ADCS, ANDS, conditions read in later blocks, calls, back edges, raw NZCV
reads) with fusion on and off and runs each 6 times from random state: registers, SP, vector
registers and memory must match. Deliberately broken conditions, carry, liveness, 32-bit
handling and inversion were each caught. `KIRIKAE_NO_FLAG_FUSION=1` at build time turns it off.

## 0026 - tighter calls and branches

- A conditional branch on a fused compare (0025) branches on the comparison itself, so the
  x86 code is `cmp` + `jcc` instead of `setcc`, `test`, `jcc`.
- Rarely taken paths are marked cold and moved to the end of each function: the after-call
  fix-up (when a call does not come back to its return address), the scheduler poll on loop
  back edges and the slow path of floating-point operations. The hot code is denser.
- With fast registers, x19-x29 and SP stay cached across calls instead of being reloaded from
  memory afterwards: the AArch64 procedure call standard makes them callee-saved, so the copies
  cached before a call (stored to memory before it, as before) still hold when it returns. A
  return to somewhere else goes through the existing resume path, which does not use them.

Tests: the fast-register and flag-fusion test callees now follow the calling convention
(restore SP, save and restore x19 through memory, clobber scratch registers) and the random
callers use x19-x26 across calls. A callee that does not restore x19, and treating caller-saved
registers as preserved, were each caught.

## 0027 - objects only

- `kirikae build --objects-only` stops after code generation: the object files and
  `src/package.bin` are written and listed in `objects.txt`, but cargo is not run. The player's
  setup links them with the prebuilt runtime library (`runtime/`) and a bundled linker, so the
  player needs neither Rust nor a C toolchain, and no download contains translated game code.

## 0028 - player name

- The Switch account nickname the game asks for (its player name) comes from the settings
  file, `[player] name = ...`, instead of the fixed `kirikae`. Up to 10 printable ASCII
  characters (no `;`); empty means `kirikae`. A settings file without the `[player]` section is
  written back with it on the next launch, so existing players get the line to edit.

## 0029 - GPU diagnostics on a lost device

- When the driver loses the device (VK_ERROR_DEVICE_LOST) the log gets the driver's fault
  report (VK_EXT_device_fault, when the driver has it: description and faulting GPU
  addresses) and the game closes with a message box instead of a panic.
- `KIRIKAE_GPU_DIAG=1` (cruisn-blast-gpu-diagnostics.bat) puts an NVIDIA checkpoint
  (VK_NV_device_diagnostic_checkpoints) before every draw, dispatch and render pass; on a loss
  the log lists the last checkpoints the GPU reached and the draws around them (vertex and
  fragment shader addresses, counts). The shader addresses work with `KIRIKAE_SKIP_SHADER`.

## 0030 - F2I of NaN is 0

- Maxwell's F2I turns NaN into 0. The translation clamped and converted it as is, which
  Vulkan leaves undefined: NVIDIA clamps a NaN to the low end, so a signed F2I gave
  0x80000000. Cruis'n Blast's wet-road shader bounds a loop by an F2I result and compares
  unsigned, so a NaN made it loop about 2^31 times and the driver reset the GPU
  (VK_ERROR_DEVICE_LOST in the second race, the rainy one). F2I now selects 0 for NaN.
- The device-lost report (0029) now marks the oldest unfinished draw: the one right after
  the oldest checkpoint the GPU reached.
