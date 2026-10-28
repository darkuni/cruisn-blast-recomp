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
