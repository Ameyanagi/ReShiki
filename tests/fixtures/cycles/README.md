# Catalytic cycle fixtures

`bowed-insertion.rsk` is a ReShiki-authored Suzuki cycle with contracted PPh3
labels. A bowed step has two incoming structures and one outgoing structure.
The free palladium intermediate reproduces the insertion that left those side
structures connected across the new neighbouring step. The regression uses the
captured drop translation and keeps the original roles and step settings.

`tidy-before.rsk` keeps the three-intermediate Suzuki–Miyaura cycle used in
the Tidy walkthrough, including its crowded reversible step. The regression
checks preserved roles, evenly spaced intermediates, painted-lane contacts,
preview/apply equality, one-step Undo and idempotence after reconciliation.
