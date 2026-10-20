#!/bin/bash
# usage: render_all.sh <reshiki binary> <ev dir>
set -e
B=$1; EV=$2
for tag in own proto; do
  out=$EV/renders/$tag; mkdir -p $out
  for f in $EV/input/$tag/*.rsk; do
    n=$(basename $f .rsk)
    $B --cli convert $f -o $out/$n.svg --force
    $B --cli convert $f -o $out/$n-export-1200dpi.png --force 2>>$out/convert.log
    w=$(python3 -c "import re,sys;print(round(float(re.search(r'viewBox=\"[^ ]+ [^ ]+ ([^ ]+)',open(sys.argv[1]).read()).group(1))))" $out/$n.svg)
    $B --cli render $f -o $out/$n-100pct.png --width $w --height 4000 --force
    echo "$n viewBox-width=$w" >> $out/convert.log
  done
done
