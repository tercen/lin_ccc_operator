//! lin_ccc_operator — Lin's concordance correlation coefficient for Tercen.
//!
//! The crosstab contract: the two measurement methods are on the x and y axes, and each cell
//! (a row × column combination) holds the paired values to compare. The result is one row per
//! non-empty cell, carried back on `.ri` / `.ci`: the CCC with its confidence interval, its
//! Pearson (precision) and bias-correction (accuracy) parts, the pair count, and the
//! Bland–Altman bias and limits of agreement. See `algorithm` for the definitions.
pub mod algorithm;
pub mod context;
pub mod input;
pub mod output;
pub mod pagecache;
pub mod pairwise;
pub mod progress;
pub mod props;
pub mod tson;
pub mod upload;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use tercen_rs::context::ContextBase;
use tercen_rs::{DevContext, TercenClient};

use algorithm::Acc;
use progress::Reporter;
use tson::TsonWriter;

/// Values fetched per gRPC round trip (measured on the asinh operator: 200k is the knee).
const CHUNK: usize = 200_000;

/// Above this many cells the accumulators live in a hash map rather than a dense table, so a
/// sparse projection (many rows × many columns, few of them filled) does not allocate every
/// combination.
const DENSE_MAX_CELLS: usize = 4_000_000;

pub fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

pub fn require_env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow!("{name} is not set"))
}

/// Production entry point (`--taskId`).
pub async fn run(task_id: &str) -> Result<()> {
    tracing::info!("lin_ccc_operator starting (task_id={task_id})");
    let client = build_client().await?;
    // Deliberately not `ProductionContext::from_task_id`: see context.rs.
    let ctx = context::from_task_id(client, task_id).await?;
    execute(
        &ctx,
        Mode::Production {
            task_id: task_id.to_string(),
        },
    )
    .await
}

/// Dev entry point (`WORKFLOW_ID` / `STEP_ID`).
pub async fn run_dev(workflow_id: &str, step_id: &str) -> Result<()> {
    tracing::info!("lin_ccc_operator starting in dev mode ({workflow_id} / {step_id})");
    let client = build_client().await?;
    let ctx = DevContext::from_workflow_step(client, workflow_id, step_id)
        .await
        .map_err(|e| anyhow!("load workflow {workflow_id} / step {step_id}: {e}"))?;
    execute(
        &ctx,
        Mode::Dev {
            workflow_id: workflow_id.to_string(),
            step_id: step_id.to_string(),
        },
    )
    .await
}

enum Mode {
    Production {
        task_id: String,
    },
    Dev {
        workflow_id: String,
        step_id: String,
    },
}

async fn build_client() -> Result<Arc<TercenClient>> {
    let client = TercenClient::from_env()
        .await
        .map_err(|e| anyhow!("connect to Tercen: {e}"))?;
    tracing::info!("connected to Tercen");
    Ok(Arc::new(client))
}

/// Per-cell accumulators, dense or sparse.
pub enum Cells {
    Dense { n_cols: usize, acc: Vec<Acc> },
    Sparse(HashMap<(i32, i32), Acc>),
}

impl Cells {
    pub fn new(n_rows: usize, n_cols: usize) -> Self {
        let n = n_rows.max(1).saturating_mul(n_cols.max(1));
        if n <= DENSE_MAX_CELLS {
            Cells::Dense {
                n_cols: n_cols.max(1),
                acc: vec![Acc::default(); n],
            }
        } else {
            Cells::Sparse(HashMap::new())
        }
    }

    /// Fold one chunk in. An index outside the facet tables is an error, not a panic.
    pub fn add(&mut self, ri: &[i32], ci: &[i32], x: &[f64], y: &[f64]) -> Result<()> {
        match self {
            Cells::Dense { n_cols, acc } => {
                for k in 0..y.len() {
                    let (r, c) = (ri[k], ci[k]);
                    if r < 0 || c < 0 || c as usize >= *n_cols {
                        return Err(anyhow!("cell index (.ri {r}, .ci {c}) is out of range"));
                    }
                    let i = r as usize * *n_cols + c as usize;
                    acc.get_mut(i)
                        .ok_or_else(|| anyhow!("cell index (.ri {r}, .ci {c}) is out of range"))?
                        .add(x[k], y[k]);
                }
            }
            Cells::Sparse(m) => {
                for k in 0..y.len() {
                    m.entry((ri[k], ci[k])).or_default().add(x[k], y[k]);
                }
            }
        }
        Ok(())
    }

