//! The result: one table, one row per non-empty cell, joined back on `.ri` / `.ci`.
//!
//! Shape is what `tercen-rs`'s `save_table` builds for a per-cell result — `{kind:
//! OperatorResult, tables: [table], joinOperators: []}` — written with the streaming TSON
//! writer the other Rust operators use, so the code path that uploads is the proven one.
use std::io::Write;

use anyhow::{Result, anyhow};

use crate::algorithm::CellStats;
use crate::tson::TsonWriter;

/// The statistic columns, in output order, without the namespace. `operator.json` declares
/// exactly these (checked by `tests/goldens.rs`).
pub const COLUMNS: [&str; 9] = [
    "ccc",
    "ccc_lower",
    "ccc_upper",
    "pearson_r",
    "bias_correction",
    "n",
    "ba_bias",
    "ba_lower",
    "ba_upper",
];

/// The text columns after the statistics: each cell's row and column facet values. Project
/// these (rows = `row_label`, columns = `column_label`) to show the result as a matrix.
pub const LABEL_COLUMNS: [&str; 2] = ["row_label", "column_label"];

/// One output row.
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub ri: i32,
    pub ci: i32,
    pub stats: CellStats,
}

/// `<namespace>.<name>` (R `ctx$addNamespace()`).
pub fn namespaced(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}.{name}")
    }
}

/// Write the whole result.
pub fn write_result<W: Write>(
    w: &mut TsonWriter<W>,
    table_name: &str,
    namespace: &str,
    rows: &[Row],
    row_labels: &[String],
    col_labels: &[String],
) -> Result<()> {
    let n = rows.len();
    let n_i32 = i32::try_from(n).map_err(|_| {
        anyhow!("the result would have {n} rows, more than a Tercen table can hold (i32::MAX)")
    })?;

    w.map(3)?;
    w.key("kind")?;
    w.str("OperatorResult")?;
    w.key("tables")?;
    w.list(1)?;

    w.map(4)?;
    w.key("kind")?;
    w.str("Table")?;
    w.key("nRows")?;
    w.i32(n_i32)?;
    w.key("properties")?;
    w.map(4)?;
    w.key("kind")?;
    w.str("TableProperties")?;
    w.key("name")?;
    w.str(table_name)?;
    w.key("sortOrder")?;
    w.list(0)?;
    w.key("ascending")?;
    w.bool(false)?;
    w.key("columns")?;
    w.list(COLUMNS.len() + 2 + LABEL_COLUMNS.len())?;

    column_header(w, ".ri", "int32", n_i32)?;
    w.i32_list(&rows.iter().map(|r| r.ri).collect::<Vec<_>>())?;
    column_header(w, ".ci", "int32", n_i32)?;
    w.i32_list(&rows.iter().map(|r| r.ci).collect::<Vec<_>>())?;

    for name in COLUMNS {
        let col = namespaced(namespace, name);
        if name == "n" {
            column_header(w, &col, "int32", n_i32)?;
            let v: Vec<i32> = rows
                .iter()
                .map(|r| i32::try_from(r.stats.n).unwrap_or(i32::MAX))
                .collect();
            w.i32_list(&v)?;
        } else {
            column_header(w, &col, "double", n_i32)?;
            let v: Vec<f64> = rows.iter().map(|r| value(&r.stats, name)).collect();
            w.f64_list(&v)?;
        }
    }

    // An absent facet (no row or no column factor) has no labels: empty text.
    let pick = |v: &[String], i: i32| v.get(i as usize).cloned().unwrap_or_default();
    column_header(w, &namespaced(namespace, LABEL_COLUMNS[0]), "string", n_i32)?;
    w.str_list(
        &rows
            .iter()
            .map(|r| pick(row_labels, r.ri))
            .collect::<Vec<_>>(),
    )?;
    column_header(w, &namespaced(namespace, LABEL_COLUMNS[1]), "string", n_i32)?;
    w.str_list(
        &rows
            .iter()
            .map(|r| pick(col_labels, r.ci))
            .collect::<Vec<_>>(),
    )?;

    // No explicit join: the server relates the table by its `.ri` / `.ci` columns, which is
    // what the R client's `ctx$save()` produces for a per-cell result.
    w.key("joinOperators")?;
    w.list(0)?;
    w.flush()?;
    Ok(())
}

