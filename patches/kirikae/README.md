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
