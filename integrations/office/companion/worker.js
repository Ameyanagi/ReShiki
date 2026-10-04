import { spawn } from "node:child_process";
import { LIMITS, createEnvelope } from "../protocol.js";

const OUTPUT_LIMIT = Math.ceil(((LIMITS.nativeBytes + LIMITS.pngBytes) * 4) / 3) + 4096;
export function runWorker(executable, mode, input = Buffer.alloc(0), options = {}) {
  if (!["--libreoffice-preview", "--libreoffice-clipboard", "--libreoffice-copy"].includes(mode))
    throw new Error("Unsupported worker mode");
  if (input.length > LIMITS.nativeBytes) throw new Error("Drawing exceeds native byte limit");
  return new Promise((resolve, reject) => {
    const child = spawn(executable, [mode], {
      shell: false,
      windowsHide: true,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let size = 0,
      errorSize = 0,
      finished = false;
    const output = [],
      errors = [];
    const finish = (error, value) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      if (error) reject(error);
      else resolve(value);
    };
    const timer = setTimeout(() => {
      child.kill();
      finish(new Error("ReShiki worker timed out"));
    }, options.timeoutMs ?? 15000);
    child.on("error", (error) => finish(error));
    child.stdout.on("data", (bytes) => {
      size += bytes.length;
      if (size > OUTPUT_LIMIT) {
        child.kill();
        finish(new Error("ReShiki worker output exceeded limit"));
      } else output.push(bytes);
    });
    child.stderr.on("data", (bytes) => {
      errorSize += bytes.length;
      if (errorSize <= 8192) errors.push(bytes);
    });
    child.stdin.on("error", (error) => {
      if (error.code !== "EPIPE") finish(error);
    });
    child.on("close", async (code) => {
      if (finished) return;
      if (code !== 0)
        return finish(
          new Error(
            `ReShiki worker failed: ${Buffer.concat(errors).toString("utf8").slice(0, 1024) || code}`,
          ),
        );
      try {
        const packet = JSON.parse(Buffer.concat(output).toString("utf8"));
        finish(null, mode === "--libreoffice-copy" ? {} : await createEnvelope(packet));
      } catch (error) {
        finish(error);
      }
    });
    child.stdin.end(input);
  });
}
