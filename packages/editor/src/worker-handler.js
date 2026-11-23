// Both naming and rendering keep their existing byte protocols. A ready
// handshake precedes work; failed initialization never dispatches an operation.
export function serveWorker(scope, { load, run }) {
  const message = (error) => String(error?.message ?? error);
  const loading = Promise.resolve()
    .then(load)
    .then(
      () => {
        scope.postMessage(null);
        return true;
      },
      (error) => {
        scope.postMessage(message(error));
        return false;
      },
    );
  let queue = loading;
  scope.onmessage = ({ data: [job, request] }) => {
    queue = queue.then(async (loaded) => {
      if (!loaded) return false;
      try {
        const reply = await run(job, request);
        scope.postMessage(reply, [reply.buffer]);
      } catch (error) {
        scope.postMessage(message(error));
      }
      return true;
    });
    return queue;
  };
  return loading;
}
