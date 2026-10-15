//! Lin's concordance correlation coefficient, per cell, in one streaming pass.
//!
//! Pure arithmetic over `(x, y)` pairs: no Tercen types, so it is testable on its own and the
//! golden test can drive it straight from a CSV.
//!
//! Definitions (Lin 1989, *Biometrics* 45:255–268; corrected 2000), the ones `DescTools::CCC`
//! and `epiR::epi.ccc` implement, with the **biased** (1/n) moments Lin uses:
//!
//! ```text
//! ρc = 2·s_xy / (s_x² + s_y² + (x̄ − ȳ)²)          the coefficient
//! r  = s_xy / (s_x·s_y)                            Pearson, the precision part
//! Cb = ρc / r                                      the accuracy (bias-correction) part
//! ```
//!
//! The interval is on Fisher's z = atanh(ρc), with Lin's large-sample variance
//!
//! ```text
//! u    = (x̄ − ȳ) / sqrt(s_x·s_y)
//! se²z = [ (1−r²)ρc²/((1−ρc²)r²) + 2ρc³(1−ρc)u²/(r(1−ρc²)²) − ρc⁴u⁴/(2r²(1−ρc²)²) ] / (n−2)
//! ```
//!
//! and back-transformed with tanh. Bland–Altman uses the **unbiased** standard deviation of
//! `d = y − x`, as Bland & Altman do, and the same normal quantile as the interval.
//!
//! Moments are accumulated with Welford's running update, so a cell of a million values with
//! a large offset keeps full precision; a plain Σx² would not.

/// Running moments of one cell.
#[derive(Debug, Clone, Copy, Default)]
pub struct Acc {
    pub n: u64,
    mean_x: f64,
    mean_y: f64,
    /// Σ(x−x̄)², Σ(y−ȳ)², Σ(x−x̄)(y−ȳ), updated à la Welford.
    m2_x: f64,
    m2_y: f64,
    c_xy: f64,
}

impl Acc {
    /// Fold one pair in. Non-finite values are skipped, so a NaN in the data lowers `n`
    /// instead of poisoning the cell.
    #[inline]
    pub fn add(&mut self, x: f64, y: f64) {
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        self.n += 1;
        let n = self.n as f64;
        let dx = x - self.mean_x;
        let dy = y - self.mean_y;
        self.mean_x += dx / n;
        self.mean_y += dy / n;
        // (x − old mean)(x − new mean): the Welford cross term.
        self.m2_x += dx * (x - self.mean_x);
        self.m2_y += dy * (y - self.mean_y);
        self.c_xy += dx * (y - self.mean_y);
    }

    /// The statistics of the cell, for a two-sided interval at `z` standard errors.
    ///
    /// A cell that cannot support a statistic reports `n` and NaN for the rest: fewer than
    /// two pairs (nothing to correlate), fewer than three for the interval (Lin's variance
    /// divides by n−2), or a constant method (s_x or s_y zero, so r is undefined).
    pub fn finish(&self, z: f64) -> CellStats {
        let n = self.n as f64;
        let mut s = CellStats {
            n: self.n,
            ..CellStats::nan()
        };
        if self.n < 2 {
            return s;
        }
        let sxx = self.m2_x / n;
        let syy = self.m2_y / n;
        let sxy = self.c_xy / n;
        let dm = self.mean_x - self.mean_y;

        let denom = sxx + syy + dm * dm;
        if denom > 0.0 {
            s.ccc = 2.0 * sxy / denom;
        }
        if sxx > 0.0 && syy > 0.0 {
            let r = sxy / (sxx * syy).sqrt();
            s.pearson_r = r;
            if r != 0.0 {
                s.bias_correction = s.ccc / r;
            }
            if self.n >= 3 && r != 0.0 && s.ccc.abs() < 1.0 {
                let rc = s.ccc;
                let u = dm / (sxx * syy).sqrt().sqrt();
                let one_rc2 = 1.0 - rc * rc;
                let var_z = ((1.0 - r * r) * rc * rc / (one_rc2 * r * r)
                    + 2.0 * rc.powi(3) * (1.0 - rc) * u * u / (r * one_rc2 * one_rc2)
                    - rc.powi(4) * u.powi(4) / (2.0 * r * r * one_rc2 * one_rc2))
                    / (n - 2.0);
                if var_z.is_finite() && var_z >= 0.0 {
                    let se = var_z.sqrt();
                    let fz = rc.atanh();
                    s.ccc_lower = (fz - z * se).tanh();
                    s.ccc_upper = (fz + z * se).tanh();
                }
            }
        }
        // Bland–Altman on d = y − x. Var(d) = Var(y) + Var(x) − 2 Cov, from the same moments,
        // with the unbiased n−1 denominator.
        s.ba_bias = -dm;
        let var_d = (self.m2_y + self.m2_x - 2.0 * self.c_xy) / (n - 1.0);
        if var_d >= 0.0 {
            let sd = var_d.sqrt();
            s.ba_lower = s.ba_bias - z * sd;
            s.ba_upper = s.ba_bias + z * sd;
        }
        s
    }
}

