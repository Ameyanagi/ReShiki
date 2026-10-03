#!/usr/bin/env node
import https from "node:https";
import { randomBytes, timingSafeEqual } from "node:crypto";
import { readFile, mkdir, realpath, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Sessions } from "./sessions.js";
import { runWorker } from "./worker.js";
import { LIMITS, decodeBase64, validateEnvelope } from "../protocol.js";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const BODY_LIMIT = Math.ceil(((LIMITS.nativeBytes + LIMITS.pngBytes) * 4) / 3) + 16384;
function forbidden(message) {
  return Object.assign(new Error(message), { status: 403 });
}
function secureEqual(a, b) {
  return (
    typeof a === "string" &&
    Buffer.byteLength(a) === Buffer.byteLength(b) &&
    timingSafeEqual(Buffer.from(a), Buffer.from(b))
  );
}
async function jsonBody(req) {
  if (req.headers["content-type"] !== "application/json")
    throw Object.assign(new Error("Expected application/json"), { status: 415 });
  if (Number(req.headers["content-length"]) > BODY_LIMIT)
    throw Object.assign(new Error("Request exceeds limit"), { status: 413 });
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > BODY_LIMIT) throw Object.assign(new Error("Request exceeds limit"), { status: 413 });
    chunks.push(chunk);
  }
  const value = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  if (!value || Array.isArray(value) || typeof value !== "object")
    throw new Error("Expected a JSON object");
  return value;
}
export function makeHandler({ origin, sessions, worker, token = randomBytes(32).toString("hex") }) {
  const hostname = new URL(origin).host;
  let inflight = 0;
  return async (req, res) => {
    res.setHeader("Cache-Control", "no-store");
    res.setHeader("X-Content-Type-Options", "nosniff");
    res.setHeader("Cross-Origin-Resource-Policy", "same-origin");
    res.setHeader("Referrer-Policy", "no-referrer");
    res.setHeader(
      "Content-Security-Policy",
      "default-src 'self'; script-src 'self' https://appsforoffice.microsoft.com; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'",
    );
    const send = (status, data, contentType = "application/json; charset=utf-8") => {
      res.writeHead(status, { "Content-Type": contentType });
      res.end(typeof data === "string" || Buffer.isBuffer(data) ? data : JSON.stringify(data));
    };
    let counted = false;
    try {
      if (req.headers.host !== hostname) throw forbidden("Invalid localhost Host");
      const url = new URL(req.url, origin);
      if (url.origin !== origin || url.search) throw forbidden("Invalid request URL");
      if (req.method === "GET" && url.pathname === "/bootstrap.json") {
        if (
          req.headers["x-reshiki-bootstrap"] !== "1" ||
          (req.headers.origin && req.headers.origin !== origin)
        )
          throw forbidden("Bootstrap requires a same-origin fetch");
        if (req.headers["sec-fetch-site"] && req.headers["sec-fetch-site"] !== "same-origin")
          throw forbidden("Bootstrap requires same origin");
        return send(200, { token });
      }
      if (url.pathname.startsWith("/api/")) {
        if (req.method !== "POST") return send(405, { error: "Use POST" });
        if (req.headers.origin !== origin || !secureEqual(req.headers["x-reshiki-token"], token))
          throw forbidden("Invalid origin or task pane token");
        if (inflight >= 4) return send(429, { error: "Companion is busy; retry shortly" });
        inflight++;
        counted = true;
        const body = await jsonBody(req);
        if (url.pathname === "/api/client")
          return send(200, { clientId: sessions.createClient(body.host) });
        sessions.touch(body.clientId);
        switch (url.pathname) {
          case "/api/heartbeat":
            return send(200, { active: true });
          case "/api/preview": {
            const envelope = await worker(
              "--libreoffice-preview",
              Buffer.from(decodeBase64(body.native, LIMITS.nativeBytes)),
            );
            return send(200, { envelope });
          }
          case "/api/clipboard/read":
            return send(200, { envelope: await worker("--libreoffice-clipboard") });
          case "/api/clipboard/write": {
            const envelope = await validateEnvelope(body.envelope);
            await worker(
              "--libreoffice-copy",
              Buffer.from(decodeBase64(envelope.native, LIMITS.nativeBytes)),
            );
            return send(200, { copied: true });
          }
          case "/api/edit":
            return send(200, await sessions.start(body.clientId, body.target, body.envelope));
          case "/api/poll":
            return send(200, await sessions.poll(body.sessionId, body.clientId));
          case "/api/ack":
            return send(200, await sessions.acknowledge(body.sessionId, body.clientId, body));
          case "/api/fail":
            return send(200, await sessions.fail(body.sessionId, body.clientId, body));
          case "/api/finish":
            return send(200, await sessions.finish(body.sessionId, body.clientId));
          default:
            return send(404, { error: "Unknown API" });
        }
      }
      if (req.method !== "GET") return send(405, { error: "Use GET" });
      const resource = url.pathname === "/" ? "/taskpane.html" : url.pathname;
      const allowed = [
        "/taskpane.html",
        "/taskpane.js",
        "/taskpane.css",
        "/protocol.js",
        "/edit-flow.js",
      ];
      if (
        !allowed.includes(resource) &&
        !/^\/host-adapters\/[a-zA-Z0-9_-]+\.js$/.test(resource) &&
        !/^\/assets\/icon-(16|32|80)\.png$/.test(resource)
      )
        return send(404, { error: "Unknown resource" });
      const data = await readFile(path.join(ROOT, resource));
      const type = resource.endsWith(".html")
        ? "text/html; charset=utf-8"
        : resource.endsWith(".css")
          ? "text/css; charset=utf-8"
          : resource.endsWith(".png")
            ? "image/png"
            : "text/javascript; charset=utf-8";
      return send(200, data, type);
    } catch (error) {
      if (!res.headersSent) send(error.status ?? 400, { error: error.message });
      else res.end();
    } finally {
      if (counted) inflight--;
    }
  };
}

