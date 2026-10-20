#!/bin/bash
# usage: render.sh <reshiki binary> <input dir> <out dir>
# Per input: the SVG export, the 1200-dpi PNG export (both from the repo CLI),
# then 100% (1 px per drawing unit) and 400% images resampled from that export
# by derive.py, plus a 400% crop around a 3-bond ring atom joint.
set -e
B=$1; IN=$2; OUT=$3; T=$(dirname "$0")
mkdir -p "$OUT"
for f in "$IN"/*.rsk; do
  n=$(basename "$f" .rsk)
  "$B" --cli convert "$f" -o "$OUT/$n.svg" --force
  "$B" --cli convert "$f" -o "$OUT/$n-export-1200dpi.png" --force 2>>"$OUT/convert.log"
  joint=$(python3 -I "$T/joint.py" "$f")
  uv run --quiet --with pillow python -I "$T/derive.py" "$OUT/$n.svg" "$OUT/$n-export-1200dpi.png" "$OUT/$n" $joint
  echo "$n joint $joint" >> "$OUT/convert.log"
done
