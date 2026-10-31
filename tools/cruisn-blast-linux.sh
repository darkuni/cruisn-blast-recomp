#!/bin/sh
# Cruis'n Blast - native Linux build. setup (in this folder) makes the cruisn-blast program and
# game/ from your own NSPs and prod.keys.
# Writes cruisn-blast.log next to itself. In Steam: add as a non-Steam game; for frame
# statistics set launch options to: KIRIKAE_STATS=1 %command%
cd "$(dirname "$(readlink -f "$0")")" || exit 1
exec ./cruisn-blast "$@" > cruisn-blast.log 2>&1
