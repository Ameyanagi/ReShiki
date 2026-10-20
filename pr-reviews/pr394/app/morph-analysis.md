# Morph capture evidence

The 40 captures show several intermediate spatial states during the 2D-to-3D transition. They support an animated geometry morph rather than a single jump directly to the final conformer. The rendering style changes to ball-and-stick before the spatial morph is complete; the captures do not show a gradual fade of the original 2D letters and bond strokes into spheres.

| Frames | Logged epoch ms / local time (JST, 2026-10-20) | Visible evidence |
| --- | --- | --- |
| f01–f31 | 1792492994240–1792492997893 / 19:43:14.240–17.893 | Unchanged selected 2D drawing. Every PNG has the same SHA-256 as `01-selected-2d.png`: `7cbde000c1adb13e17112e31f7b7002309edd47ee50952d670fd59388436d489`. |
| f32 | 1792492998023 / 19:43:18.023 | 3D is selected, but the canvas still contains the 2D drawing. The inspector says “Generating the conformer · ETKDG, then MMFF94s”. |
| f33 | 1792492998157 / 19:43:18.157 | First ball-and-stick frame. Heavy-atom centers remain at the original drawing positions, and explicit hydrogen spheres are absent. This is an initial spatial state, distinct from the endpoint. |
| f34 | 1792492998289 / 19:43:18.289 | Heavy-atom positions have begun changing slightly. |
| f35 | 1792492998423 / 19:43:18.423 | Clear intermediate geometry, with pale, partially transparent hydrogens appearing near the methyl groups and right ring carbon. |
| f36–f38 | 1792492998555–1792492998814 / 19:43:18.555–18.814 | Hydrogens spread outward to visible bonded positions while the heavy-atom arrangement moves toward the final conformer. f37 and f38 are close to the endpoint but remain distinct PNGs. |
| f39–f40 | 1792492998943–1792492999065 / 19:43:18.943–19.065 | Stable final 3D state; these two PNGs are byte-identical, SHA-256 `de23c753e600ba675a5b53162bbbaf6a5f9103f0a4dbb1edc84add9239c738d5`. They visually agree with the final molecular arrangement in `02-3d-shown.png`; that separately captured PNG has a different file hash. |

The morph is visible in **f33–f38**, and the first stable endpoint is **f39**. The sampled initial-to-stable span is **786 ms** (f33 to f39), so approximately **0.8 seconds** is a reasonable description. The switch from 2D rendering to the initial ball-and-stick state occurs between f32 and f33, within **19:43:18.023–18.157 JST**. The last observed geometric difference resolves between f38 and f39, within **19:43:18.814–18.943 JST**. If these screenshots bracket the true onset and completion, the duration lies approximately between **657 and 920 ms**; the capture alone cannot establish exact animation start/end times.

Four useful evidence frames are **f32** (3D requested; 2D still visible), **f33** (initial ball-and-stick geometry), **f35** (unambiguous intermediate geometry and translucent hydrogens), and **f39** (stable final 3D). Comparing f33, f35, and f39 makes the progression particularly clear: the upper-right nitrogen moves down and left, the methyl carbons shift, and hydrogen spheres emerge and separate around them. `morph-raw/contact-sheet.png` provides an overview of all 40 captures.

Caveats: the log records screenshot times, not animation clock values or render-frame timestamps. Sampling gaps are 116–134 ms (median 122 ms), about eight captures per second; these images prove sampled intermediate states, not uninterrupted frame-by-frame smoothness. The elapsed capture is 4.825 seconds, most of which precedes the mode switch. Only the forward 2D-to-3D transition is covered. No conclusion about reverse morphing, atom-ID preservation, chemical correctness, or undo behavior follows from this visual sequence alone.
