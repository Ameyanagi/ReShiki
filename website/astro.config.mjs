import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightLlmsTxt from "starlight-llms-txt";
import { readdirSync } from "node:fs";
import { historicalTopics, movedRecords } from "./scripts/doc-topics.mjs";

const archivedPages = [
  "developer/changes-*",
  ...historicalTopics.map((topic) => `developer/${topic}`),
];

// Release notes, newest first: one docs/changes-<version>.md per release, so a
// new release appears without editing this file. All but the newest five fold away.
const releases = readdirSync(new URL("../docs/", import.meta.url))
  .map((name) => /^changes-(\d+)\.(\d+)(?:\.(\d+))?\.md$/.exec(name))
  .filter(Boolean)
  .map(([name, ...parts]) => ({
    slug: `developer/${name.slice(0, -3)}`,
    version: parts.map((part) => Number(part ?? 0)),
  }))
  .sort(
    (a, b) =>
      b.version[0] - a.version[0] || b.version[1] - a.version[1] || b.version[2] - a.version[2],
  )
  .map(({ slug, version }) => ({ slug, label: `ReShiki ${version.join(".")}` }));
const releaseNotes = [
  { label: "Unreleased changes", slug: "developer/changes-unreleased" },
  ...releases.slice(0, 5),
  { label: "Older releases", collapsed: true, items: releases.slice(5) },
];

// Implementation notes behind the guides, by area.
const engineeringNotes = {
  Drawing: [
    "bond-tools",
    "chain-tools",
    "tool-palettes",
    "3d-keyboard-drawing",
    "ring-presets",
    "template-library",
    "template-placement",
    "fragment-joining",
    "haworth-projections",
    "porphine-core",
    "selection-and-groups",
    "selection-transforms",
    "selection-cleanup",
    "selection-and-ring-placement",
    "reference-geometry",
    "rear-opacity",
    "abbreviations",
    "atom-labels",
    "typography",
    "arrows",
    "reactions",
    "graphics",
    "scientific-symbols",
    "drawing-styles",
    "jacs-style",
  ],
  Chemistry: [
    "chemical-naming-rust",
    "structure-to-name-harness",
    "nmr-prediction",
    "generic-structures",
    "mol-import-3d",
    "chemistry-resource-policy",
    "geometry-backend",
  ],
  Exchange: [
    "clipboard",
    "clipboard-compatibility",
    "haworth-interchange",
    "pictures",
    "figure-export",
    "publication-pages",
    "ruviz-integration",
  ],
  Platform: ["assistant", "assistant-benchmark", "framework-research"],
};

