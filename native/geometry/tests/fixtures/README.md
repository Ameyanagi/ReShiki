These inputs preserve native InChI atom and bond insertion order. They were captured from the published ReShiki InChI worker with `sanitize=true` and `remove_hydrogens=false`, using protocol 3 and InChI 1.07.5. Each capture returned status 0 without diagnostics. The geometry requests use MMFF94s, seed `0x52534b`, eight conformers, and 500 minimization iterations; individual tests may change the field or sample count.

`taxol70-request.json` uses the paclitaxel InChI retrieved for [PubChem CID 36314](https://pubchem.ncbi.nlm.nih.gov/compound/36314). Its 70 original atoms include 62 heavy atoms and eight explicit stereo hydrogens, with 76 bonds. This is the application's import representation; a heavy-atom-only reference omitted the eight original hydrogens and did not reproduce the reported failure. Request SHA-256: `8305acfc3019fd56a14f9269ca0d007e88c3a277144ec10d099d5773c1cba0f4`.

```text
InChI=1S/C47H51NO14/c1-25-31(60-43(56)36(52)35(28-16-10-7-11-17-28)48-41(54)29-18-12-8-13-19-29)23-47(57)40(61-42(55)30-20-14-9-15-21-30)38-45(6,32(51)22-33-46(38,24-58-33)62-27(3)50)39(53)37(59-26(2)49)34(25)44(47,4)5/h7-21,31-33,35-38,40,51-52,57H,22-24H2,1-6H3,(H,48,54)/t31-,32-,33+,35-,36+,37+,38-,40-,45+,46-,47+/m0/s1
```

`user-c36-request.json` uses the user's exact input below: 36 original carbon atoms and 42 bonds, with no specified atom or bond stereo. Native reconstruction and independent RDKit 2026.03.6 reconstruction have identical ordered bond records: 36 aromatic bonds and six double bonds. Request SHA-256: `5d41bb08f73fc8e3df10396f68d818b7d30ad5ecf196744d5329989a4c9b191d`.

```text
InChI=1S/C36H24/c1-2-26-4-3-25(1)27-5-7-29(8-6-27)31-13-15-33(16-14-31)35-21-23-36(24-22-35)34-19-17-32(18-20-34)30-11-9-28(26)10-12-30/h1-24H
```

Both eight-conformer requests reproduced the published application's `embedding lost a conformer` error when the embedding timeout returned a negative conformer ID. Single-conformer Taxol succeeded; single-conformer C36 still failed ETKDG's planarity check. An independent RDKit development probe succeeded for C36 using ETDG (`useBasicKnowledge=false`, `useExpTorsionAnglePrefs=true`) and converged with MMFF94, MMFF94s, and UFF. Integration tests assert valid geometry and retained identities rather than elapsed time or a platform-specific retry path. Optional same-coordinate energy and gradient comparisons remain development-only Python tests.

The complete C60 graph and independent spatial reference are in `c60-request.json` and `c60-reference.json`; the latter records its source InChI, oracle parameters, hashes, and closed set of 32 faces.
