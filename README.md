# Lin's CCC operator

##### Description

Computes **Lin's concordance correlation coefficient** (CCC) between the x and y values of
each cell. CCC measures agreement, not just association: it is 1 only when the two methods
give the same values, and it drops both when the points scatter (precision) and when they sit
off the identity line (accuracy). Use it for method comparison, instrument or lab transfer,
and replicate reproducibility.

Alongside the coefficient the operator reports its confidence interval, its two parts, and the
Bland–Altman limits of agreement, so one step gives the full agreement picture.

##### Usage

Input|.
---|---
`x-axis` | values from the reference method (method A)
`y-axis` | values from the method being compared (method B), paired with x in the same cell
`row` | optional, factor for stratification (one CCC per level)
`column` | optional, factor for stratification (one CCC per level)
`labels` | pairwise only: the observation id to pair on (for example the gene id)

**Pairwise.** To compare every sample with every other, put the sample factor on both rows
and columns, the value on y, the observation id as a label, and leave x empty. Cell (row i,
column j) then compares sample i (y) with sample j (x) on the observations they share; the
diagonal is 1.

Settings|.
---|---
`confidence_level` | two-sided level for the CCC interval and the limits of agreement, default 0.95

Output|.
---|---
`ccc` | Lin's concordance correlation coefficient, ρc
`ccc_lower`, `ccc_upper` | confidence interval of ρc (Fisher z transform, Lin 1989)
`pearson_r` | Pearson correlation r, the precision part
`bias_correction` | C_b = ρc / r, the accuracy part (1 = no location or scale shift)
`n` | number of complete pairs in the cell
`ba_bias` | Bland–Altman mean difference, mean(y − x)
`ba_lower`, `ba_upper` | Bland–Altman limits of agreement, bias ± z·sd(y − x)

##### Details

Per cell, with the biased moments Lin uses:

    ρc = 2·s_xy / (s_x² + s_y² + (x̄ − ȳ)²),   r = s_xy / (s_x·s_y),   C_b = ρc / r

The interval is computed on z = atanh(ρc) with Lin's large-sample variance and transformed
back. These are the same definitions as `DescTools::CCC` and `epiR::epi.ccc` in R. Pairs with
a missing or non-finite value are skipped. A cell with fewer than 2 pairs, or where one method
is constant, reports `n` and leaves the undefined statistics empty; the interval needs at
least 3 pairs.

The operator streams the crosstab and keeps a few numbers per cell, so its memory does not grow
with the number of pairs per cell.

##### References

Lin, L. I. (1989). A concordance correlation coefficient to evaluate reproducibility.
*Biometrics* 45, 255–268. Correction: *Biometrics* 56 (2000), 324–325.

Bland, J. M., Altman, D. G. (1986). Statistical methods for assessing agreement between two
methods of clinical measurement. *Lancet* 1, 307–310.

##### See Also

[correlation_operator](https://github.com/tercen/correlation_operator)
