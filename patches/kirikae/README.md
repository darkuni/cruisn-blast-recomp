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
