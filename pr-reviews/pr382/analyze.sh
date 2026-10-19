#!/bin/sh
# Analyze and render each saved drawing with the branch's CLI build.
# Usage: analyze.sh RESHIKI_BIN EVIDENCE_DIR
bin="$1"
dir="$2"
for side in before after; do
  out="$dir/$side/analysis.txt"
  : > "$out"
  for name in morpholino 2-thienyl MeO NC; do
    file="$dir/$side/$name.rsk"
    if [ ! -f "$file" ]; then
      echo "$name: no drawing (Apply failed, see harness.txt)" >> "$out"
      continue
    fi
    json=$("$bin" --cli analyze "$file" 2>/dev/null | grep '^{')
    echo "$name: $(printf '%s' "$json" | python3 -c 'import json,sys; a=json.load(sys.stdin)["value"]["analysis"]; print("formula=%s smiles=%s mass=%s" % (a["formula"], a["smiles"], a["mass"]))')" >> "$out"
    "$bin" --cli render "$file" -o "$dir/$side/$name.png" 2>/dev/null
  done
done
