// <reshiki-editor>: the editor as a custom element. Importing this module
// defines it. The element fills its own box, so give it a size.
//
//   <reshiki-editor src="caffeine.mol" format="mol" style="height: 600px">
//
// `fonts` lists font URLs separated by spaces, and `cjk-font` is the CJK
// fallback's URL, or "false" for none. It fires `ready` once the editor runs,
// `change` after each edit and `error` for a font that could not load, and has
// the editor's load, save and exportImage methods. Changing `src` or `format`
// afterwards loads the drawing again, after a cancelable `beforeload`.
import { mount } from "./editor.js";

export class ReshikiEditor extends HTMLElement {
  static observedAttributes = ["src", "format"];

  #editor;
  // Counts attribute loads, so a slow fetch never replaces a later one.
  #loads = 0;
  #changed = false;

  connectedCallback() {
    if (this.#editor) return;
    if (!this.style.display) this.style.display = "block";
    this.#editor = this.#start();
    this.#editor.catch((error) => this.#error(error));
  }

  #error(error) {
    this.dispatchEvent(new ErrorEvent("error", { error, message: error.message }));
  }

  // A move disconnects and reconnects the element at once; only stop the
  // editor if it is still out of the page afterwards.
  disconnectedCallback() {
    this.#editor
      ?.then((editor) => {
        if (!this.isConnected) editor.destroy();
      })
      .catch(() => {});
  }

  // The browser also reports the attributes the element starts with; those
  // are read by #start. `src` and `format` set together load once.
  attributeChangedCallback(_name, before, now) {
    if (!this.#editor || before === now || this.#changed) return;
    this.#changed = true;
    queueMicrotask(() => {
      this.#changed = false;
      this.#reload().catch((error) => this.#error(error));
    });
  }

  async #reload() {
    const src = this.getAttribute("src");
    if (!src) return;
    const format = this.getAttribute("format") ?? "auto";
    const editor = await this.#editor;
    // load() replaces the drawing in front without asking, so the page gets
    // to keep one with unsaved changes.
    const asked = new CustomEvent("beforeload", {
      cancelable: true,
      detail: { src, format, dirty: editor.isDirty() },
    });
    if (!this.dispatchEvent(asked)) return;
    const load = ++this.#loads;
    const data = await this.#fetch(src);
    if (load === this.#loads) await editor.load(data, format);
  }

  async #fetch(src) {
    const response = await fetch(src);
    if (!response.ok) throw new Error(`Could not load ${src}: HTTP ${response.status}`);
    return new Uint8Array(await response.arrayBuffer());
  }

  async #start() {
    const src = this.getAttribute("src");
    const cjkFont = this.getAttribute("cjk-font");
    const editor = await mount(this, {
      data: src ? await this.#fetch(src) : undefined,
      format: this.getAttribute("format") ?? "auto",
      fonts: this.getAttribute("fonts")?.split(/\s+/).filter(Boolean),
      cjkFont: cjkFont === "false" ? false : (cjkFont ?? undefined),
      onChange: () => this.dispatchEvent(new Event("change")),
      onError: (error) => this.#error(error),
    });
    this.dispatchEvent(new Event("ready"));
    return editor;
  }

  /** Resolves once the editor runs; rejects if it could not start. */
  get ready() {
    return this.#editor ?? Promise.reject(new Error("The element is not in a page"));
  }

  async load(data, format) {
    return (await this.ready).load(data, format);
  }

  async save(format) {
    return (await this.ready).save(format);
  }

  async exportImage(format) {
    return (await this.ready).exportImage(format);
  }
}

if (!customElements.get("reshiki-editor")) {
  customElements.define("reshiki-editor", ReshikiEditor);
}
