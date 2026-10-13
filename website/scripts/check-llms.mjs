import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const output = process.argv[2]
  ? path.resolve(process.argv[2])
  : fileURLToPath(new URL("../dist/", import.meta.url));
const catalog = fileURLToPath(
  new URL("../../tests/fixtures/agent-contract/ops-catalog.json", import.meta.url),
);
const proposalSchema = fileURLToPath(
  new URL("../../tests/fixtures/agent-contract/proposal-schema.json", import.meta.url),
);
const maxBytes = 200 * 1024;

async function read(relativePath) {
  return readFile(path.join(output, relativePath)).catch((cause) => {
    throw new Error(`Missing or unreadable ${relativePath} in ${output}`, { cause });
  });
}

const index = (await read("llms.txt")).toString("utf8");
if (!/^# ReShiki\r?\n(?:[ \t]*\r?\n)*> /.test(index)) {
  throw new Error('llms.txt must start with "# ReShiki" followed by a "> " summary line.');
}

const agentApi = await read("_llms-txt/agent-api.txt");
if (agentApi.length >= maxBytes) {
  throw new Error(`agent-api.txt must be under ${maxBytes} bytes; found ${agentApi.length}.`);
}
// The plugin's Markdown output escapes underscores in headings (### document\_new).
const text = agentApi.toString("utf8").replaceAll("\\_", "_");
const tools = JSON.parse(await readFile(catalog, "utf8")).map((tool) => tool.name);
const missing = tools.filter((name) => !new RegExp(`^#{1,6} +${name}[ \\t]*\\r?$`, "m").test(text));
if (missing.length) {
  throw new Error(`agent-api.txt has no heading for these tools: ${missing.join(", ")}.`);
}
// The schema is a code block, so it is not escaped; compare without whitespace.
const squash = (value) => value.replace(/\s+/g, "");
const schema = JSON.stringify(JSON.parse(await readFile(proposalSchema, "utf8")));
if (!squash(agentApi.toString("utf8")).includes(squash(schema))) {
  throw new Error("agent-api.txt must contain the whole Proposal JSON Schema.");
}
console.log(
  `Verified llms.txt and agent-api.txt (${agentApi.length} bytes, ${tools.length} tools).`,
);
