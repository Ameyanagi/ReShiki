// Format and size exports from the approved PNG masters. Run on macOS with Bun or Node.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin")
  throw new Error("This export script requires macOS sips and iconutil.");
const root = fileURLToPath(new URL("../", import.meta.url));
const brand = join(root, "assets/branding");
const website = join(root, "website/public");
const temporary = mkdtempSync(join(tmpdir(), "reshiki-icons-"));
const sizes = [16, 24, 32, 48, 64, 128, 256, 512, 1024];

function resize(source, destination, width, height = width) {
  mkdirSync(dirname(destination), { recursive: true });
  execFileSync("sips", ["-z", String(height), String(width), source, "--out", destination], {
    stdio: "pipe",
  });
}

// Modern Windows accepts PNG payloads in ICO directories, preserving the generated alpha.
function ico(files, destination) {
  const images = files.map(({ size, path }) => ({ size, data: readFileSync(path) }));
  const directory = Buffer.alloc(6 + images.length * 16);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(images.length, 4);
  let offset = directory.length;
  images.forEach(({ size, data }, index) => {
    const entry = 6 + index * 16;
    directory.writeUInt8(size === 256 ? 0 : size, entry);
    directory.writeUInt8(size === 256 ? 0 : size, entry + 1);
    directory.writeUInt16LE(1, entry + 4);
    directory.writeUInt16LE(32, entry + 6);
    directory.writeUInt32LE(data.length, entry + 8);
    directory.writeUInt32LE(offset, entry + 12);
    offset += data.length;
  });
  writeFileSync(destination, Buffer.concat([directory, ...images.map(({ data }) => data)]));
}

try {
  for (const mode of ["light", "dark"]) {
    const master = join(brand, `app-icon-${mode}.png`);
    const exports = join(brand, "exports", mode);
    for (const size of sizes) resize(master, join(exports, `icon-${size}.png`), size);
    const iconset = join(temporary, `${mode}.iconset`);
    mkdirSync(iconset);
    for (const size of [16, 32, 128, 256, 512]) {
      for (const scale of [1, 2]) {
        const suffix = scale === 2 ? "@2x" : "";
        copyFileSync(
          join(exports, `icon-${size * scale}.png`),
          join(iconset, `icon_${size}x${size}${suffix}.png`),
        );
      }
    }
    const basename = mode === "light" ? "reshiki" : "reshiki-dark";
    execFileSync("iconutil", ["-c", "icns", iconset, "-o", join(brand, `${basename}.icns`)]);
    ico(
      sizes
        .filter((size) => size <= 256)
        .map((size) => ({ size, path: join(exports, `icon-${size}.png`) })),
      join(brand, `${basename}.ico`),
    );
    resize(join(brand, `promotion-${mode}.png`), join(exports, "social-1200x630.png"), 1200, 630);
    resize(join(brand, `mark-${mode}.png`), join(website, `brand/mark-${mode}.png`), 128);
  }
  resize(join(brand, "mark-light.png"), join(brand, "runtime/mark.png"), 128);
  copyFileSync(join(brand, "exports/light/icon-256.png"), join(brand, "runtime/icon.png"));
  copyFileSync(join(brand, "exports/light/icon-64.png"), join(website, "favicon.png"));
  resize(join(brand, "app-icon-light.png"), join(website, "apple-touch-icon.png"), 180);
  copyFileSync(
    join(brand, "exports/light/social-1200x630.png"),
    join(website, "brand/promotion-light.png"),
  );
  console.log("Exported light/dark PNGs, ICNS, ICO, desktop resources, and website assets.");
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
