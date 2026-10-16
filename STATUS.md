## 0.1.2 (2026-10-16) — `row_label` / `column_label`, so a pairwise result can be shown as a matrix

A downstream step on Faris's pairwise result (rows = columns = samples.variable, y = ccc) showed
only the diagonal, and with wrong values. Cause, in the platform: `_convertResultByRowCol`
(sci `sci_relation_vm/lib/src/sarno/save_result.dart`) joins a per-cell result to the column
facet on `.ci` and the row facet on `.ri`, and in a pairwise projection both facets carry the
same factor name, so they collide into one attribute. Each sample's row of CCCs then lands in
one cell and the crosstab shows their mean: measured on Studio, the five diagonal values were
exactly the per-sample means of the golden matrix.

The operator now writes each cell's facet values as two text columns, in both modes. A
downstream projection on `row_label` × `column_label` uses only result-side attributes, so the
query planner prunes the input and the facets (`_removeCompositeRelation`) and reads the result
table alone. Studio: 25 of 25 cells, worst relative difference 1.0e-15 against the pairwise
golden; the x/y platform export still matches its golden, labels included. The proper platform
fix (distinct names for the two facets of a pairwise result) is a sci change, not done here.

## 0.1.1 (2026-10-16) — pairwise projections

0.1.0 failed on Faris's tercen.com step with `Column named .x not found`: the step was a
pairwise projection (samples.variable on rows and columns, samples.value on y, ds0.Gene_ID as
a label, no x). In that layout the platform stores each value once on its own sample's
diagonal cell (`.ri == .ci`, measured: 1,079,790 values, all diagonal) and the gRPC API has no
pairwise select, so the operator now builds the grid itself (`src/pairwise.rs`): cell (i, j)
pairs sample i (y) with sample j (x) on equal label values. Detected when the table has no
`.x` and a factor is on both rows and columns; a pairwise projection without a label is a clear
error. Checked against a numpy pairwise golden (`dev/make_pairwise_golden.py`, 25 cells, one
missing observation) in the test suite and end to end on Studio (platform export, worst
relative difference 7e-15).

## 0.1.0 (2026-10-16) — first release

Lin's CCC per cell with Fisher-z interval, Pearson r, bias correction, n, and Bland–Altman
bias and limits. Parity with numpy, base R and the R correlation operator's golden on the
crabs fixture (worst relative difference below 1e-11 on every column). Memory model shipped as
a first-principles cut; refit after the first real runs.

Verified on Studio (tercen 1.1.11) before tagging:

- dev binary through the server (`dev/setup_studio.py`, then `target/release/dev`): 10 cells
  from 1,000 pairs in 2.1 s, peak RSS 12 MB; the result exported back through the platform
  (`dev/export_result.py`) matches `tests/test_1_out_1.csv` on every cell and column, worst
  relative difference 1.5e-14 (`dev/compare_to_golden.py`), and the platform's column types
  equal the sidecar's.
- production path: the release image (7.3 MB) run as uid 1000 with `--taskId` against a
  `RunComputationTask` with a pre-allocated result file (`dev/make_run_task.py`) uploads into
  that file; peak RSS 6 MB.
- Studio gRPC is the container address on 50051 (`TERCEN_URI=http://172.42.0.42:50051`), not
  the published 5402.
