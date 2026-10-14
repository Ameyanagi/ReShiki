# Command line

## Overview

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli <command> [options]

Commands:
  convert  Convert a structure or drawing to another file format
  render   Render a preview image of a structure or drawing
  compose  Lay out an assistant Proposal (JSON) and write it as a file
  analyze  Print molecular properties as one JSON line
  info     Print versions, limits and formats as one JSON line
  tools    Print the agent tool catalog as one JSON line
  schema   Print a JSON Schema: reshiki --cli schema proposal
  skill    Write an Agent Skill folder that teaches AI agents to use ReShiki
  help     Show this help, or a command's help: reshiki --cli help <command>

Options are allowed once each; -- ends the options.

Examples:
  reshiki --cli convert --smiles 'CCO' -o ethanol.mol
  reshiki --cli convert scheme.rsk -o figure.pdf
  reshiki --cli render scheme.rsk -o preview.png
  reshiki --cli analyze --smiles 'c1ccccc1O'
  reshiki --cli compose proposal.json -o scheme.svg

Output:
  stdout carries only the result: the file, one JSON line, or one receipt
  line with --receipt. With -o FILE and no --receipt it stays empty.
  Warnings and errors go to stderr.

Exit status:
  0   success
  1   the operation failed, or reading the input or writing the output failed
  2   usage error
  75  the memory ceiling was reached (see RESHIKI_AGENT_HEAP_MB)

Environment:
  RESHIKI_AGENT_HEAP_MB  memory ceiling in MiB, 256-16384
                         (default 2048); an invalid value exits 2

Files named on the command line are read and written with your own
permissions. On Windows, release builds cannot write to a console window:
redirect the output, for example `reshiki --cli info > info.json`.

Documentation: https://reshiki.com/guide/agents/
```

## reshiki --cli convert

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli convert (INPUT | - | --smiles TEXT) [--from FORMAT] [--to FORMAT]
                             [-o OUTPUT|-] [--pages] [--force] [--receipt]

Converts a structure or drawing, making the file as the app's Export does.

Input, exactly one of:
  INPUT          A file. Without --from, its extension picks the format:
                 .rsk, .reshiki and .moruno are ReShiki drawings; .mol, .rxn,
                 .rsmi, .cdxml, .cdx and .inchi are read as such; anything
                 else is read as SMILES.
  -              Standard input; needs --from.
  --smiles TEXT  SMILES text; cannot be combined with --from.

Options:
  --from FORMAT  The input format: auto, smiles, mol, rxn, rsmi, inchi, cdxml, cdx, reshiki.
  --to FORMAT    The output format: svg, pdf, png, cdxml, mol, smiles, inchi.
                 Without --to, the extension of OUTPUT picks it (.smi is smiles).
  -o OUTPUT      Write the file to OUTPUT. Without -o, or with -o -, the file
                 goes to stdout and needs --to; pdf and png are never written
                 to a terminal.
  --pages        Export every publication page as one PDF (pdf only).
  --force        Replace OUTPUT if it exists; otherwise that is an error.
  --receipt      With -o OUTPUT, print one JSON line to stdout:
                 {"receipt": {"format", "byte_len", "detail"}, "api": {…}}.

Inputs are at most 16777216 bytes (16777218 for cdx), so a ReShiki drawing with
large embedded pictures that the app opens can be too large here. rsk, rxn,
rsmi, cdx and emf files cannot be written.

Export details, such as PNG size and dpi, are printed to stderr as a
`reshiki: warning:` line; this does not mean the command failed.
```

## reshiki --cli render

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli render (INPUT | - | --smiles TEXT) [--from FORMAT] [--to FORMAT]
                            [-o OUTPUT|-] [--width N] [--height N] [--force]

Renders the preview the assistant looks at: png on white, scaled to fit
--width × --height, or svg with a transparent surround. For figures to
publish, use convert. The input is read as for convert
(reshiki --cli help convert).

Options:
  --to FORMAT  The image format: png, svg.
               Without --to, the extension of OUTPUT picks it.
  -o OUTPUT    Write the image to OUTPUT. Without -o, or with -o -, it goes
               to stdout and needs --to; png is never written to a terminal.
  --width N    The widest a png may be, in pixels (default 1600).
  --height N   The tallest a png may be, in pixels (default 1000).
               Each is 1 to 8192, and a png has at most 16000000 pixels.
  --force      Replace OUTPUT if it exists; otherwise that is an error.
