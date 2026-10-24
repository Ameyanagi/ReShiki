#!/usr/bin/env bash
# Package the browser editor demo for https://reshiki.com/demo/ as a
# reshiki-web-demo.tar.gz holding one demo/ folder: examples/demo.html as its
# page, the JavaScript it imports, the editor's WebAssembly and the license
# texts, all linked by relative paths. Run scripts/build_wasm.sh first, or pass --build to
# run it here. The archive goes to dist/reshiki-web-demo.tar.gz unless a path
# is given. --previous ARCHIVE keeps the files that archive's page loads, so a
# page cached before this release can still load its own files.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
package=$root/packages/editor

previous=
while [[ $# -gt 0 ]]; do
  case $1 in
    --build) "$root/scripts/build_wasm.sh"; shift ;;
    --previous) previous=$(cd "$(dirname "$2")" && pwd)/$(basename "$2"); shift 2 ;;
    *) break ;;
  esac
done
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

# The demo page sits one folder below src/ and dist/ in the package and beside
# them in the demo. It shows a download progress bar, so it is told the
# WebAssembly's size; a compressed response's Content-Length is smaller.
size=$(wc -c < "$package/dist/editor/reshiki_bg.wasm" | tr -d ' ')
SIZE=$size perl -pe 's#"\.\./(src|dist)/#"./$1/#g; s/data-bytes="0"/data-bytes="$ENV{SIZE}"/' \
  "$package/examples/demo.html" > "$demo/index.html"
if grep -q '"\.\./' "$demo/index.html" || ! grep -q "data-bytes=\"$size\"" "$demo/index.html"; then
  echo "packages/editor/examples/demo.html no longer links ../src/ and ../dist/ as expected" >&2
  exit 1
fi
# release.yml compares this with the published demo's.
awk -F '"' '/^version = / { print $2; exit }' "$root/Cargo.toml" > "$demo/VERSION"
cp "$package/src/editor.js" "$package/src/fonts.js" "$demo/src/"
cp "$package/dist/editor/reshiki.js" "$package/dist/editor/reshiki_bg.wasm" "$demo/dist/editor/"
cp -R "$package/dist/licenses" "$demo/licenses"

# Name the modules and the WebAssembly by a hash of their content and point the
# files that load them at the new names. A release then changes every name, so
# a browser never pairs new JavaScript with old WebAssembly, and the files can
# be cached for as long as the host allows. Dependencies go first, so each hash
# covers the names the file loads. index.html keeps its name.
hashed() {
  local file=$demo/$1 name=${1##*/} hash new
  hash=$(shasum -a 256 "$file" | cut -c1-12)
  new=${name%.*}.$hash.${name##*.}
  mv "$file" "${file%/*}/$new"
  NAME=$name NEW=$new perl -pi -e 's/(?<![\w.-])\Q$ENV{NAME}\E(?![\w-])/$ENV{NEW}/g' \
    "$demo/index.html" "$demo"/src/*.js "$demo"/dist/editor/*.js
  if ! grep -qrF "$new" "$demo/index.html" "$demo/src" "$demo/dist/editor"; then
    echo "No demo file loads $1 any more" >&2
    exit 1
  fi
}
hashed dist/editor/reshiki_bg.wasm
hashed dist/editor/reshiki.js
hashed src/fonts.js
hashed src/editor.js

# index.html can stay cached for 10 minutes after a deploy, and that page asks
# for the previous release's names. Keep the files it loads, following its
# imports, so files older than one release drop out.
if [[ -n $previous ]]; then
  old=$stage/previous
  mkdir -p "$old"
  tar -xzf "$previous" -C "$old"
  queue=index.html seen=
  while [[ -n $queue ]]; do
    file=${queue%% *}
    queue=${queue#"$file"}; queue=${queue# }
    for name in $(grep -oE '[A-Za-z0-9_-]+\.[0-9a-f]{12}\.(js|wasm)' "$old/demo/$file" | sort -u); do
      [[ " $seen " == *" $name "* ]] && continue
      seen="$seen $name"
      path=$(cd "$old/demo" && find src dist -name "$name" | head -n 1)
      [[ -n $path ]] || continue
      [[ -e $demo/$path ]] || cp "$old/demo/$path" "$demo/$path"
      if [[ $name == *.js ]]; then queue="${queue:+$queue }$path"; fi
    done
  done
fi

# No macOS metadata, which GNU tar warns about when docs.yml extracts it.
COPYFILE_DISABLE=1 tar --no-xattrs -czf "$output" -C "$stage" demo
echo "$output: $(wc -c < "$output" | tr -d ' ') bytes"
