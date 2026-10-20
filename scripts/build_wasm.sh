#!/usr/bin/env bash
# Build the headless browser library (crates/web) and the browser editor (the
# app itself, for wasm32) into packages/editor/dist/{wasm,editor}.
#
# Installs the wasm-bindgen CLI that matches Cargo.lock and a pinned binaryen
# release (for wasm-opt) under the target directory, so no global tools are
# needed. Pass --test to run the package tests afterwards.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target}
package=$root/packages/editor

version=$(awk '$0 == "name = \"wasm-bindgen\"" { getline; gsub(/"/, "", $3); print $3 }' \
  "$root/Cargo.lock")
bindgen=$target/tools/bin/wasm-bindgen
if [[ ! -x $bindgen || "$("$bindgen" --version)" != "wasm-bindgen $version" ]]; then
  CARGO_TARGET_DIR=$target/tools-build cargo install wasm-bindgen-cli \
    --version "$version" --locked --root "$target/tools"
  rm -rf "$target/tools-build"
fi

binaryen=version_123
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) platform=arm64-macos ;;
  Darwin-x86_64) platform=x86_64-macos ;;
  Linux-x86_64) platform=x86_64-linux ;;
  Linux-aarch64) platform=aarch64-linux ;;
  *) echo "No binaryen release for $(uname -s) $(uname -m)" >&2 && exit 1 ;;
esac
wasm_opt=$target/tools/binaryen-$binaryen/bin/wasm-opt
if [[ ! -x $wasm_opt ]]; then
  archive=binaryen-$binaryen-$platform.tar.gz
  url=https://github.com/WebAssembly/binaryen/releases/download/$binaryen/$archive
  mkdir -p "$target/tools"
  (
    cd "$target/tools"
    curl -fsSLO "$url"
    curl -fsSL "$url.sha256" | shasum -a 256 -c -
    tar xzf "$archive"
    rm "$archive"
  )
fi

# Size over speed, as one optimized unit. Only this build uses these settings.
CARGO_PROFILE_RELEASE_OPT_LEVEL=s \
  CARGO_PROFILE_RELEASE_LTO=true \
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  CARGO_PROFILE_RELEASE_PANIC=abort \
  CARGO_PROFILE_RELEASE_DEBUG=false \
  CARGO_PROFILE_RELEASE_STRIP=true \
  cargo build --manifest-path "$root/Cargo.toml" -p reshiki-web -p reshiki --lib --bin reshiki \
  --target wasm32-unknown-unknown --release --locked

rm -rf "$package/dist"
# The features are those Rust enables by default for wasm32-unknown-unknown.
for module in reshiki_web:wasm reshiki:editor; do
  name=${module%%:*}
  out=$package/dist/${module##*:}
  "$bindgen" --target web --out-dir "$out" "$target/wasm32-unknown-unknown/release/$name.wasm"
  wasm=$out/${name}_bg.wasm
  "$wasm_opt" -Os --strip-debug --strip-producers \
    --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    --enable-mutable-globals --enable-reference-types --enable-multivalue \
    "$wasm" -o "$wasm"
done

# The package carries the same license texts and notices as the desktop release.
metadata=$(mktemp)
trap 'rm -f "$metadata"' EXIT
cargo metadata --manifest-path "$root/Cargo.toml" --format-version 1 --locked > "$metadata"
uv run --no-project --isolated --python 3.12 python -c '
import json, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from license_notices import write_notices
write_notices(Path(sys.argv[2]), Path(sys.argv[3]), json.loads(Path(sys.argv[4]).read_text()))
' "$root/scripts" "$root" "$package/dist/licenses" "$metadata"

for wasm in "$package"/dist/*/*_bg.wasm; do
  raw=$(wc -c < "$wasm" | tr -d ' ')
  zipped=$(gzip -9c "$wasm" | wc -c | tr -d ' ')
  echo "$(basename "$wasm"): $raw bytes, $zipped gzip -9"
done

if [[ ${1:-} == --test ]]; then
  (cd "$package" && bun test)
fi