```

## reshiki --cli compose

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli compose (PROPOSAL | -) [--to FORMAT] [-o OUTPUT|-] [--pages]
                             [--force] [--receipt]

Lays out a Proposal as ReShiki's assistant composes its drafts, in the
default style, then writes the drawing as convert does.

PROPOSAL is a JSON file, or - for standard input, of at most 16777216 bytes;
its replace_ids must be empty. The Proposal schema and an example are at
https://reshiki.com/developer/agent-reference/#proposal
Run `reshiki --cli schema proposal` for the schema.

Options:
  --to FORMAT  The output format: svg, pdf, png, cdxml, mol, smiles, inchi.
               Without --to, the extension of OUTPUT picks it (.smi is smiles).
  -o OUTPUT    Write the file to OUTPUT. Without -o, or with -o -, the file
               goes to stdout and needs --to; pdf and png are never written
               to a terminal.
  --pages      Export every publication page as one PDF (pdf only).
  --force      Replace OUTPUT if it exists; otherwise that is an error.
  --receipt    With -o OUTPUT, print one receipt JSON line to stdout, as
               convert does.

A ReShiki drawing (.rsk) cannot be written.

Export details, such as PNG size and dpi, are printed to stderr as a
`reshiki: warning:` line; this does not mean the command failed.
```

## reshiki --cli analyze

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli analyze (INPUT | - | --smiles TEXT) [--from FORMAT]

Prints the molecular properties of a structure or drawing as one JSON line:
SMILES, formula, mass, exact mass, logP, TPSA, hydrogen-bond donors and
acceptors, rings, unpaired electrons, InChI and InChIKey, under
value.analysis. The input is read as for convert (reshiki --cli help convert).
```

## reshiki --cli info

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli info

Prints ReShiki's versions, the limits it enforces and the input and output
formats as one JSON line.

grants and files describe the file tools of reshiki --mcp; the command line
uses neither.
```

## reshiki --cli tools

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli tools

Prints one JSON line with the experimental notice, the data-not-instructions
rule, the guide URL and every operation tool reshiki --mcp serves, including
its title, description, inputSchema and hints. The catalog is versioned by
api.operation_api. Run `reshiki --cli help --json` for the commands as JSON.
```

## reshiki --cli schema

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli schema NAME

NAME is proposal: the Proposal JSON Schema compose reads, the same schema
reshiki --mcp publishes. It is printed as one JSON line without the
experimental label, so it can be saved and used as a schema file.
```

## reshiki --cli skill

```text
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli skill -o FOLDER [--name NAME] [--executable PATH]
                           [--force] [--receipt]

Writes an Agent Skill that teaches AI agents to use ReShiki: the folder
FOLDER/NAME with SKILL.md, references/cli.md, references/mcp-tools.md,
references/limits-and-errors.md, references/setup.md,
assets/proposal-schema.json and assets/examples/esterification.json.
Review the files before you install the skill. Nothing is sent over the
network.

Options:
  -o FOLDER          The folder that holds skills, such as ~/.claude/skills,
                     .claude/skills, ~/.agents/skills or .agents/skills. It
                     must exist; it is never created.
  --name NAME        The skill's name and folder name (default reshiki):
                     1 to 64 lowercase letters, digits and hyphens, with no
                     leading, trailing or double hyphen. It must not contain
                     claude or anthropic.
  --executable PATH  The absolute path of the ReShiki executable that the
                     recipes and setup snippets run (default: this
                     executable, as the system reports its path).
  --force            Replace the generated files if FOLDER/NAME exists; other
                     files in it are kept. A symbolic link in place of
                     FOLDER/NAME or one of its generated folders is refused.
  --receipt          Print one JSON line to stdout after writing:
                     {"skill": {"path", "name", "files"}, "api": {…}}.

Without --force, an existing FOLDER/NAME is an error and nothing is
written. A failure part-way leaves the files already written. Without
--receipt, stdout stays empty.
```

## reshiki --cli help

Prints the [overview](#overview).

