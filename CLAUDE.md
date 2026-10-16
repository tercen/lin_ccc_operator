# lin_ccc_operator — maintenance notes

Rust operator for Lin's concordance correlation coefficient. Built 2026-10-16 on the
`asinh_operator` layout (the first Rust crosstab operator), which is where the plumbing modules
come from.

## What it is

Per cell (row × column of the crosstab): Lin's ρc between `.x` and `.y`, its Fisher-z
interval, Pearson r, the bias-correction factor C_b = ρc / r, the pair count, and Bland–Altman
bias and limits of agreement. One output table on `.ri` / `.ci`, one row per non-empty cell.
Definitions and edge cases are documented in `src/algorithm.rs`.

## Layout

| module | holds |
|---|---|
| `algorithm` | `Acc` (Welford moments per cell), `CellStats`, the normal quantile (AS 241) |
| `props` | the one setting, `confidence_level` |
| `input` | crosstab size, facet table sizes, chunked `.ri/.ci/.x/.y` decode |
| `output` | the result columns (`COLUMNS`) and the TSON writer for them |
| `pairwise` | pairwise projections: label-keyed samples × observations tables, every cell assembled |
| `lib` | `Cells` (dense up to 4 M cells, hash map beyond), mode detection and the run |

`context`, `tson`, `upload`, `pagecache` and `progress` are **copies** of the asinh modules,
kept identical on purpose; they should become a shared crate (see asinh's CLAUDE.md).

## Tests — three independent references

`tests/goldens.rs`, on the R `correlation_operator`'s own fixture (crabs, 10 cells × 100):

1. `tests/test_1_out_1.csv` — every column, from `dev/make_goldens.py` (numpy/scipy, two-pass).
   It is also the **platform unit test** golden (`tests/test.json`, typed by the `.schema`
   sidecars, as asinh 0.1.8 learned is required for int32 columns).
2. `tests/r_crosscheck.csv` — ρc and interval from a separate base-R implementation
   (`dev/ccc_check.R`).
3. `tests/r_cortest_out.csv` — the R correlation operator's published golden; `pearson_r`
   must equal its `cor_coef`.

Plus: the decomposition ρc = r·C_b on every cell, `operator.json` attributes / default /
container pin against the code, the sidecar types against the columns, and the memory model's
shape and coverage.

Regenerate goldens: `python3 dev/make_goldens.py tests/test_1_in.csv tests` and, from the repo
root, `Rscript dev/ccc_check.R`. Never regenerate them from this crate's own output.

## Label columns (0.1.2)

Every result row also carries `row_label` and `column_label` (`output::LABEL_COLUMNS`): the
row and column facet values as text, `" / "`-joined for several factors, empty for an absent
facet. They exist because a downstream projection on the original factors cannot show a
pairwise result (see STATUS 0.1.2); projecting the labels reads the result table alone.

## Memory model

`memory_model.json`: `8e-5 MB × n_main + 1.5 × 100 MB`. The binary is ~20 MB, one decoded
chunk of 200 k values is ~40 MB, and the accumulators are 48 B per cell. The linear term
covers the worst case (one value per cell) to 10 M values with 1.3× headroom; a normal CCC
projection (many pairs per cell) uses far less, so refit from `stats_d_actual_ram_peak` once
real runs exist. `n_main` is the number of projected values (the FAUST fit confirmed this).

## Release rule

`operator.json` `container` is bumped in the same commit as the tag; a test fails if it does
not match `Cargo.toml`'s version. Tag `X.Y.Z` (no `v`) triggers `release.yml`.

Local build needs `export ASDF_RUST_VERSION=1.94.0 PROTOC=$HOME/.local/protoc/bin/protoc`.
