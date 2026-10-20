// The editor in an iframe, for style isolation or several editors on a page.
// The frame page (frame.html) runs the editor; this side forwards calls to it
// with postMessage and returns the same editor handle as mount().
const FRAME = new URL("./frame.html", import.meta.url);

export function embed(element, options = {}) {
  const frameUrl = new URL(options.frameUrl ?? FRAME, location.href);
  const frame = document.createElement("iframe");
  frame.src = frameUrl.href;
  frame.title = options.title ?? "ReShiki editor";
  frame.style.cssText = "display: block; width: 100%; height: 100%; border: 0";
  const pending = new Map();
  let next = 0;
  let listeners = options.onChange ? [options.onChange] : [];
  let started;
  let removed = false;
  const ready = new Promise((resolve, reject) => (started = { resolve, reject }));
  // Calls see a failed start; it is not unhandled when nothing else waits.
  ready.catch(() => {});
  // A frame whose page or scripts fail to load never reports, so stop waiting.
  const timer = setTimeout(
    () => started.reject(new Error("The editor frame did not start")),
    options.timeout ?? 60_000,
  );
  ready.finally(() => clearTimeout(timer)).catch(() => {});

  const receive = ({ source, origin, data }) => {
    if (source !== frame.contentWindow || origin !== frameUrl.origin) return;
    if (data?.event === "ready") started.resolve();
    else if (data?.event === "error") started.reject(new Error(data.error));
    else if (data?.event === "change") listeners.forEach((listener) => listener());
    else if (pending.has(data?.id)) {
      const { resolve, reject } = pending.get(data.id);
      pending.delete(data.id);
      if ("error" in data) reject(new Error(data.error));
      else resolve(data.result);
    }
  };
  addEventListener("message", receive);
  element.append(frame);

  const call = async (method, ...args) => {
    await ready;
    if (removed) throw new Error("The editor was removed");
    const id = ++next;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      frame.contentWindow.postMessage({ id, method, args }, frameUrl.origin);
    });
  };

  return {
    ready,
    load: (data, format = "auto") => call("load", data, format),
    save: (format = "rsk") => call("save", format),
    exportImage: (format = "svg") => call("exportImage", format),
    onChange(callback) {
      listeners = [...listeners, callback];
      return () => {
        listeners = listeners.filter((listener) => listener !== callback);
      };
    },
    destroy() {
      removed = true;
      started.reject(new Error("The editor was removed"));
      removeEventListener("message", receive);
      pending.forEach(({ reject }) => reject(new Error("The editor was removed")));
      pending.clear();
      listeners = [];
      frame.remove();
    },
  };
}
