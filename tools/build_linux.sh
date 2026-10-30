#!/bin/sh
# Build the native Linux version (Steam Deck / SteamOS) of Cruis'n Blast.
#
#   tools/build_linux.sh <project folder: extracted_v2/exefs, tools/cruisn-blast.profile> [out-dir]
#
# Needs: git, Rust (rustup), libasound2-dev and libudev-dev (or your distro's equivalents),
# and for a binary that runs on older glibc (SteamOS included): pip install ziglang
# cargo-zigbuild. Uses the same patches and profile as the Windows build. The result reads
# its game files from game/ next to it (made by setup.exe on Windows, or tools/setup) and
# needs cruisn-blast.shaders there too.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
dump=${1:?project folder holding extracted_v2/exefs and tools/cruisn-blast.profile}
out=${2:-$here/build-linux}
mkdir -p "$out"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

kirikae="$out/kirikae"
if [ ! -d "$kirikae/.git" ]; then git clone https://github.com/Cor3Duo/kirikae "$kirikae"; fi
(cd "$kirikae" && git fetch -q origin && git checkout -q --force "$(cat "$here/patches/kirikae/BASE_COMMIT")" && git clean -fdq &&
  for p in "$here"/patches/kirikae/*.patch; do git apply --whitespace=nowarn "$p"; done)
export RUST_MIN_STACK=268435456
(cd "$kirikae" && cargo build --release -p kirikae)
k="$kirikae/target/release/kirikae"

# The ExeFS only; an empty romfs.bin stands in for the RomFS (the game reads game/romfs.bin).
mkdir -p "$work/dump/exefs"
cp "$dump"/extracted_v2/exefs/* "$work/dump/exefs/"
printf x > "$work/dump/romfs.bin"
cat "$dump/tools/cruisn-blast.profile" > "$work/merged.profile"
[ -f "$dump/build/cruisn-blast.profile" ] && cat "$dump/build/cruisn-blast.profile" >> "$work/merged.profile"
"$k" build "$work/dump" -o "$out" --name cruisn-blast --profile "$work/merged.profile" --fast-regs

project="$out/cruisn-blast-build"
cp "$project/src/package.bin" "$project/src/package.bin.dev"
"$k" dev external-package "$project/src/package.bin.dev" "$project/src/package.bin"
if command -v cargo-zigbuild >/dev/null; then
  (cd "$project" && RUSTFLAGS="-C link-arg=-Wl,--allow-shlib-undefined" cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.31)
  bin="$project/target/x86_64-unknown-linux-gnu/release/cruisn-blast"
else
  echo "cargo-zigbuild not found: linking against this system's glibc" >&2
  (cd "$project" && cargo build --release)
  bin="$project/target/release/cruisn-blast"
fi
mv "$project/src/package.bin.dev" "$project/src/package.bin"

pkg="$work/pkg"
mkdir -p "$pkg"
cp "$bin" "$pkg/cruisn-blast"
cp "$here/tools/cruisn-blast-linux.sh" "$pkg/cruisn-blast.sh"
chmod +x "$pkg/cruisn-blast" "$pkg/cruisn-blast.sh"
tar -C "$pkg" -cJf "$out/CruisnBlast-linux.tar.xz" cruisn-blast cruisn-blast.sh
echo "Done: $out/CruisnBlast-linux.tar.xz"
