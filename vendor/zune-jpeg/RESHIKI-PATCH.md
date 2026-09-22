# ReShiki JPEG sampling patch

This directory retains the published `zune-jpeg` 0.5.15 source and its MIT,
Apache-2.0, and Zlib licenses. The registry package checksum is
`27bc9d5b815bc103f142aa054f561d9187d191692ec7c2d1e2b4737f8dbd7296`;
upstream source commit `31d81fed7551c8ccea456d9d8e2b1fd8bebb6995`,
`crates/zune-jpeg`. Registry bookkeeping, lockfile, changelog and benchmark notes
are omitted. `Cargo.toml`, `Cargo.toml.orig`, `README.md` and `.cargo_vcs_info.json` have formatting-only changes required by the repository hooks. Trailing whitespace was removed from `src/color_convert/avx.rs`, `src/decoder.rs` and `src/upsampler/portable_simd.rs`. The application applies it through Cargo's `[patch.crates-io]`.

The sampling and integer precision are checked against Pillow 12.3.0 with
libjpeg-turbo 3.1.4.1. Reference algorithms are in that tag's
[`src/jdsample.c`](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/3.1.4.1/src/jdsample.c)
and [`src/jidctint.c`](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/3.1.4.1/src/jidctint.c).
Their distribution terms are retained in `LICENSE-IJG`.

Changes from the registry source:

- `mcu.rs::post_process` supplies real image dimensions to `worker.rs::upsample`.
- `worker.rs::upsample` uses `upsampler/compatible.rs::sample` for horizontal,
  vertical and combined 2:1 sampling. The checked scalar filter uses real
  component edges, nearest-neighbor sampling when horizontal component width
  is at most two, and libjpeg's alternating rounding biases. Combined filtering
  rounds once. It shares the same implementation across CPU dispatch modes.
  Other sampling ratios retain the upstream implementation.
- `upsampler.rs` declares the additional private module.
- `idct/{scalar,avx2,neon}.rs` use libjpeg's 13-bit integer constants and shifts,
  including the specialized 4x4 path. The final rounding constant omits the
  upstream extra first-pass bias. DC-only processing and SIMD dispatch stay
  unchanged. No unsafe blocks or unsafe operations were added.

No public API, feature defaults, dependency versions, allocation strategy,
entropy decoding, metadata, orientation, or color conversion was changed.
`tests/jpeg_sampling.rs` compares baseline and progressive JPEGs at 4:4:4,
4:2:2 and 4:2:0, small/odd/MCU-boundary sizes, high-contrast/noise/gradient pixels,
EXIF orientation, opacity, ordinary picture imports and scalar/SIMD decoding.
The 2-level RGB allowance is unchanged; alpha and lossless formats remain exact.
