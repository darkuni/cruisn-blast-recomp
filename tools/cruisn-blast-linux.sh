#!/bin/sh
# Cruis'n Blast - native Linux build (test). Put this and cruisn-blast in the same folder as
# the Windows build's game/ folder and cruisn-blast.shaders (made by setup.exe).
# Writes cruisn-blast.log next to itself. In Steam: add as a non-Steam game; for frame
# statistics set launch options to: KIRIKAE_STATS=1 %command%
cd "$(dirname "$(readlink -f "$0")")" || exit 1
exec ./cruisn-blast "$@" > cruisn-blast.log 2>&1
