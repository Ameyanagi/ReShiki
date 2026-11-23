// A custom editor module has no verified matching bundled renderer. Keep its
// figure work local while retaining the existing naming worker configuration.
export function createInitializer({ load, configureWorker, worker, available }) {
  let loading;
  let renderBundled;
  return function initEditor(module) {
    loading ??= load(module === undefined ? undefined : { module_or_path: module })
      .then(() => {
        // wasm-bindgen retains the first loaded module even if later worker
        // configuration fails. A retry must retain that module's eligibility.
        renderBundled ??= module === undefined;
        if (available()) configureWorker(worker, renderBundled);
      })
      .catch((error) => {
        loading = undefined;
        throw error;
      });
    return loading.then(() => undefined);
  };
}
