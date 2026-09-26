# OKLab conversion matrices

`src/color_contrast.rs` uses the linear-sRGB/OKLab conversion matrices from Björn Ottosson's [A perceptual color space for image processing](https://bottosson.github.io/posts/oklab/), matrix revision **2021-01-25**.

The source explicitly offers the conversion implementation in the **public domain**, with MIT as an alternative. ReShiki uses the public-domain option. The matrices were translated to Rust with f64 arithmetic; palette generation, constant-hue gamut reduction, quantized-RGB contrast solving and application roles are ReShiki code. No external color-library dependency was added.
