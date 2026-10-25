/** Formats a structure or drawing can be read from. `rsk` is a native
 * ReShiki drawing, `cdx` the binary ChemDraw file, `rsmi` reaction SMILES;
 * `auto` tells the text formats apart. */
export type InputFormat =
  | "rsk"
  | "mol"
  | "smiles"
  | "inchi"
  | "cdxml"
  | "cdx"
  | "rxn"
  | "rsmi"
  | "auto";

/** Outputs returned as bytes. */
export type BinaryFormat = "png" | "pdf" | "cdx";
/** Outputs returned as text. */
export type TextFormat = "rsk" | "svg" | "mol" | "smiles" | "inchi" | "cdxml" | "rxn" | "rsmi";
export type OutputFormat = BinaryFormat | TextFormat;

/** Text is read as UTF-8; `cdx` input must be bytes. */
export type Input = string | Uint8Array;

export interface PngOptions {
  /** Default 1600; 1 to 8192. */
  maxWidth?: number;
  /** Default 1000; 1 to 8192. */
  maxHeight?: number;
}

/** Load the WebAssembly module. Every function calls it, so calling it first
 * is needed only to load from somewhere other than next to this package's
 * `reshiki_web_bg.wasm`, or from bytes or a compiled module. */
export function init(
  module?: string | URL | Request | Response | BufferSource | WebAssembly.Module,
): Promise<void>;

/** Convert between formats. svg, png and pdf are figures as the desktop's
 * Export makes them. Rejects with an Error carrying the app's message. */
export function convert(data: Input, from: InputFormat, to: BinaryFormat): Promise<Uint8Array>;
export function convert(data: Input, from: InputFormat, to: TextFormat): Promise<string>;
export function convert(
  data: Input,
  from: InputFormat,
  to: OutputFormat,
): Promise<string | Uint8Array>;

/** The editor's SVG preview, with a transparent surround. */
export function renderSvg(data: Input, format: InputFormat): Promise<string>;

/** The editor's PNG preview on white, scaled up to 3x and down to fit. */
export function renderPng(
  data: Input,
  format: InputFormat,
  options?: PngOptions,
): Promise<Uint8Array>;

/** The canonical SMILES. */
export function smiles(data: Input, format: InputFormat): Promise<string>;

/** The standard InChI. */
export function inchi(data: Input, format: InputFormat): Promise<string>;

/** Add a TrueType or OpenType font (or collection), from a URL or bytes, for
 * figures and previews; text whose font lacks a glyph, such as Japanese,
 * falls back to it. Resolves to the number of faces added. */
export function addFont(font: string | URL | Uint8Array | ArrayBuffer): Promise<number>;

/** The ReShiki release version, shared with the application and npm package. */
export function version(): Promise<string>;
