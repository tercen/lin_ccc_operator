#!/usr/bin/env python3
"""Pairwise golden: every sample against every other, paired on the observation label.

Input is built from the crabs fixture (sp == "B"): sample = variable (5), observation = the
row's position within its variable (100), value = y_values, with observation 7 removed from
CW so one pair has fewer shared observations. Output: one row per (.ri, .ci) of the 5 × 5
grid, y = row sample, x = column sample, using the same closed forms as make_goldens.py.

Usage: make_pairwise_golden.py <test_1_in.csv> <out_dir>
"""
import csv, sys
from collections import defaultdict

inp, out_dir = sys.argv[1], sys.argv[2]
rows = [r for r in csv.DictReader(open(inp)) if r['sp'] == 'B']
pos = defaultdict(int); long = []
for r in rows:
    v = r['variable']; o = pos[v]; pos[v] += 1
    if v == 'CW' and o == 7:
        continue
    long.append((o, v, float(r['y_values'])))
with open(f'{out_dir}/pairwise_in.csv', 'w') as f:
    f.write('"observation","variable","value"\n')
    for o, v, y in long:
        f.write(f'{o},"{v}",{y!r}\n')

samples = sorted({v for _, v, _ in long})
vals = defaultdict(dict)
for o, v, y in long:
    vals[v][o] = y

import math, numpy as np
from scipy.stats import norm
q = norm.ppf(0.975)
def ccc(x, y):
    x, y = np.asarray(x), np.asarray(y); n = len(x)
    mx, my = x.mean(), y.mean()
    sxx, syy = ((x-mx)**2).mean(), ((y-my)**2).mean(); sxy = ((x-mx)*(y-my)).mean()
    rc = 2*sxy/(sxx+syy+(mx-my)**2)
    d = y - x
    return dict(n=n, ccc=rc, ba_bias=d.mean())

with open(f'{out_dir}/pairwise_out.csv', 'w') as f:
    f.write('".ri",".ci","ccc","ba_bias","n"\n')
    for ci, cs in enumerate(samples):
        for ri, rs in enumerate(samples):
            shared = sorted(set(vals[rs]) & set(vals[cs]))
            s = ccc([vals[cs][o] for o in shared], [vals[rs][o] for o in shared])
            f.write(f'{ri},{ci},{float(s["ccc"])!r},{float(s["ba_bias"])!r},{s["n"]}\n')
print('samples', samples, 'values', len(long))