/// What one cell reports. Every field is a result column (see `output::COLUMNS`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellStats {
    pub ccc: f64,
    pub ccc_lower: f64,
    pub ccc_upper: f64,
    pub pearson_r: f64,
    pub bias_correction: f64,
    pub n: u64,
    pub ba_bias: f64,
    pub ba_lower: f64,
    pub ba_upper: f64,
}

impl CellStats {
    pub fn nan() -> Self {
        Self {
            ccc: f64::NAN,
            ccc_lower: f64::NAN,
            ccc_upper: f64::NAN,
            pearson_r: f64::NAN,
            bias_correction: f64::NAN,
            n: 0,
            ba_bias: f64::NAN,
            ba_lower: f64::NAN,
            ba_upper: f64::NAN,
        }
    }
}

/// Two-sided normal quantile for a confidence level: `z` such that P(|Z| ≤ z) = `level`.
pub fn z_for_level(level: f64) -> f64 {
    normal_quantile(1.0 - (1.0 - level) / 2.0)
}

/// Inverse of the standard normal CDF. Wichura's algorithm AS 241 (PPND16), accurate to about
/// 1e-16 over the whole open interval — the same algorithm R's `qnorm` uses. The coefficients
/// are the published ones, kept verbatim, hence the lint allowances.
#[allow(clippy::excessive_precision, clippy::inconsistent_digit_grouping)]
pub fn normal_quantile(p: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) || p.is_nan() {
        return f64::NAN;
    }
    if p == 0.0 {
        return f64::NEG_INFINITY;
    }
    if p == 1.0 {
        return f64::INFINITY;
    }
    let q = p - 0.5;
    if q.abs() <= 0.425 {
        let r = 0.180625 - q * q;
        return q
            * (((((((2509.080_928_730_122_7 * r + 33430.575_583_588_13) * r
                + 67265.770_927_008_7)
                * r
                + 45921.953_931_549_87)
                * r
                + 13731.693_765_509_461)
                * r
                + 1971.590_950_306_551_3)
                * r
                + 133.141_667_891_784_38)
                * r
                + 3.387_132_872_796_366_5)
            / (((((((5226.495_278_852_545_2 * r + 28729.085_735_721_943) * r
                + 39307.895_800_092_71)
                * r
                + 21213.794_301_586_597)
                * r
                + 5394.196_021_424_751)
                * r
                + 687.187_007_492_057_9)
                * r
                + 42.313_330_701_600_91)
                * r
                + 1.0);
    }
    let mut r = if q < 0.0 { p } else { 1.0 - p };
    r = (-r.ln()).sqrt();
    let val = if r <= 5.0 {
        r -= 1.6;
        (((((((7.745_450_142_783_414e-4 * r + 0.022_723_844_989_269_184) * r
            + 0.241_780_725_177_450_6)
            * r
            + 1.270_458_252_452_368_4)
            * r
            + 3.647_848_324_763_204_5)
            * r
            + 5.769_497_221_460_691)
            * r
            + 4.630_337_846_156_546)
            * r
            + 1.423_437_110_749_683_6)
            / (((((((1.050_750_071_644_416_9e-9 * r + 5.475_938_084_995_345e-4) * r
                + 0.015_198_666_563_616_457)
                * r
                + 0.148_103_976_427_480_08)
                * r
                + 0.689_767_334_985_1)
                * r
                + 1.676_384_830_183_803_8)
                * r
                + 2.053_191_626_637_758_7)
                * r
                + 1.0)
    } else {
        r -= 5.0;
        (((((((2.010_334_399_292_288_1e-7 * r + 2.711_555_568_743_487_6e-5) * r
            + 0.001_242_660_947_388_078_4)
            * r
            + 0.026_532_189_526_576_124)
            * r
            + 0.296_560_571_828_504_87)
            * r
            + 1.784_826_539_917_291_3)
            * r
            + 5.463_784_911_164_114)
            * r
            + 6.657_904_643_501_103)
            / (((((((2.044_263_103_389_939_7e-15 * r + 1.421_511_758_316_445_9e-7) * r
                + 1.846_318_317_510_054_8e-5)
                * r
                + 7.868_691_311_456_132e-4)
                * r
                + 0.014_875_361_290_849_339)
                * r
                + 0.136_929_880_922_735_8)
                * r
                + 0.599_832_206_555_887_8)
                * r
                + 1.0)
    };
    if q < 0.0 { -val } else { val }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::excessive_precision)]
    fn quantiles_match_r_qnorm() {
        // R: qnorm(0.975) = 1.959963984540054, qnorm(0.995) = 2.5758293035489,
        // qnorm(0.95) = 1.6448536269514722
        assert!((normal_quantile(0.975) - 1.959_963_984_540_054).abs() < 1e-14);
        assert!((normal_quantile(0.995) - 2.575_829_303_548_9).abs() < 1e-12);
        assert!((normal_quantile(0.95) - 1.644_853_626_951_472_2).abs() < 1e-14);
        assert!((z_for_level(0.95) - 1.959_963_984_540_054).abs() < 1e-14);
        assert_eq!(normal_quantile(0.5), 0.0);
        assert!((normal_quantile(1e-10) + 6.361_340_902_404_056).abs() < 1e-9);
    }

    #[test]
    fn perfect_agreement_is_one_and_perfect_shift_lowers_it() {
        let mut a = Acc::default();
        for i in 0..50 {
            a.add(i as f64, i as f64);
        }
        let s = a.finish(1.96);
        assert!((s.ccc - 1.0).abs() < 1e-15);
        assert!((s.pearson_r - 1.0).abs() < 1e-15);
        assert_eq!(s.n, 50);
        assert!((s.ba_bias).abs() < 1e-15);

        let mut b = Acc::default();
        for i in 0..50 {
            b.add(i as f64, i as f64 + 10.0);
        }
        let t = b.finish(1.96);
        assert!((t.pearson_r - 1.0).abs() < 1e-15, "shift keeps precision");
        assert!(t.ccc < 1.0, "shift costs accuracy");
        assert!((t.ba_bias - 10.0).abs() < 1e-12);
    }

    #[test]
    fn a_common_offset_does_not_cost_precision() {
        // CCC is invariant to adding the same constant to both methods. A naive Σx² formulation
        // loses most of its digits at an offset of 1e9; Welford's update must not.
        let base: Vec<(f64, f64)> = (0..1000)
            .map(|i| ((i % 7) as f64 * 0.5, (i % 5) as f64 * 0.25 + 1.0))
            .collect();
        let run = |off: f64| {
            let mut a = Acc::default();
            for (x, y) in &base {
                a.add(x + off, y + off);
            }
            a.finish(1.96)
        };
        let (plain, shifted) = (run(0.0), run(1e9));
        let rel = (plain.ccc - shifted.ccc).abs() / plain.ccc.abs();
        assert!(
            rel < 1e-6,
            "ccc {} vs {} (rel {rel:e})",
            plain.ccc,
            shifted.ccc
        );
        assert!((plain.ba_bias - shifted.ba_bias).abs() < 1e-6);
    }

    #[test]
    fn degenerate_cells_report_n_and_nan_rather_than_panicking() {
        let mut one = Acc::default();
        one.add(1.0, 2.0);
        let s = one.finish(1.96);
        assert_eq!(s.n, 1);
        assert!(s.ccc.is_nan());

        let mut constant = Acc::default();
        for _ in 0..10 {
            constant.add(3.0, 7.0);
        }
        let c = constant.finish(1.96);
        assert_eq!(c.n, 10);
        assert!(c.pearson_r.is_nan(), "a constant method has no correlation");
        assert!(c.ccc.is_nan() || c.ccc == 0.0);

        let mut nans = Acc::default();
        nans.add(f64::NAN, 1.0);
        nans.add(1.0, f64::INFINITY);
        assert_eq!(nans.n, 0, "non-finite pairs are skipped");
    }
}