export async function start(config) {
  if (!path.isAbsolute(config.executable) || !path.isAbsolute(config.recoveryDir))
    throw new Error("Configured executable and recovery directory must be absolute paths");
  const executable = await realpath(config.executable);
  if (!(await stat(executable)).isFile())
    throw new Error("Configured ReShiki executable is not a file");
  const port = config.port ?? 43127;
  if (!Number.isInteger(port) || port < 1024 || port > 65535)
    throw new Error("Invalid localhost port");
  await mkdir(config.recoveryDir, { recursive: true, mode: 0o700 });
  const worker = (mode, input) => runWorker(executable, mode, input);
  const sessions = new Sessions({ recoveryDir: config.recoveryDir, executable, worker });
  const origin = `https://localhost:${port}`;
  const server = https.createServer(
    { cert: await readFile(config.cert), key: await readFile(config.key), minVersion: "TLSv1.2" },
    makeHandler({ origin, sessions, worker }),
  );
  server.requestTimeout = 20000;
  server.headersTimeout = 10000;
  server.timeout = 25000;
  // localhost manifests use IPv4. Never bind to all interfaces.
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  return { server, origin, sessions };
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const index = process.argv.indexOf("--config");
  if (index < 0 || !process.argv[index + 1]) {
    console.error("Usage: node companion/server.js --config /absolute/path/config.json");
    process.exitCode = 1;
  } else {
    try {
      const config = JSON.parse(await readFile(process.argv[index + 1], "utf8"));
      const { origin } = await start(config);
      console.log(
        `ReShiki Office companion: ${origin}\nRecovery: ${config.recoveryDir}\nKeep this process running while editing.`,
      );
    } catch (error) {
      console.error(error.message);
      process.exitCode = 1;
    }
  }
}
