#!/bin/sh
# Stand-in libasound.so.2 and libudev.so.1 for linking the Linux build on the player's machine.
# They only name the functions the runtime calls; at run time the system's real libraries are
# loaded (SteamOS and desktop Linux have both). With them the link needs no development
# packages and does not depend on where a distribution keeps its libraries.
#   tools/make_stubs.sh <libcruisn_blast_runtime.a> <zig> <out dir>
set -eu
lib=$1 zig=$2 out=$3
mkdir -p "$out"
tmp=$(mktemp -d)
nm -u "$lib" 2>/dev/null | awk '{print $NF}' | sort -u > "$tmp/undef"
for pair in asound:libasound.so.2:'^snd_' udev:libudev.so.1:'^udev_'; do
  name=${pair%%:*} rest=${pair#*:}
  soname=${rest%%:*} pattern=${rest#*:}
  grep -E "$pattern" "$tmp/undef" | sed 's/.*/void &(void) {}/' > "$tmp/$name.c"
  "$zig" cc -target x86_64-linux-gnu.2.31 -shared -nostdlib -Wl,-soname,"$soname" -o "$out/$soname" "$tmp/$name.c"
  echo "$soname: $(wc -l < "$tmp/$name.c") functions"
done
rm -rf "$tmp"
