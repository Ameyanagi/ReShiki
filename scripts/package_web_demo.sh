#!/usr/bin/env bash
# Package the browser editor demo for https://reshiki.com/demo/ as a
# reshiki-web-demo.tar.gz holding one demo/ folder: the editor-only page, the
# JavaScript it imports, the editor's WebAssembly and the license texts, all
# linked by relative paths. Run scripts/build_wasm.sh first, or pass --build to
# run it here. The archive goes to dist/reshiki-web-demo.tar.gz unless a path
# is given.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
package=$root/packages/editor

if [[ ${1:-} == --build ]]; then
  "$root/scripts/build_wasm.sh"
  shift
fi
output=${1:-$root/dist/reshiki-web-demo.tar.gz}
mkdir -p "$(dirname "$output")"
output=$(cd "$(dirname "$output")" && pwd)/$(basename "$output")

for file in dist/editor/reshiki.js dist/editor/reshiki_bg.wasm dist/licenses; do
  if [[ ! -e $package/$file ]]; then
    echo "Missing packages/editor/$file: run scripts/build_wasm.sh first" >&2
    exit 1
  fi
done

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
demo=$stage/demo
mkdir -p "$demo/src" "$demo/dist/editor"

# The example page sits one folder below src/; the demo page sits beside it.
sed 's|"\.\./src/editor\.js"|"./src/editor.js"|' "$package/examples/index.html" > "$demo/index.html"
if ! grep -q '"./src/editor.js"' "$demo/index.html"; then
  echo "packages/editor/examples/index.html no longer imports ../src/editor.js" >&2
  exit 1
fi
cp "$package/src/editor.js" "$demo/src/"
cp "$package/dist/editor/reshiki.js" "$package/dist/editor/reshiki_bg.wasm" "$demo/dist/editor/"
cp -R "$package/dist/licenses" "$demo/licenses"

COPYFILE_DISABLE=1 tar -czf "$output" -C "$stage" demo
echo "$output: $(wc -c < "$output" | tr -d ' ') bytes"
