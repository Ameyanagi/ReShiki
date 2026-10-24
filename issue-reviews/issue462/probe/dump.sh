#!/bin/bash
# dump.sh NAME -- dump every pasteboard item/type to out/NAME and report which markers survived
cd "$(dirname "$0")"; mkdir -p out
./pb dump out/$1 > out/$1.txt; echo "--- markers:" >> out/$1.txt
python3 -I scan.py out/$1 >> out/$1.txt 2>&1; cat out/$1.txt
