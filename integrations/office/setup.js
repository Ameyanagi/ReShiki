#!/usr/bin/env node
// Generates configuration/manifests only. It never trusts a certificate, writes
// Office's sideload directories, changes a registry, or starts an Office app.
import { readFile, writeFile, mkdir, stat } from "node:fs/promises";
import { X509Certificate, createPrivateKey, createPublicKey } from "node:crypto";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";

export const HOSTS = [
  {
    name: "word",
    host: "Document",
    api: "WordApi",
    version: "1.4",
    id: "ea615cbd-ce62-4492-a8ea-cd5b426bd02e",
  },
  {
    name: "excel",
    host: "Workbook",
    api: "ExcelApi",
    version: "1.19",
    id: "118ea7ec-c8ea-4a11-a1f3-3c6a8f08d9dd",
  },
  {
    name: "powerpoint",
    host: "Presentation",
    api: "PowerPointApi",
    version: "1.8",
    id: "59b424f2-7de1-4c81-aa8b-f9273f0d06b4",
  },
];
export function manifest(host, port = 43127) {
  if (!Number.isInteger(port) || port < 1024 || port > 65535)
    throw new Error("Invalid localhost port");
  const origin = `https://localhost:${port}`;
  return `<?xml version="1.0" encoding="UTF-8"?>
<OfficeApp xmlns="http://schemas.microsoft.com/office/appforoffice/1.1" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="TaskPaneApp">
  <Id>${host.id}</Id><Version>1.0.0.0</Version><ProviderName>ReShiki</ProviderName><DefaultLocale>en-US</DefaultLocale>
  <DisplayName DefaultValue="ReShiki"/><Description DefaultValue="Insert and edit native ReShiki molecular drawings with the local companion."/>
  <IconUrl DefaultValue="${origin}/assets/icon-32.png"/><HighResolutionIconUrl DefaultValue="${origin}/assets/icon-80.png"/>
  <SupportUrl DefaultValue="https://github.com/Ameyanagi/ReShiki/issues"/>
  <AppDomains><AppDomain>${origin}</AppDomain></AppDomains>
  <Hosts><Host Name="${host.host}"/></Hosts>
  <Requirements><Sets DefaultMinVersion="1.1"><Set Name="${host.api}" MinVersion="${host.version}"/></Sets></Requirements>
  <DefaultSettings><SourceLocation DefaultValue="${origin}/taskpane.html"/></DefaultSettings>
  <Permissions>ReadWriteDocument</Permissions>
</OfficeApp>
`;
}
export async function setup({ executable, cert, key, directory, port = 43127 }) {
  for (const [label, value] of Object.entries({ executable, cert, key, directory }))
    if (typeof value !== "string" || !path.isAbsolute(value))
      throw new Error(`${label} must be an absolute path`);
  if (!(await stat(executable)).isFile()) throw new Error("ReShiki executable must be a file");
  const certificate = new X509Certificate(await readFile(cert));
  if (!certificate.checkHost("localhost", { subject: "never" }))
    throw new Error("Certificate must include localhost in its subject alternative names");
  if (
    Date.parse(certificate.validTo) <= Date.now() ||
    Date.parse(certificate.validFrom) > Date.now()
  )
    throw new Error("Certificate is not currently valid");
  const privateKey = createPrivateKey(await readFile(key));
  if (
    !certificate.publicKey
      .export({ type: "spki", format: "der" })
      .equals(createPublicKey(privateKey).export({ type: "spki", format: "der" }))
  )
    throw new Error("Certificate and private key do not match");
  const rendered = HOSTS.map((host) => [host.name, manifest(host, port)]);
  await mkdir(path.join(directory, "manifests"), { recursive: true, mode: 0o700 });
  await mkdir(path.join(directory, "recovery"), { recursive: true, mode: 0o700 });
  // Exclusive creation prevents accidentally replacing an existing installation.
  await writeFile(
    path.join(directory, "config.json"),
    JSON.stringify(
      { executable, cert, key, recoveryDir: path.join(directory, "recovery"), port },
      null,
      2,
    ) + "\n",
    { flag: "wx", mode: 0o600 },
  );
  for (const [name, xml] of rendered)
    await writeFile(path.join(directory, "manifests", `${name}.xml`), xml, {
      flag: "wx",
      mode: 0o600,
    });
  return directory;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = new Map();
  for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i], process.argv[i + 1]);
  try {
    const directory = await setup({
      executable: args.get("--executable"),
      cert: args.get("--cert"),
      key: args.get("--key"),
      directory: args.get("--directory") ?? path.join(os.homedir(), ".reshiki-office"),
      port: args.has("--port") ? Number(args.get("--port")) : 43127,
    });
    console.log(
      `Configuration prepared: ${path.join(directory, "config.json")}\nManifests: ${path.join(directory, "manifests")}\nNo certificate trust or Office sideload settings were changed.`,
    );
  } catch (error) {
    console.error(
      `${error.message}\nUsage: node setup.js --executable /absolute/path/reshiki --cert /absolute/path/localhost.crt --key /absolute/path/localhost.key [--directory /absolute/path/install] [--port 43127]`,
    );
    process.exitCode = 1;
  }
}
