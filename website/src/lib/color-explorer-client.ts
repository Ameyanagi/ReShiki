import { catalog, hex } from "./color-reference";
import {
  inspector,
  paletteTiles,
  supportingTiles,
  type Selection,
  type Mode,
} from "./color-explorer";

export function initExplorer() {
  const root = document.querySelector<HTMLDivElement>("#color-reference");
  if (!root) return;
  const elements = root.querySelector<HTMLDivElement>("#cr-elements")!;
  const search = root.querySelector<HTMLInputElement>("#cr-search")!;
  const status = root.querySelector<HTMLParagraphElement>("#cr-copy-status")!;
  const menu = root.querySelector<HTMLDetailsElement>(".cr-download-menu")!;
  let theme = catalog.themes[0];
  let mode: Mode = "light";
  let selected: Selection = { kind: "element", index: 6 };
  let copyTimer: ReturnType<typeof setTimeout>;

  function setTabStop(preferred?: HTMLButtonElement) {
    const buttons = [...elements.querySelectorAll<HTMLButtonElement>(".cr-tile")];
    const enabled = buttons.filter((button) => !button.disabled);
    const stop =
      (preferred && enabled.includes(preferred) ? preferred : undefined) ||
      enabled.find((button) => button.tabIndex === 0) ||
      enabled.find((button) => button.getAttribute("aria-pressed") === "true") ||
      enabled[0];
    for (const button of buttons) button.tabIndex = button === stop ? 0 : -1;
  }

  function filter() {
    const query = search.value.trim().toLowerCase();
    let count = 0;
    for (const tile of elements.querySelectorAll<HTMLButtonElement>(".cr-tile")) {
      const matches =
        !query ||
        (/^\d+$/.test(query)
          ? tile.dataset.number === String(Number(query))
          : tile.dataset.symbol!.startsWith(query));
      tile.disabled = !matches;
      if (matches) count++;
    }
    setTabStop();
    root!.querySelector("#cr-count")!.textContent = count
      ? `${count} element${count === 1 ? "" : "s"}${query ? " found" : ""}`
      : "No matches. Try N or 7.";
  }

  function showSelection(scroll = false) {
    root!.querySelector("#cr-inspector-content")!.innerHTML = inspector(theme, selected, mode);
    for (const tile of elements.querySelectorAll<HTMLButtonElement>(".cr-tile")) {
      tile.setAttribute(
        "aria-pressed",
        String(selected.kind === "element" && Number(tile.dataset.element) === selected.index),
      );
    }
    for (const tile of root!.querySelectorAll<HTMLButtonElement>("[data-support]")) {
      tile.setAttribute(
        "aria-pressed",
        String(selected.kind === "role" && tile.dataset.support === selected.name),
      );
    }
    if (scroll && window.matchMedia("(max-width: 1050px)").matches) {
      root!.querySelector("#cr-inspector")!.scrollIntoView({
        block: "start",
        behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches
          ? "instant"
          : "smooth",
      });
    }
  }

  function update() {
    root!.dataset.canvasMode = mode;
    root!.querySelector("#cr-description")!.textContent =
      `Colors as they appear on ${mode === "light" ? "white" : "black"} paper.`;
    root!.querySelector("#cr-support-colors")!.innerHTML = supportingTiles(theme, mode);
    for (const button of root!.querySelectorAll<HTMLButtonElement>("button[data-canvas-mode]")) {
      button.setAttribute("aria-pressed", String(button.dataset.canvasMode === mode));
    }
    for (const card of root!.querySelectorAll<HTMLButtonElement>("[data-theme-id]")) {
      const palette = catalog.themes.find((item) => item.id === card.dataset.themeId)!;
      card.querySelectorAll<HTMLElement>(".cr-theme-strip > span").forEach((swatch, index) => {
        swatch.style.background = hex(
          palette.modes[mode].elements[[6, 7, 14, 15, 16, 52][index]].label,
        );
      });
    }
    elements.innerHTML = paletteTiles(theme, selected, mode);
    for (const button of root!.querySelectorAll("[data-theme-id]"))
      button.setAttribute(
        "aria-pressed",
        String((button as HTMLElement).dataset.themeId === theme.id),
      );
    const native = root!.querySelector<HTMLAnchorElement>("#cr-native")!;
    native.href = `/colors/${theme.id}.reshiki-theme`;
    native.querySelector("small")!.textContent = `${theme.name} · both modes`;
    filter();
    showSelection();
  }

  root.addEventListener("click", async (event) => {
    const target = event.target as Element;
    const button = target.closest<HTMLButtonElement>("button");
    if (!button) return;
    if (button.dataset.themeId) {
      theme = catalog.themes.find((item) => item.id === button.dataset.themeId)!;
      update();
    } else if (button.dataset.canvasMode) {
      mode = button.dataset.canvasMode as Mode;
      update();
    } else if (button.dataset.element) {
      selected = { kind: "element", index: Number(button.dataset.element) };
      showSelection(true);
    } else if (button.dataset.support) {
      selected = { kind: "role", name: button.dataset.support };
      showSelection(true);
    } else if (button.dataset.copy) {
      try {
        await navigator.clipboard.writeText(button.dataset.copy);
        status.textContent = `Copied ${button.dataset.copy}`;
      } catch {
        status.textContent = "Copy unavailable. Select the value and copy it manually.";
      }
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        status.textContent = "";
      }, 4000);
    }
  });
  search.addEventListener("input", filter);
  search.addEventListener("keydown", (event) => {
    if (event.key === "Enter")
      elements.querySelector<HTMLButtonElement>("button:not(:disabled)")?.click();
    if (event.key === "Escape") {
      search.value = "";
      filter();
    }
  });
  // Move through the palette without tabbing through all 118 tiles.
  elements.addEventListener("focusin", (event) => {
    if (event.target instanceof HTMLButtonElement) setTabStop(event.target);
  });
  elements.addEventListener("keydown", (event) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const buttons = [...elements.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const current = buttons.indexOf(event.target as HTMLButtonElement);
    if (current < 0) return;
    event.preventDefault();
    const index =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? buttons.length - 1
          : (current + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) % buttons.length;
    buttons[index]?.focus();
  });
  document.addEventListener("click", (event) => {
    if (!menu.contains(event.target as Node)) menu.open = false;
  });
  menu.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      menu.open = false;
      menu.querySelector("summary")?.focus();
    }
  });
}
