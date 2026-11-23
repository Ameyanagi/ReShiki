// Independent of generated glue so this protocol can be tested without Wasm.
export function createWorkerHandler(load, wasm, post) {
  let loading;
  let fonts = 0;
  const error = (value) => ({
    message: String(value?.message ?? value),
    ...(value?.code ? { code: value.code } : {}),
  });
  return async ({ data }) => {
    if (data?.type === "init") {
      try {
        if (!data.module) throw new Error("The rendering worker requires the initialized module");
        loading ??= load({ module_or_path: data.module });
        await loading;
        post({ type: "ready" });
      } catch (failure) {
        post({ type: "startup-error", error: error(failure) });
      }
      return;
    }
    if (data?.type !== "run") return;
    try {
      if (!loading) throw new Error("The rendering worker is not initialized");
      await loading;
      for (const font of data.fonts) {
        wasm.addFont(font);
        fonts += 1;
      }
      if (!["convert", "renderSvg", "renderPng"].includes(data.operation)) {
        throw new Error(`Unknown rendering operation ${data.operation}`);
      }
      const output = await wasm[data.operation](...data.args);
      // Transfer an owned buffer, never a Wasm memory view.
      const value = output instanceof Uint8Array ? new Uint8Array(output) : output;
      post(
        { type: "result", id: data.id, value, fonts },
        value instanceof Uint8Array ? [value.buffer] : [],
      );
    } catch (failure) {
      post({ type: "error", id: data.id, error: error(failure), fonts });
    }
  };
}