export default defineConfig({
  output: "static",
  site: "https://reshiki.com",
  base: "/",
  redirects: {
    "/reference/abbreviations": "/guide/labels/",
    "/reference/architecture": "/developer/architecture/",
    "/reference/arrows": "/guide/reactions/",
    "/reference/assistant": "/guide/assistant/",
    "/reference/atom-labels": "/guide/labels/",
    "/reference/bond-tools": "/guide/first-molecule/",
    "/reference/capabilities":
      "https://github.com/Ameyanagi/ReShiki/blob/main/docs/changes/capabilities.md",
    "/reference/chain-tools": "/guide/first-molecule/",
    "/reference/changes-0.2": "/developer/changes-0.2/",
    "/reference/clipboard": "/guide/sharing/",
    "/reference/contextual-shortcuts": "/guide/shortcuts/",
    "/reference/development": "/developer/development/",
    "/reference/drawing-styles": "/guide/figures/",
    "/reference/feature-status": "/developer/feature-status/",
    "/reference/fragment-joining": "/guide/templates/",
    "/reference/framework-research": "/developer/framework-research/",
    "/reference/getting-started": "/guide/install/",
    "/developer/getting-started": "/guide/install/",
    "/reference/graphics": "/guide/figures/",
    "/reference/jacs-style": "/guide/figures/",
    "/reference/pictures": "/guide/figures/",
    "/reference/publication-pages": "/guide/figures/",
    "/reference/reactions": "/guide/reactions/",
    "/reference/releasing": "/developer/releasing/",
    "/reference/ring-presets": "/guide/templates/",
    "/reference/roadmap": "/developer/roadmap/",
    "/reference/runtime-safety": "/developer/runtime-safety/",
    "/reference/ruviz-integration": "/developer/ruviz-integration/",
    "/reference/scientific-symbols": "/guide/figures/",
    "/reference/selection-and-groups": "/guide/editing/",
    "/reference/selection-and-ring-placement": "/developer/selection-and-ring-placement/",
    "/reference/selection-cleanup": "/guide/editing/",
    "/reference/selection-transforms": "/guide/editing/",
    "/reference/template-library": "/guide/templates/",
    "/reference/template-placement": "/guide/templates/",
    "/reference/tool-palettes": "/guide/first-molecule/",
    "/reference/typography": "/guide/labels/",
    "/reference/workspace-review":
      "https://github.com/Ameyanagi/ReShiki/blob/main/docs/changes/workspace-review.md",
    "/developer/contextual-shortcuts": "/guide/shortcuts/",
    "/developer/assistant-setup": "/guide/assistant-setup/",
    ...Object.fromEntries(
      movedRecords.map((name) => [
        `/developer/${name}`,
        `https://github.com/Ameyanagi/ReShiki/blob/main/docs/changes/${name}.md`,
      ]),
    ),
  },
  integrations: [
    starlight({
      title: "ReShiki",
      favicon: "/favicon.png",
      head: [
        { tag: "link", attrs: { rel: "apple-touch-icon", href: "/apple-touch-icon.png" } },
        {
          tag: "meta",
          attrs: { property: "og:image", content: "https://reshiki.com/brand/promotion-light.png" },
        },
        { tag: "meta", attrs: { property: "og:image:type", content: "image/png" } },
        { tag: "meta", attrs: { property: "og:image:width", content: "1200" } },
        { tag: "meta", attrs: { property: "og:image:height", content: "630" } },
        {
          tag: "meta",
          attrs: {
            property: "og:image:alt",
            content: "ReShiki — Chemical drawing, reinvented.",
          },
        },
        {
          tag: "meta",
          attrs: {
            name: "twitter:image",
            content: "https://reshiki.com/brand/promotion-light.png",
          },
        },
        {
          tag: "meta",
          attrs: { name: "twitter:image:alt", content: "ReShiki — Chemical drawing, reinvented." },
        },
      ],
      description: "Draw molecules, build reaction schemes, and prepare publication figures.",
      plugins: [
        starlightLlmsTxt({
          projectName: "ReShiki",
          description:
            "Free chemical drawing app (Rust) for molecules, reaction schemes and publication figures.",
          details:
            "AI agents can drive ReShiki through an experimental MCP server (`reshiki --mcp`) or command line (`reshiki --cli`), available since ReShiki 0.12.0. Start with the [Agent API set](https://reshiki.com/_llms-txt/agent-api.txt); its tool reference is generated from the code.",
          customSets: [
            {
              label: "Agent API",
              description:
                "Everything an AI agent needs to use ReShiki over MCP or the command line",
              paths: [
                "guide/agents",
                "developer/agent-api",
                "developer/agent-cli",
                "developer/agent-reference",
                "developer/agent-skill",
                "developer/runtime-safety",
              ],
            },
          ],
          demote: archivedPages,
          exclude: archivedPages,
          // Custom sets are minified like llms-small.txt. Keep newlines so headings,
          // tables and lists survive (check-llms.mjs looks for one heading per tool).
          // Keep <details> too: the Proposal JSON Schema sits in one.
          minify: { whitespace: false, details: false },
          optionalLinks: [
            {
              label: "GitHub repository",
              url: "https://github.com/Ameyanagi/ReShiki",
              description: "Source code, releases and issue reports.",
            },
            {
              label: "Privacy policy",
              url: "https://reshiki.com/developer/privacy-policy/",
              description: "Local processing and optional assistant data transfer.",
            },
          ],
        }),
      ],
      social: [
        { icon: "github", label: "Star on GitHub", href: "https://github.com/Ameyanagi/ReShiki" },
        {
          icon: "heart",
          label: "Sponsor this work",
          href: "https://github.com/sponsors/Ameyanagi",
        },
      ],
      customCss: ["./src/styles/custom.css"],
      components: { Header: "./src/components/DocsHeader.astro" },
      sidebar: [
        {
          label: "Start",
          items: [
            { label: "Why I’m building ReShiki", slug: "why" },
            { label: "Install ReShiki", slug: "guide/install" },
            { label: "Draw your first molecule", slug: "guide/first-molecule" },
          ],
        },
        {
          label: "Draw",
          items: [
            { label: "Build with templates", slug: "guide/templates" },
            { label: "Select and edit", slug: "guide/editing" },
            { label: "Labels and abbreviations", slug: "guide/labels" },
            { label: "Draw a reaction", slug: "guide/reactions" },
          ],
        },
        {
          label: "Finish and share",
          items: [
            { label: "Prepare a figure", slug: "guide/figures" },
            { label: "Journal drawing presets", slug: "developer/journal-drawing-presets" },
            { label: "Color palettes", slug: "guide/color-palettes" },
            { label: "Save and share", slug: "guide/sharing" },
            { label: "Acknowledging ReShiki", slug: "guide/acknowledgments" },
          ],
        },
        {
          label: "Chemistry tools",
          items: [
            { label: "Use chemical names", slug: "guide/names" },
            { label: "Predict NMR shifts", slug: "guide/nmr" },
            { label: "Expand generic structures", slug: "guide/generic-structures" },
            { label: "Crystal structures", slug: "developer/crystal-structures" },
          ],
        },
        {
          label: "AI",
          items: [
            { label: "Set up Codex", slug: "guide/assistant-setup" },
            { label: "Draw with the assistant", slug: "guide/assistant" },
            { label: "Connect AI agents (experimental)", slug: "guide/agents" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "Shortcuts and examples", slug: "guide/shortcuts" },
            { label: "Windows", slug: "guide/windows" },
            { label: "Privacy policy", slug: "developer/privacy-policy" },
            { label: "Code signing policy", slug: "developer/code-signing-policy" },
          ],
        },
        { label: "Release notes", collapsed: true, items: releaseNotes },
        {
          label: "Integrations",
          collapsed: true,
          items: [
            { label: "Python package", slug: "developer/python" },
            { label: "Browser editor (npm)", slug: "developer/npm-package" },
            { label: "CAS SciFinder", slug: "developer/scifinder-handoff" },
            { label: "Agent API overview", slug: "developer/agent-api" },
            { label: "Agent command line", slug: "developer/agent-cli" },
            { label: "Agent tool reference", slug: "developer/agent-reference" },
            { label: "Agent skill", slug: "developer/agent-skill" },
            { label: "Runtime safety", slug: "developer/runtime-safety" },
          ],
        },
        {
          label: "Developer",
          collapsed: true,
          items: [
            { label: "Build and test", slug: "developer/development" },
            { label: "Architecture", slug: "developer/architecture" },
            { label: "Module index", slug: "developer/modules" },
            { label: "Windows development", slug: "developer/windows" },
            { label: "Release and signing", slug: "developer/releasing" },
            { label: "SignPath setup", slug: "developer/signpath-setup" },
            { label: "Writing and terms", slug: "developer/writing-style" },
            { label: "Visual evidence for changes", slug: "developer/visual-review" },
            { label: "Feature status", slug: "developer/feature-status" },
            { label: "Roadmap", slug: "developer/roadmap" },
            { label: "Contributors", slug: "developer/contributors" },
          ],
        },
        {
          label: "Engineering notes",
          collapsed: true,
          items: Object.entries(engineeringNotes).map(([label, topics]) => ({
            label,
            collapsed: true,
            items: topics.map((name) => ({ slug: `developer/${name}` })),
          })),
        },
      ],
    }),
  ],
});
