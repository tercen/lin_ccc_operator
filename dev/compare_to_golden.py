"""Compare a result exported from a Tercen instance with the golden, every column.

    tercenctl data export-csv -s <schemaId> --filePath /tmp/out.csv
    python dev/compare_to_golden.py /tmp/out.csv tests/test_1_out_1.csv

Checks the whole platform path: how the server read the TSON this operator wrote.
"""
import csv, sys

def load(path):
    rows = list(csv.DictReader(open(path)))
    if not rows:
        sys.exit(f"{path} is empty")
    return rows

got_rows, want_rows = load(sys.argv[1]), load(sys.argv[2])
stats = [c.split(".", 1)[1] for c in want_rows[0] if c.startswith("ds0.")]
def col(row, name):
    return next(c for c in row if c == name or c.endswith("." + name))
key = lambda r: (int(float(r[".ri"])), int(float(r[".ci"])))
got = {key(r): r for r in got_rows}
want = {key(r): r for r in want_rows}
missing, extra = set(want) - set(got), set(got) - set(want)
worst, at = 0.0, None
for k, w in want.items():
    if k not in got:
        continue
    for s in stats:
        a, b = float(got[k][col(got[k], s)]), float(w[col(w, s)])
        e = abs(a - b) / max(abs(b), 1e-12)
        if e > worst:
            worst, at = e, (k, s)
print(f"rows: {len(got)} from Tercen, {len(want)} golden; missing {len(missing)}, extra {len(extra)}")
print(f"worst relative difference {worst:.3e}" + (f" at {at}" if at else ""))
ok = not missing and not extra and worst <= 1e-9
print("PARITY OK" if ok else "PARITY FAILED")
sys.exit(0 if ok else 1)
