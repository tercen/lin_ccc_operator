#!/usr/bin/env python3
"""Reference implementation of Lin's concordance correlation coefficient (Lin 1989, with the
Fisher-z confidence interval as in DescTools::CCC / epiR::epi.ccc) and Bland-Altman limits of
agreement, used to generate the golden fixture the Rust tests compare against.

Independent of the Rust code by construction: numpy, two-pass moments, closed forms.
Usage: make_goldens.py <in.csv> <out_dir> [conf_level]
"""
import csv, math, sys
from collections import defaultdict
import numpy as np
from scipy.stats import norm

inp, out_dir = sys.argv[1], sys.argv[2]
conf = float(sys.argv[3]) if len(sys.argv) > 3 else 0.95
q = norm.ppf(1 - (1 - conf) / 2)

rows = list(csv.DictReader(open(inp)))
row_levels = sorted({r['sp'] for r in rows})        # .ri order = the row facet table
col_levels = sorted({r['variable'] for r in rows})  # .ci order = the column facet table
cells = defaultdict(lambda: ([], []))
for r in rows:
    x, y = cells[(r['sp'], r['variable'])]
    x.append(float(r['x_values'])); y.append(float(r['y_values']))

def ccc(x, y):
    x, y = np.asarray(x), np.asarray(y); n = len(x)
    mx, my = x.mean(), y.mean()
    sxx, syy = ((x - mx) ** 2).mean(), ((y - my) ** 2).mean()   # biased, as Lin 1989
    sxy = ((x - mx) * (y - my)).mean()
    rc = 2 * sxy / (sxx + syy + (mx - my) ** 2)
    r = sxy / math.sqrt(sxx * syy)
    cb = rc / r
    u = (mx - my) / (sxx * syy) ** 0.25
    z = math.atanh(rc)
    se2 = ((1 - r * r) * rc * rc / ((1 - rc * rc) * r * r)
           + 2 * rc ** 3 * (1 - rc) * u * u / (r * (1 - rc * rc) ** 2)
           - rc ** 4 * u ** 4 / (2 * r * r * (1 - rc * rc) ** 2)) / (n - 2)
    se = math.sqrt(se2)
    lo, hi = math.tanh(z - q * se), math.tanh(z + q * se)
    d = y - x
    bias = d.mean(); sd = d.std(ddof=1)
    return dict(n=n, ccc=rc, ccc_lower=lo, ccc_upper=hi, pearson_r=r, bias_correction=cb,
                ba_bias=bias, ba_lower=bias - q * sd, ba_upper=bias + q * sd)

cols = ['ccc', 'ccc_lower', 'ccc_upper', 'pearson_r', 'bias_correction', 'n',
        'ba_bias', 'ba_lower', 'ba_upper']
with open(f'{out_dir}/test_1_out_1.csv', 'w') as f:
    f.write('".ri",".ci",' + ','.join(f'"ds0.{c}"' for c in cols) + ',"ds0.row_label","ds0.column_label"\n')
    for ci, cl in enumerate(col_levels):
        for ri, rl in enumerate(row_levels):
            s = ccc(*cells[(rl, cl)])
            f.write(f'{ri},{ci},' + ','.join(repr(float(s[c])) if c != 'n' else str(s[c]) for c in cols)
                    + f',"{rl}","{cl}"\n')
with open(f'{out_dir}/test_1_out_2.csv', 'w') as f:
    f.write('"variable"\n' + ''.join(f'"{c}"\n' for c in col_levels))
with open(f'{out_dir}/test_1_out_3.csv', 'w') as f:
    f.write('"sp"\n' + ''.join(f'"{r}"\n' for r in row_levels))
print('cells', len(cells), 'conf', conf, 'q', q)
