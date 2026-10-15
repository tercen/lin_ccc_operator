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
