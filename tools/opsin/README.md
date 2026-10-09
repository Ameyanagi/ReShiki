# Pinned offline OPSIN adapter

The unmodified official **opsin-core 2.9.0 jar-with-dependencies** is embedded
in the Rust executable with the original `ReShikiOpsin.java` adapter. Runtime
users need a compatible Java 11+ HotSpot JRE/JDK installed locally. No Java,
parser resources, classes or dependencies are downloaded at runtime. Python
is used only by the adapter rebuild script, never by the installed app.

Artifact source: [official 2.9.0 release](https://github.com/dan2097/opsin/releases/tag/2.9.0).
Grammar/source pin: tag 2.9.0, commit `b91b610af5ab07560fedb20730d7aef46bb2bca0`.
Licenses and bundled dependency provenance: [licenses/opsin](../../licenses/opsin/NOTICE).

| Payload                                      | SHA256                                                             |
| -------------------------------------------- | ------------------------------------------------------------------ |
| `opsin-core-2.9.0-jar-with-dependencies.jar` | `627ee5da4af551f9c4d1d766f545eb7cf519a344776e0bb677247a47abac0252` |
| `reshiki-opsin-adapter.jar`                  | `0ea22d92917e852a2666e035de278ef7e162c6350a7a78c2a89bbd31d2b6a388` |

Rebuild with the validated Zulu JDK 21.0.8+9 compiler (Java 11 class target)
and build-time Python:

```sh
python3 tools/opsin/build_adapter.py
```

Compilation targets Java 11 class files, omits debugging attributes, uses a
controlled empty CWD and removes JVM injection variables. The archive has
fixed timestamps, stored compression and sorted class entries. Rebuilding
must match the adapter checksum before changing the source pin. Other compiler
versions may produce different bytecode; the script rejects that change and
retains the existing payload. The Rust
adapter verifies both embedded payload hashes before each operation.

One UTF-8 input name (at most 2048 bytes, no controls) arrives on standard
input followed by EOF. One bounded JSON response includes protocol1,
version2.9.0, options`strict-cx13`, exact input, status, message, typed warnings
and semantic CXSMILES. No plain-SMILES fallback is accepted. CX flags include
enhanced stereo, polymers and atom labels; cosmetic atom values are excluded.
Radicals, permissive acid shorthand, wildcard radicals and uninterpretable
stereo are disabled. Relative/racemic/polymer/label semantics and any warning
cause a native adapter rejection rather than a simplified graph.

The installed app extracts the verified jars into a private temporary
directory and invokes Java from a separate empty working directory. This
prevents OPSIN's documented `./resources` override from replacing grammar.
JVM heap, metaspace, code cache and stack are bounded; parent timeout, output
caps and OS resource supervision terminate the dedicated Unix process group or
Windows job and reap the direct worker on failure or cancellation. Java must be
a trusted direct HotSpot executable, not a daemonizing launcher. Nonblocking
pipes and independent editor-lifetime controls are described in the limits. See [naming limits](../../docs/chemical-naming.md).
