// <reshiki-editor>: the editor as a custom element. Importing this module
// defines it. The element fills its own box, so give it a size.
//
//   <reshiki-editor src="caffeine.mol" format="mol" style="height: 600px">
//
// `fonts` lists font URLs separated by spaces, and `cjk-font` is the CJK
// fallback's URL, or "false" for none. It fires `ready` once the editor runs,
// `change` after each edit and `error` for a font that could not load, and has
// the editor's load, save and exportImage methods.
import { mount } from "./editor.js";

export class ReshikiEditor extends HTMLElement {
  #editor;

  connectedCallback() {
    if (this.#editor) return;
    if (!this.style.display) this.style.display = "block";
    this.#editor = this.#start();
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

  async #start() {
    const src = this.getAttribute("src");
    const cjkFont = this.getAttribute("cjk-font");
    const data = src ? new Uint8Array(await (await fetch(src)).arrayBuffer()) : undefined;
    const editor = await mount(this, {
      data,
      format: this.getAttribute("format") ?? "auto",
      fonts: this.getAttribute("fonts")?.split(/\s+/).filter(Boolean),
      cjkFont: cjkFont === "false" ? false : (cjkFont ?? undefined),
      onChange: () => this.dispatchEvent(new Event("change")),
      onError: (error) =>
        this.dispatchEvent(new ErrorEvent("error", { error, message: error.message })),
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