/// The value of one statistic column for a cell.
pub fn value(s: &CellStats, name: &str) -> f64 {
    match name {
        "ccc" => s.ccc,
        "ccc_lower" => s.ccc_lower,
        "ccc_upper" => s.ccc_upper,
        "pearson_r" => s.pearson_r,
        "bias_correction" => s.bias_correction,
        "n" => s.n as f64,
        "ba_bias" => s.ba_bias,
        "ba_lower" => s.ba_lower,
        "ba_upper" => s.ba_upper,
        other => unreachable!("unknown column {other}"),
    }
}

fn column_header<W: Write>(w: &mut TsonWriter<W>, name: &str, ty: &str, n: i32) -> Result<()> {
    w.map(6)?;
    w.key("kind")?;
    w.str("Column")?;
    w.key("name")?;
    w.str(name)?;
    w.key("type")?;
    w.str(ty)?;
    w.key("nRows")?;
    w.i32(n)?;
    w.key("size")?;
    w.i32(n)?;
    w.key("values")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithm::Acc;

    #[test]
    fn the_columns_carry_the_namespace() {
        assert_eq!(namespaced("ds0", "ccc"), "ds0.ccc");
        assert_eq!(namespaced("", "ccc"), "ccc");
    }

    #[test]
    fn a_written_result_decodes_with_every_column() {
        let mut a = Acc::default();
        for i in 0..10 {
            a.add(i as f64, i as f64 * 1.1 + 0.3);
        }
        let rows = [Row {
            ri: 0,
            ci: 0,
            stats: a.finish(1.96),
        }];
        let mut w = TsonWriter::new(Vec::new()).unwrap();
        let (rl, cl) = (vec!["sampleA".to_string()], vec!["sampleB".to_string()]);
        write_result(&mut w, "t", "ds0", &rows, &rl, &cl).unwrap();
        let bytes = w.into_inner();
        let v = rustson::decode(std::io::Cursor::new(&bytes[..])).unwrap();
        let rustson::Value::MAP(m) = v else {
            panic!("not a map")
        };
        let rustson::Value::LST(tables) = &m["tables"] else {
            panic!("no tables")
        };
        let rustson::Value::MAP(t) = &tables[0] else {
            panic!("table not a map")
        };
        let rustson::Value::LST(cols) = &t["columns"] else {
            panic!("no columns")
        };
        let names: Vec<String> = cols
            .iter()
            .map(|c| {
                let rustson::Value::MAP(c) = c else { panic!() };
                let rustson::Value::STR(n) = &c["name"] else {
                    panic!()
                };
                n.clone()
            })
            .collect();
        let mut want = vec![".ri".to_string(), ".ci".to_string()];
        want.extend(COLUMNS.iter().map(|c| format!("ds0.{c}")));
        want.extend(LABEL_COLUMNS.iter().map(|c| format!("ds0.{c}")));
        assert_eq!(names, want);
        // and the labels are the facet values at the row's indices
        let values = |name: &str| -> Vec<String> {
            let c = cols
                .iter()
                .find_map(|c| {
                    let rustson::Value::MAP(c) = c else {
                        return None;
                    };
                    matches!(&c["name"], rustson::Value::STR(n) if n == name).then_some(c)
                })
                .unwrap();
            let rustson::Value::LSTSTR(v) = &c["values"] else {
                panic!("not text")
            };
            v.try_to_vec().unwrap()
        };
        assert_eq!(values("ds0.row_label"), ["sampleA"]);
        assert_eq!(values("ds0.column_label"), ["sampleB"]);
    }
}
