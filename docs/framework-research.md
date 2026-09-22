# Framework decision

This records the initial framework decision and worker-based prototype. For the Rust chemistry engine and native InChI helper used in 0.6.0 builds, see [architecture](architecture.md).

Use Iced 0.14 for ReShiki's desktop UI, with a custom canvas and a Python/RDKit worker. Keep graph storage, drawing geometry and chemistry contracts outside the UI layer.

Iced's message/update model gives the editor a clear location for commands, undoable changes, selection and asynchronous results. Its [Canvas API](https://docs.rs/iced/0.14.0/iced/widget/canvas/) supports custom geometry and interaction. [Tasks](https://docs.rs/iced/0.14.0/iced/struct.Task.html) support asynchronous work, and the [0.14 release](https://github.com/iced-rs/iced/releases/tag/0.14.0) adds relevant rendering, input and testing work. This is a good fit for a custom molecular editor, but it does not supply chemical hit testing, a document model, rich atom labels or publication export; those remain ReShiki responsibilities.

Iced is not categorically more flexible than egui. Both support custom painting. The reason to choose Iced here is the user's preference and its fit with explicit editor state. The [egui Scene documentation](https://docs.rs/egui/latest/egui/containers/struct.Scene.html) also describes panning and zooming, along with text-blurring considerations during zoom. Framework choice alone does not establish chemical rendering quality or performance.

The implemented vertical slice validates the choice on this Mac: native windowing, controls, canvas tools, asynchronous chemistry, text outlines, file dialogs and SVG output work together. It also exposed two integration details that a mockup would miss: batched pointer events require event-specific coordinates, and native file filters can disable otherwise valid selections. Semantic accessibility, large-document performance, richer text editing and packaging remain unresolved.

RDKit provides a mature initial basis for molecule parsing, sanitization, aromaticity/stereochemistry, descriptors, 2D coordinates and interchange. Its [Python getting-started guide](https://www.rdkit.org/docs/GettingStartedInPython.html), [chemistry reference](https://www.rdkit.org/docs/RDKit_Book.html) and [installation guide](https://www.rdkit.org/docs/Install.html) are the primary implementation references. A persistent subprocess keeps Python runtime concerns separate from the Rust editor and provides a recoverable boundary for chemistry failures. It adds serialization and packaging costs, which are acceptable for this first version.

Basic XML drawing exchange follows the public [CDX/CDXML format documentation](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/). ReShiki's own document remains the source of truth because an interchange format may carry more objects and semantics than the current editor can represent.

The current evidence supports building the next editor features on Iced. It does not establish feature parity, a performance advantage over alternatives, or readiness to replace the chemistry backend without a larger validation corpus.
