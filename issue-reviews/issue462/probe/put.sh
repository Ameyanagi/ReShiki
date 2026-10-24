#!/bin/bash
# put.sh CASE [SET]  -- put a marked picture on the general pasteboard (SET: marked or marked2)
cd "$(dirname "$0")"; S=${2:-marked}
case $1 in
  png) ./pb put public.png $S/marked.png ;;
  svg) ./pb put public.svg-image $S/marked.svg ;;
  pdf) ./pb put com.adobe.pdf $S/marked.pdf ;;
  combo) ./pb put com.adobe.pdf $S/marked.pdf public.png $S/marked.png public.svg-image $S/marked.svg ;;  # like ReShiki Copy image
  *) echo "unknown case"; exit 1 ;;
esac
