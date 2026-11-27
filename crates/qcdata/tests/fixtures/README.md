`claisen-irc.json` contains the existing 14-atom Claisen verification run's
121 traced IRC geometries, two relaxed endpoints, and 15 NEB images. It was derived from `w2-ts/runs-final/
claisen/{irc,path}.xyz` and `w2-ts/runs-final/claisen.json` in the owner-provided
`xtb-program` scratch evidence. The arrays retain the report's full-precision
hartree energies; the ångström coordinates use qcdata's lossless deflate of
single-precision coordinates. The masses are the search's natural-abundance
table values: C 12.011, H 1.008, O 15.999 amu.

The source arrays include the relaxed endpoints at their first and last indices.
The fixture tests separate those endpoints when constructing an IRC result. The
ordered trace runs through one saddle. Both branches exhausted their downhill step budgets
before the subsequent endpoint relaxations; their `completed` flags therefore
remain false even though the relaxed endpoint classifications connect the
requested reaction. The fixture is computed numerical output, included under
the repository's MIT license; it contains no engine source code.
