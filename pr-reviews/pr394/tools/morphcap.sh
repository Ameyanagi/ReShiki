#!/bin/bash
# Usage: morphcap.sh <artifact dir> <wid-helper>
# Waits for <dir>/morph-go (which holds the app PID), finds that PID's window and
# captures it 40 times as fast as screencapture allows, logging ms timestamps.
D=$1; WIDBIN=$2
while [ ! -s "$D/morph-go" ]; do sleep 0.05; done
PID=$(cat "$D/morph-go"); WID=$($WIDBIN $PID)
echo "pid $PID wid $WID" > "$D/morph-raw/log.txt"
for i in $(seq -w 1 40); do
  screencapture -x -o -l $WID "$D/morph-raw/f$i.png"
  perl -MTime::HiRes=time -e 'printf "f'$i' %.0f\n", time*1000' >> "$D/morph-raw/log.txt"
done
touch "$D/morph-done"
