/** Formats a structure or drawing can be read from. `rsk` (also `reshiki`,
 * the name the MCP tools and the CLI use) is a native ReShiki drawing, `cdx`
 * the binary ChemDraw file, `rsmi` reaction SMILES; `auto` tells the text
 * formats apart. */
export type InputFormat =
  "rsk" | "reshiki" | "mol" | "smiles" | "inchi" | "cdxml" | "cdx" | "rxn" | "rsmi" | "auto";

/** Outputs returned as bytes. */
export type BinaryFormat = "png" | "pdf" | "cdx";
/** Outputs returned as text. */
export type TextFormat =
  "rsk" | "reshiki" | "svg" | "mol" | "smiles" | "inchi" | "cdxml" | "rxn" | "rsmi";
export type OutputFormat = BinaryFormat | TextFormat;

/** The `code` of every Error these functions reject with, as the MCP tools
 * and the CLI report the same failure: `invalid_arguments` for a format or
 * option or nucleus that is not accepted, `failed` when the structure cannot be read,
 * drawn or written. */
export type ErrorCode = "invalid_arguments" | "failed";

/** What every function here rejects with. */
export interface ReShikiError extends Error {
  code: ErrorCode;
}

export interface SvgOptions {
  /** `outline` (the default) turns SVG text into paths, for the same
   * appearance in every viewer; `live` keeps text elements. The MCP tools,
   * the CLI and Python name this option `svg_text`. Other formats ignore it. */
  svgText?: "outline" | "live";
}

/** Text is read as UTF-8; `cdx` input must be bytes. */
export type Input = string | Uint8Array;

export interface ReadResult {
  /** The native ReShiki drawing (rsk), for convert(drawing, "rsk", to). */
  drawing: string;
  /** Import warnings, in order; empty when nothing was changed or assumed. */
  warnings: string[];
}

export interface PngOptions {
  /** Default 1600; 1 to 8192. */
  maxWidth?: number;
  /** Default 1000; 1 to 8192. */
  maxHeight?: number;
}

export type NmrNucleus = "1H" | "13C";

/** One carbon site (13C), or the H attached to one heavy atom (1H).
 * Sites without a prediction retain their row, null values and a note. */
export interface NmrShift {
  /** Decimal atom ID; for 1H, the heavy atom the H are attached to. */
  atom: string;
  nucleus: NmrNucleus;
  /** Median chemical shift in ppm. */
  ppm: number | null;
  /** Sphere radius of the reference match. */
  shell: number | null;
  references: number | null;
  sd: number | null;
  min: number | null;
  max: number | null;
  broad: boolean;
  /** Attached H count for 1H; null for 13C. */
  hydrogen_count: number | null;
  /** Decimal IDs of attached H drawn as atoms; implicit H have none. */
  hydrogens: string[];
  note: string | null;
}

/** The CLI's NMR result without its session revision. */
export interface NmrPrediction {
  nucleus: NmrNucleus;
  solvent: string;
  rows: NmrShift[];
  method: string;
  data_version: string;
  conditions: string;
  limitations: string;
  /** Cite this nmrshiftdb2 attribution with published values. */
  credit: string;
  data_access: string;
}

/** Load the WebAssembly module. Every function calls it, so calling it first
 * is needed only to load from somewhere other than next to this package's
 * `reshiki_web_bg.wasm`, or from bytes or a compiled module. */
export function init(
  module?: string | URL | Request | Response | BufferSource | WebAssembly.Module,
): Promise<void>;

/** Read a drawing or structure, returning its native drawing and any import
 * warnings. Rejects with a ReShikiError carrying the app's message when
 * unreadable. */
export function read(data: Input, format: InputFormat): Promise<ReadResult>;

/** Convert between formats. svg, png and pdf are figures as the desktop's
 * Export makes them; SVG text is outlined unless `svgText` is `live`.
 * Rejects with a ReShikiError carrying the app's message. */
export function convert(
  data: Input,
  from: InputFormat,
  to: BinaryFormat,
  options?: SvgOptions,
): Promise<Uint8Array>;
export function convert(
  data: Input,
  from: InputFormat,
  to: TextFormat,
  options?: SvgOptions,
): Promise<string>;
export function convert(
  data: Input,
  from: InputFormat,
  to: OutputFormat,
  options?: SvgOptions,
): Promise<string | Uint8Array>;

/** The editor's SVG preview, with a transparent surround; text is outlined
 * unless `svgText` is `live`. */
export function renderSvg(data: Input, format: InputFormat, options?: SvgOptions): Promise<string>;

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

/** Predict offline 1H or 13C chemical shifts for exactly one whole molecule.
 * Cite the result's credit with published values. Rejects with
 * invalid_arguments for another nucleus, or failed for unsupported chemistry,
 * more than one molecule or no sites for the requested nucleus. */
export function predictNmr(
  data: Input,
  format: InputFormat,
  nucleus: NmrNucleus,
): Promise<NmrPrediction>;

/** Add a TrueType or OpenType font (or collection), from a URL or bytes, for
 * figures and previews; text whose font lacks a glyph, such as Japanese,
 * falls back to it. Resolves to the number of faces added. */
export function addFont(font: string | URL | Uint8Array | ArrayBuffer): Promise<number>;

/** The ReShiki release version, shared with the application and npm package. */
export function version(): Promise<string>;