    /// The result rows, ordered by `.ci` then `.ri`, skipping cells that received no value.
    pub fn finish(self, z: f64) -> Vec<output::Row> {
        let mut rows: Vec<output::Row> = match self {
            Cells::Dense { n_cols, acc } => acc
                .into_iter()
                .enumerate()
                .filter(|(_, a)| a.n > 0)
                .map(|(i, a)| output::Row {
                    ri: (i / n_cols) as i32,
                    ci: (i % n_cols) as i32,
                    stats: a.finish(z),
                })
                .collect(),
            Cells::Sparse(m) => m
                .into_iter()
                .filter(|(_, a)| a.n > 0)
                .map(|((ri, ci), a)| output::Row {
                    ri,
                    ci,
                    stats: a.finish(z),
                })
                .collect(),
        };
        rows.sort_by_key(|r| (r.ci, r.ri));
        rows
    }
}

async fn execute(ctx: &ContextBase, mode: Mode) -> Result<()> {
    let t_start = Instant::now();
    tracing::info!(
        workflow = ctx.workflow_id(),
        step = ctx.step_id(),
        namespace = ctx.namespace(),
        "context loaded"
    );
    let rep = match &mode {
        Mode::Production { task_id } => Reporter::spawn(Arc::clone(ctx.client()), task_id.clone()),
        Mode::Dev { .. } => Reporter::silent(),
    };
    let s = props::settings_from_ctx(ctx)?;
    let z = algorithm::z_for_level(s.confidence_level);
    tracing::info!(?s, z, "properties");

    let n_values = input::cell_count(ctx).await?;
    let n_rows = input::row_count(ctx).await?;
    let n_cols = input::column_count(ctx).await?;
    tracing::info!(n_values, n_rows, n_cols, "crosstab size");
    rep.at(0, format!("Reading {n_values} value pairs"));

    let qt_cols = input::qt_columns(ctx).await?;
    let rows = if qt_cols.iter().any(|c| c == ".x") {
        read_xy(ctx, n_values, n_rows, n_cols, z, &rep).await?
    } else {
        let rnames = ctx
            .rnames()
            .await
            .map_err(|e| anyhow!("row factors: {e}"))?;
        let cnames = ctx
            .cnames()
            .await
            .map_err(|e| anyhow!("column factors: {e}"))?;
        let shared: Vec<&String> = rnames
            .iter()
            .filter(|r| !r.is_empty() && cnames.contains(r))
            .collect();
        if shared.is_empty() {
            return Err(anyhow!(
                "a numeric x axis is required: put the reference method on x and the method \
                 being compared on y. (Or, to compare every sample with every other, put the \
                 sample factor on both rows and columns and the observation id as a label.)"
            ));
        }
        // The label factors of the first axis query: the observation id to pair on.
        let labels_f: Vec<String> = ctx
            .cube_query()
            .axis_queries
            .first()
            .map(|aq| aq.labels.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        if labels_f.is_empty() {
            return Err(anyhow!(
                "pairwise projection ({} on rows and columns) without a label: add the factor \
                 that identifies an observation (for example the gene or peptide id) as a \
                 label, so values of two samples can be paired",
                shared
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        tracing::info!(?labels_f, ?shared, "pairwise projection");
        rep.info(format!(
            "Pairwise: comparing every {} with every other, paired on {}",
            shared
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            labels_f.join(", ")
        ));
        read_pairwise(ctx, n_values, n_rows, n_cols, z, &labels_f, &rep).await?
    };

    // `row_label` / `column_label`: each cell's row and column facet values as text, in both
    // modes. A downstream step that projects only these and the statistics reads the result
    // table on its own, which is the only way to show a pairwise matrix: the facets of a
    // pairwise projection share their factor names, and the platform's join of the result
    // back onto the input collides them (each sample's row of values collapses into one cell
    // showing their mean). See STATUS.md 0.1.2.
    let row_names = ctx
        .rnames()
        .await
        .map_err(|e| anyhow!("row factors: {e}"))?;
    let col_names = ctx
        .cnames()
        .await
        .map_err(|e| anyhow!("column factors: {e}"))?;
    let row_labels = input::facet_labels(ctx, ctx.row_hash(), &row_names).await?;
    let col_labels = input::facet_labels(ctx, ctx.column_hash(), &col_names).await?;
    tracing::info!(cells = rows.len(), "statistics computed");
    rep.at(progress::WRITE.0, format!("Writing {} cells", rows.len()));

    let work_root =
        std::env::temp_dir().join(format!("lin_ccc_{}_{}", ctx.workflow_id(), ctx.step_id()));
    std::fs::create_dir_all(&work_root)
        .with_context(|| format!("create {}", work_root.display()))?;
    let _guard = TempDirGuard(work_root.clone());
    let result_path = work_root.join("result.tson");
    {
        let f = std::fs::File::create(&result_path)
            .with_context(|| format!("create {}", result_path.display()))?;
        let mut w = TsonWriter::new(std::io::BufWriter::with_capacity(1 << 20, f))?;
        output::write_result(
            &mut w,
            &format!("{}_{}", ctx.step_id(), ctx.qt_hash()),
            ctx.namespace(),
            &rows,
            &row_labels,
            &col_labels,
        )?;
    }

    rep.at(progress::UPLOAD.0, "Uploading the result");
    match mode {
        Mode::Production { task_id } => {
            upload::save_production(ctx, &task_id, &result_path, &rep).await?
        }
        Mode::Dev {
            workflow_id,
            step_id,
        } => {
            let saved = upload::save_dev(ctx, &workflow_id, &step_id, &result_path).await?;
            tracing::info!(
                task_id = saved.task_id,
                file_id = saved.file_id,
                "dev result saved"
            );
        }
    }
    rep.at(100, "Done");
    rep.info(format!(
        "Lin's CCC: {} cells from {n_values} pairs in {:.1} s",
        rows.len(),
        t_start.elapsed().as_secs_f64()
    ));
    tracing::info!(
        total_secs = format!("{:.1}", t_start.elapsed().as_secs_f64()),
        peak_rss_kb = peak_rss_kb().unwrap_or(0),
        "done"
    );
    Ok(())
}

/// The explicit x/y projection: each cell's pairs are in the cell.
async fn read_xy(
    ctx: &ContextBase,
    n_values: usize,
    n_rows: usize,
    n_cols: usize,
    z: f64,
    rep: &Reporter,
) -> Result<Vec<output::Row>> {
    let mut cells = Cells::new(n_rows, n_cols);
    let mut seen = 0usize;
    input::for_each_chunk(ctx, &[".ri", ".ci", ".x", ".y"], n_values, CHUNK, |c| {
        cells.add(&c.ri, &c.ci, &c.x, &c.y)?;
        seen += c.len();
        rep.at(
            progress::band(progress::READ, seen, n_values.max(1)),
            format!("Read {seen} of {n_values} value pairs"),
        );
        Ok(())
    })
    .await?;
    Ok(cells.finish(z))
}

/// The pairwise projection: values sit on their own sample's diagonal cell, keyed by label.
async fn read_pairwise(
    ctx: &ContextBase,
    n_values: usize,
    n_rows: usize,
    n_cols: usize,
    z: f64,
    labels: &[String],
    rep: &Reporter,
) -> Result<Vec<output::Row>> {
    let mut columns: Vec<&str> = vec![".ri", ".ci", ".y"];
    columns.extend(labels.iter().map(|s| s.as_str()));
    let mut collector = pairwise::Collector::default();
    let mut seen = 0usize;
    input::for_each_chunk(ctx, &columns, n_values, CHUNK, |c| {
        collector.add(&c.ri, &c.ci, &c.keys, &c.y);
        seen += c.len();
        rep.at(
            progress::band(progress::READ, seen, n_values.max(1)),
            format!("Read {seen} of {n_values} values"),
        );
        Ok(())
    })
    .await?;
    let (rows, summary) = collector.finish(n_rows, n_cols, z)?;
    tracing::info!(?summary, "pairwise assembled");
    if summary.duplicates > 0 {
        rep.info(format!(
            "{} values share a sample and label with another value; the last one was used. \
             Add the factor that makes them distinct as a label.",
            summary.duplicates
        ));
    }
    Ok(rows)
}

fn peak_rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    s.lines()
        .find(|l| l.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

struct TempDirGuard(PathBuf);
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_and_sparse_agree() {
        let ri = [0, 1, 0, 1, 0, 1];
        let ci = [0, 0, 1, 1, 0, 0];
        let x = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let y = [1.1, 2.2, 2.9, 4.1, 5.3, 5.8];
        let mut d = Cells::new(2, 2);
        d.add(&ri, &ci, &x, &y).unwrap();
        let mut s = Cells::Sparse(HashMap::new());
        s.add(&ri, &ci, &x, &y).unwrap();
        let (d, s) = (d.finish(1.96), s.finish(1.96));
        assert_eq!(d.len(), s.len());
        for (a, b) in d.iter().zip(&s) {
            assert_eq!((a.ri, a.ci, a.stats.n), (b.ri, b.ci, b.stats.n));
            assert!(a.stats.ccc == b.stats.ccc || (a.stats.ccc.is_nan() && b.stats.ccc.is_nan()));
        }
    }

    #[test]
    fn an_index_outside_the_facets_is_an_error() {
        let mut d = Cells::new(1, 1);
        assert!(d.add(&[3], &[0], &[1.0], &[1.0]).is_err());
    }
}
