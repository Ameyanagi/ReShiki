// Serve this package for the demo pages: `bun examples/serve.ts`, then open
// http://localhost:8080/ for the editor alone, /examples/api.html for the
// JavaScript API demo or /examples/iframe.html for two embedded editors.
// Set PORT to use another port.
import { normalize, resolve, sep } from "node:path";

const root = resolve(import.meta.dir, "..");
const port = Number(process.env.PORT ?? 8080);

Bun.serve({
  port,
  async fetch(request) {
    let path = decodeURIComponent(new URL(request.url).pathname);
    if (path === "/") return Response.redirect(new URL("/examples/", request.url), 302);
    if (path.endsWith("/")) path += "index.html";
    const file = resolve(root, normalize(`.${path}`));
    if (file !== root && !file.startsWith(root + sep)) {
      return new Response("Not found", { status: 404 });
    }
    const body = Bun.file(file);
    return (await body.exists()) ? new Response(body) : new Response("Not found", { status: 404 });
  },
});
console.log(`ReShiki editor: http://localhost:${port}/`);
console.log(`JavaScript API demo: http://localhost:${port}/examples/api.html`);
