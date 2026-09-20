import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://ameyanagi.github.io",
  base: "/moruno",
  integrations: [
    starlight({
      title: "Moruno",
      description: "Draw molecules, build reaction schemes, and prepare publication figures.",
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/Ameyanagi/moruno" }],
      customCss: ["./src/styles/custom.css"],
      sidebar: [
        {
          label: "Start here",
          items: [
            { label: "Welcome", slug: "" },
            { label: "Getting started", slug: "reference/getting-started" },
            { label: "What is supported", slug: "reference/capabilities" },
          ],
        },
        {
          label: "Drawing",
          items: [
            "bond-tools",
            "atom-labels",
            "chain-tools",
            "ring-presets",
            "template-library",
            "template-placement",
            "fragment-joining",
            "abbreviations",
            "arrows",
            "reactions",
          ].map((name) => ({ slug: `reference/${name}` })),
        },
        {
          label: "Editing & presentation",
          items: [
            "selection-and-groups",
            "selection-transforms",
            "selection-cleanup",
            "typography",
            "jacs-style",
            "drawing-styles",
            "graphics",
            "pictures",
            "scientific-symbols",
            "publication-pages",
            "clipboard",
            "contextual-shortcuts",
            "tool-palettes",
            "assistant",
          ].map((name) => ({ slug: `reference/${name}` })),
        },
        {
          label: "Development",
          collapsed: true,
          items: [
            "development",
            "releasing",
            "architecture",
            "runtime-safety",
            "feature-status",
            "changes-0.2",
            "roadmap",
            "framework-research",
            "ruviz-integration",
            "workspace-review",
            "selection-and-ring-placement",
          ].map((name) => ({ slug: `reference/${name}` })),
        },
      ],
    }),
  ],
});
