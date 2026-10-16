//! Pairwise projections: the same factor on rows and columns, no x axis.
//!
//! The usual way to compare every sample with every other one in Tercen is to put the sample
//! factor on **both** rows and columns, the value on y, and the observation id (a gene, a
//! peptide) as a **label**. The platform then stores each value once, on the diagonal cell of
//! its own sample (`.ri == .ci`), with no `.x` column — measured on tercen.com, 2026-10-16:
//! 1,079,790 values, all on the diagonal, keyed by `ds0.Gene_ID`.
//!
//! So the operator builds the off-diagonal cells itself. For cell (row i, column j) it pairs
//! the values of row-sample i (**y**) with those of column-sample j (**x**) on equal label
//! values, which is what the R correlation operator's pairwise mode computes.
//!
//! Memory: one f64 per (sample, observation) on each side, so about 16 bytes per value, and the
//! label text once per distinct observation.
use std::collections::HashMap;

use crate::algorithm::Acc;
use crate::output::Row;

/// The values of one side (rows or columns) as a dense samples × observations table.
pub struct Side {
    n_keys: usize,
    values: Vec<f64>,
}

impl Side {
    fn new(n: usize, n_keys: usize) -> Self {
        Side {
            n_keys,
            values: vec![f64::NAN; n.max(1) * n_keys],
        }
    }
    fn set(&mut self, sample: usize, key: usize, v: f64) -> bool {
        let i = sample * self.n_keys + key;
        let fresh = self.values[i].is_nan();
        self.values[i] = v;
        fresh
    }
    fn row(&self, sample: usize) -> &[f64] {
        &self.values[sample * self.n_keys..(sample + 1) * self.n_keys]
    }
}

/// Collects `(.ri, .ci, key, y)` as the stream arrives, then assembles every cell.
#[derive(Default)]
pub struct Collector {
    key_index: HashMap<String, u32>,
    entries: Vec<(i32, i32, u32, f64)>,
}

/// What the assembly found, for the task log.
#[derive(Debug, Default, PartialEq)]
pub struct Summary {
    pub observations: usize,
    /// The same (sample, observation) seen more than once; the last value wins.
    pub duplicates: usize,
}

impl Collector {
    pub fn add(&mut self, ri: &[i32], ci: &[i32], keys: &[String], y: &[f64]) {
        for k in 0..y.len() {
            let next = self.key_index.len() as u32;
            let id = *self.key_index.entry(keys[k].clone()).or_insert(next);
            self.entries.push((ri[k], ci[k], id, y[k]));
        }
    }

