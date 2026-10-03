// Shared by Office's webview and the local companion. These are ReShiki limits,
// not claims about Microsoft's per-host capacity.
export const LIMITS = Object.freeze({ nativeBytes: 16 * 1024 * 1024, pngBytes: 8 * 1024 * 1024 });
export function decodeBase64(value, limit) {
  // A repeated four-character regex group exhausts V8's regex stack on valid
  // multi-megabyte drawings. Scan the alphabet and short padding separately.
  if (
    typeof value !== "string" ||
    !value.length ||
    value.length > Math.ceil(limit / 3) * 4 ||
    value.length % 4 ||
    /[^A-Za-z0-9+/=]/.test(value)
  )
    throw new Error("Invalid or oversized base64 drawing");
  const padding = value.indexOf("=");
  if (padding >= 0 && (padding < value.length - 2 || !/^={1,2}$/.test(value.slice(padding))))
    throw new Error("Invalid base64 padding");
  const raw = atob(value);
  if (raw.length > limit || btoa(raw) !== value)
    throw new Error("Noncanonical or oversized base64 drawing");
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}
export async function nativeSha256(native) {
  const digest = await crypto.subtle.digest("SHA-256", decodeBase64(native, LIMITS.nativeBytes));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
export function createObjectId() {
  return crypto.randomUUID();
}
export async function createEnvelope(packet) {
  return validateEnvelope({ ...packet, revision: await nativeSha256(packet.native) });
}
export async function validateEnvelope(value) {
  if (
    !value ||
    Object.getPrototypeOf(value) !== Object.prototype ||
    Object.keys(value).sort().join(",") !== "extent,native,png,revision,version" ||
    value.version !== 1
  )
    throw new Error("Unsupported ReShiki envelope");
  if (
    !Array.isArray(value.extent) ||
    value.extent.length !== 2 ||
    !value.extent.every((n) => Number.isInteger(n) && n > 0 && n <= 2147483647)
  )
    throw new Error("Invalid drawing extent");
  const png = decodeBase64(value.png, LIMITS.pngBytes);
  const signature = [137, 80, 78, 71, 13, 10, 26, 10];
  if (
    png.length < 33 ||
    !signature.every((n, i) => png[i] === n) ||
    String.fromCharCode(...png.slice(12, 16)) !== "IHDR"
  )
    throw new Error("Invalid PNG preview");
  const view = new DataView(png.buffer, png.byteOffset, png.byteLength);
  const width = view.getUint32(16),
    height = view.getUint32(20);
  if (!width || !height || width > 16384 || height > 16384 || width * height > 32 * 1024 * 1024)
    throw new Error("PNG dimensions exceed preview limits");
  if (
    !/^[a-f0-9]{64}$/.test(value.revision) ||
    (await nativeSha256(value.native)) !== value.revision
  )
    throw new Error("Drawing revision does not match native bytes");
  return {
    version: 1,
    native: value.native,
    png: value.png,
    extent: [...value.extent],
    revision: value.revision,
  };
}
