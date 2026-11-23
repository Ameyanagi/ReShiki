// Only failures before an operation is posted may fall back to the caller.
// A timed-out operation must never run twice.
const failed = (message) => Object.assign(new Error(message), { code: "failed" });

export function createFigureWorker({
  module,
  direct,
  fonts,
  createWorker,
  available = () => typeof Worker === "function",
  loadTimeout = 60_000,
  jobTimeout = 60_000,
  queueTimeout = 60_000,
  maxPending = 8,
}) {
  let worker;
  let ready;
  let active;
  let sentFonts = 0;
  let next = 0;
  let pumping = false;
  const queue = [];

  const stop = () => {
    if (worker) {
      worker.onmessage = worker.onerror = worker.onmessageerror = null;
      worker.terminate();
    }
    worker = ready = undefined;
    sentFonts = 0;
  };

  const start = () => {
    if (ready) return ready;
    ready = new Promise((resolve, reject) => {
      let loaded = false;
      const timer = setTimeout(
        () => reject(failed("The rendering worker did not load in time")),
        loadTimeout,
      );
      const startupFailed = (message) => {
        clearTimeout(timer);
        reject(failed(message));
      };
      try {
        worker = createWorker();
        // Node-compatible hosts may offer Worker too; an idle renderer need
        // not keep their process alive. Browsers have no unref method.
        worker.unref?.();
        worker.onmessage = ({ data }) => {
          if (!loaded) {
            if (data?.type === "ready") {
              loaded = true;
              clearTimeout(timer);
              resolve();
            } else
              startupFailed(data?.error?.message ?? "The rendering worker could not initialize");
            return;
          }
          if (!active || data?.id !== active.id) {
            active?.reject(failed("The rendering worker returned an unexpected response"));
            return;
          }
          if (data.type === "result" || data.type === "error") {
            sentFonts = data.fonts;
            if (data.type === "result") active.resolve(data.value);
            else {
              const error = new Error(data.error?.message ?? "Rendering failed");
              if (data.error?.code) error.code = data.error.code;
              active.reject(error, false);
            }
          } else active.reject(failed("The rendering worker returned an unexpected response"));
        };
        const broken = (event) => {
          event.preventDefault?.();
          const error = failed(event.message || "The rendering worker stopped");
          if (loaded) {
            if (active) active.reject(error);
            else stop();
          } else startupFailed(error.message);
        };
        worker.onerror = worker.onmessageerror = broken;
        worker.postMessage({ type: "init", module: module() });
      } catch (error) {
        startupFailed(`Could not start the rendering worker: ${error?.message ?? error}`);
      }
    });
    return ready;
  };

  const pump = async () => {
    if (pumping) return;
    pumping = true;
    try {
      while (queue.length) {
        const job = queue.shift();
        clearTimeout(job.wait);
        if (!available()) {
          try {
            job.resolve(await direct(job.operation, job.args));
          } catch (error) {
            job.reject(error);
          }
          continue;
        }
        try {
          await start();
        } catch {
          // Nothing was dispatched: retain compatibility with hosts that
          // cannot start module workers (including restrictive CSPs).
          stop();
          try {
            job.resolve(await direct(job.operation, job.args));
          } catch (error) {
            job.reject(error);
          }
          continue;
        }
        try {
          const value = await new Promise((resolve, reject) => {
            const timer = setTimeout(
              () => active?.reject(failed("Rendering reached its time limit")),
              jobTimeout,
            );
            active = {
              id: job.id,
              resolve(value) {
                clearTimeout(timer);
                active = undefined;
                resolve(value);
              },
              reject(error, transport = true) {
                clearTimeout(timer);
                active = undefined;
                if (transport) stop();
                reject(error);
              },
            };
            try {
              worker.postMessage({
                type: "run",
                id: job.id,
                operation: job.operation,
                args: job.args,
                fonts: fonts.slice(sentFonts, job.fonts),
              });
            } catch (error) {
              active.reject(
                failed(`Could not reach the rendering worker: ${error?.message ?? error}`),
              );
            }
          });
          job.resolve(value);
        } catch (error) {
          job.reject(error);
        }
      }
    } finally {
      pumping = false;
    }
  };

  return {
    run(operation, args) {
      if (queue.length + Number(pumping) >= maxPending) {
        return Promise.reject(failed("The rendering queue is full"));
      }
      // Queued jobs own their input. postMessage never transfers caller memory.
      const owned = [new Uint8Array(args[0]), ...args.slice(1)];
      return new Promise((resolve, reject) => {
        const job = { id: ++next, operation, args: owned, fonts: fonts.length, resolve, reject };
        job.wait = setTimeout(() => {
          const index = queue.indexOf(job);
          if (index >= 0) {
            queue.splice(index, 1);
            reject(failed("Rendering waited too long in the queue"));
          }
        }, queueTimeout);
        queue.push(job);
        void pump();
      });
    },
  };
}
