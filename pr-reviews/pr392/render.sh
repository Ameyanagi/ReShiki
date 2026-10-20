#!/bin/zsh
# Usage: render.sh <reshiki binary> <out dir>
# Exports each input (CDXML, or the app-drawn .rsk) through the CLI (same path as the app's Export).
set -u
B=$1; O=$2; D=${0:a:h}
mkdir -p $O
for f in $D/inputs/*.cdxml(N) $D/inputs/*.rsk(N); do
  n=${f:t:r}
  $B --cli convert $f --to png   -o $O/$n-1200dpi.png   --force 2>>$O/stderr.log   # app Export PNG (1200 dpi, ~16.7x)
  $B --cli convert $f --to svg   -o $O/$n.svg   --force 2>>$O/stderr.log   # vector export
  $B --cli convert $f --to cdxml -o $O/$n.cdxml --force 2>>$O/stderr.log   # ChemDraw export
  $B --cli render  $f --to png   -o $O/$n-preview.png --width 1600 --height 1000 --force 2>>$O/stderr.log
done