    /// Every (row, column) cell with at least one shared observation, ordered by `.ci` then
    /// `.ri`. `z` is the normal quantile for the interval.
    pub fn finish(
        self,
        n_rows: usize,
        n_cols: usize,
        z: f64,
    ) -> anyhow::Result<(Vec<Row>, Summary)> {
        let n_keys = self.key_index.len();
        let mut rows = Side::new(n_rows, n_keys);
        let mut cols = Side::new(n_cols, n_keys);
        let mut summary = Summary {
            observations: n_keys,
            duplicates: 0,
        };
        for (ri, ci, key, y) in self.entries {
            let (r, c) = (ri as usize, ci as usize);
            if ri < 0 || ci < 0 || r >= n_rows.max(1) || c >= n_cols.max(1) {
                anyhow::bail!("cell index (.ri {ri}, .ci {ci}) is out of range");
            }
            if !rows.set(r, key as usize, y) {
                summary.duplicates += 1;
            }
            cols.set(c, key as usize, y);
        }
        let mut out = Vec::new();
        for ci in 0..n_cols.max(1) {
            let x = cols.row(ci);
            for ri in 0..n_rows.max(1) {
                let y = rows.row(ri);
                let mut acc = Acc::default();
                for k in 0..n_keys {
                    acc.add(x[k], y[k]);
                }
                if acc.n > 0 {
                    out.push(Row {
                        ri: ri as i32,
                        ci: ci as i32,
                        stats: acc.finish(z),
                    });
                }
            }
        }
        Ok((out, summary))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Layout = (Vec<i32>, Vec<i32>, Vec<String>, Vec<f64>, Vec<Vec<f64>>);

    /// Three samples × 40 observations, laid out the way the platform stores a pairwise
    /// projection: every value on its own diagonal cell, keyed by the observation.
    fn diagonal_layout() -> Layout {
        let samples: Vec<Vec<f64>> = (0..3)
            .map(|s| {
                (0..40)
                    .map(|k| k as f64 * (1.0 + 0.1 * s as f64) + ((k * 7 + s * 3) % 5) as f64)
                    .collect()
            })
            .collect();
        let (mut ri, mut ci, mut keys, mut y) = (vec![], vec![], vec![], vec![]);
        // interleave samples, and shuffle observation order per sample, as a stream would
        for k in 0..40 {
            for (s, vals) in samples.iter().enumerate() {
                let kk = (k * 13 + s * 5) % 40;
                ri.push(s as i32);
                ci.push(s as i32);
                keys.push(format!("gene{kk}"));
                y.push(vals[kk]);
            }
        }
        (ri, ci, keys, y, samples)
    }

    #[test]
    fn every_pair_matches_a_direct_computation_on_aligned_vectors() {
        let (ri, ci, keys, y, samples) = diagonal_layout();
        let mut c = Collector::default();
        c.add(&ri[..50], &ci[..50], &keys[..50], &y[..50]);
        c.add(&ri[50..], &ci[50..], &keys[50..], &y[50..]);
        let (rows, summary) = c.finish(3, 3, 1.96).unwrap();
        assert_eq!(
            summary,
            Summary {
                observations: 40,
                duplicates: 0
            }
        );
        assert_eq!(rows.len(), 9, "every row × column cell");
        for r in &rows {
            let mut a = Acc::default();
            for (x, y) in samples[r.ci as usize].iter().zip(&samples[r.ri as usize]) {
                a.add(*x, *y);
            }
            let want = a.finish(1.96);
            assert_eq!(r.stats.n, 40);
            assert!((r.stats.ccc - want.ccc).abs() < 1e-14, "{:?}", (r.ri, r.ci));
            assert!((r.stats.ba_bias - want.ba_bias).abs() < 1e-12);
        }
    }

    #[test]
    fn the_diagonal_is_perfect_agreement_and_the_matrix_is_symmetric() {
        let (ri, ci, keys, y, _) = diagonal_layout();
        let mut c = Collector::default();
        c.add(&ri, &ci, &keys, &y);
        let (rows, _) = c.finish(3, 3, 1.96).unwrap();
        let at = |i: i32, j: i32| rows.iter().find(|r| r.ri == i && r.ci == j).unwrap().stats;
        for i in 0..3 {
            assert!((at(i, i).ccc - 1.0).abs() < 1e-14);
            for j in 0..3 {
                assert!(
                    (at(i, j).ccc - at(j, i).ccc).abs() < 1e-14,
                    "CCC is symmetric"
                );
                assert!(
                    (at(i, j).ba_bias + at(j, i).ba_bias).abs() < 1e-12,
                    "bias flips sign"
                );
            }
        }
    }

    #[test]
    fn observations_missing_in_one_sample_are_left_out_of_that_pair_only() {
        let ri = vec![0, 0, 0, 1, 1];
        let ci = ri.clone();
        let keys: Vec<String> = ["a", "b", "c", "a", "b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let y = vec![1.0, 2.0, 3.0, 1.5, 2.5];
        let mut c = Collector::default();
        c.add(&ri, &ci, &keys, &y);
        let (rows, _) = c.finish(2, 2, 1.96).unwrap();
        let n = |i, j| {
            rows.iter()
                .find(|r| r.ri == i && r.ci == j)
                .unwrap()
                .stats
                .n
        };
        assert_eq!(n(0, 0), 3);
        assert_eq!(n(0, 1), 2, "only a and b are shared");
        assert_eq!(n(1, 1), 2);
    }

    #[test]
    fn a_repeated_observation_is_counted() {
        let mut c = Collector::default();
        let k: Vec<String> = vec!["a".into(), "a".into()];
        c.add(&[0, 0], &[0, 0], &k, &[1.0, 2.0]);
        let (_, s) = c.finish(1, 1, 1.96).unwrap();
        assert_eq!(s.duplicates, 1);
    }
}
