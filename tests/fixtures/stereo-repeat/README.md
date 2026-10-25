# Stereo and repeated-label QA fixtures

Original ReShiki-generated inputs for issues #530–#532:

- `glucose-input.rsk`: alpha-D-glucopyranose Haworth template, expanded CH2OH.
- `pentaerythritol-input.rsk`: native depiction of `OCC(CO)(CO)CO`.
- `durene-input.rsk`: native depiction of `Cc1cc(C)c(C)cc1C`.
- `durene-three-methoxy.rsk`: the durene input after OMe at atoms 7, 10 and 1;
  atom 5 is the remaining lower-left methyl end.
- `before-properties.json` / `after-properties.json`: actual local-engine
  analysis recorded by the matched full-application renderer harness.

See [the QA record](../../../docs/changes/qa-stereo-repeat.md) for source commits,
exact actions, stereo round trips, native interactions and capture timing.
