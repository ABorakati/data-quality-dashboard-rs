//! Property tests for the result-evaluation and summary code.
//!
//! Only behaviour guaranteed by upstream DataQualityDashboard 2.9.0 is asserted:
//! `R/evaluateThresholds.R`, `R/calculateNotApplicableStatus.R` and
//! `R/summarizeResults.R`. Case counts are kept small so the file runs in seconds.

use data_quality_dashboard::catalog::Catalog;
use data_quality_dashboard::evaluate::{
    calculate_not_applicable, evaluate_thresholds, round_percent, summarize,
};
use data_quality_dashboard::model::Row;
use proptest::prelude::*;
use serde_json::{Value, json};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 128,
        max_shrink_iters: 256,
        ..ProptestConfig::default()
    }
}

fn row(value: Value) -> Row {
    value.as_object().expect("object").clone()
}

fn get<'a>(row: &'a Row, name: &str) -> &'a Value {
    row.get(name).unwrap_or(&Value::Null)
}

fn flag_of(row: &Row, name: &str) -> Option<i64> {
    get(row, name).as_i64()
}

// ---------------------------------------------------------------------------
// round(x, 2): independent reference for R >= 4.0 (src/nmath/fround.c).
// Candidates are the two neighbouring 2-decimal values; the closer one in the
// original scale wins; an exact tie goes to the even last digit.
// ---------------------------------------------------------------------------

fn reference_round2(x: f64) -> f64 {
    assert!(x.is_finite() && x >= 0.0);
    let k = (x * 100.0).floor() as i64;
    let lo = k as f64 / 100.0;
    let hi = (k + 1) as f64 / 100.0;
    let (d_lo, d_hi) = (x - lo, hi - x);
    if d_lo < d_hi {
        lo
    } else if d_hi < d_lo {
        hi
    } else if k % 2 == 0 {
        lo
    } else {
        hi
    }
}

fn rounded(x: f64) -> f64 {
    round_percent(x)
        .as_f64()
        .expect("finite input gives a number")
}

#[test]
fn round_percent_known_r_values() {
    // R 4.x: round(12.5, 0) style ties-to-even at the 2-decimal scale.
    for (x, want) in [
        (0.125, 0.12),
        (0.375, 0.38),
        (0.625, 0.62),
        (0.875, 0.88),
        (33.333333333333336, 33.33),
        (66.66666666666667, 66.67),
        (100.0, 100.0),
        (0.0, 0.0),
    ] {
        assert_eq!(rounded(x), want, "round({x}, 2)");
    }
    assert_eq!(round_percent(f64::NAN), Value::Null);
    assert_eq!(round_percent(f64::INFINITY), Value::Null);
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn round_percent_matches_reference_on_arbitrary_doubles(r in 0.0f64..=1.0) {
        for x in [r, r * 100.0] {
            prop_assert_eq!(rounded(x), reference_round2(x));
        }
    }

    #[test]
    fn round_percent_matches_reference_on_tie_prone_values(k in 0u32..=20_000, n in 0u32..=20) {
        // k / 200 (percent scale k / 2) and dyadic k / 2^n.
        let dyadic = f64::from(k) / f64::from(1u32 << n);
        for r in [f64::from(k) / 200.0, dyadic.min(1.0)] {
            for x in [r, r * 100.0] {
                prop_assert_eq!(rounded(x), reference_round2(x));
            }
        }
    }

    #[test]
    fn round_percent_stays_close_and_is_idempotent(x in 0.0f64..=100.0) {
        let r = rounded(x);
        prop_assert!((r - x).abs() <= 0.005 + 1e-9);
        prop_assert_eq!(rounded(r), r);
    }

    #[test]
    fn round_percent_is_antisymmetric(x in 0.0f64..=100.0) {
        prop_assert_eq!(rounded(-x), -rounded(x));
    }
}

// ---------------------------------------------------------------------------
// summarize (R/summarizeResults.R). Row subscripts with NA count as rows
// (`nrow(df[NA, ])`), so the model counts rows whose condition is not FALSE.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Sr {
    failed: Option<u8>,
    passed: Option<u8>,
    error: Option<&'static str>,
    category: Option<&'static str>,
}

fn sr() -> impl Strategy<Value = Sr> {
    (
        prop::option::of(0u8..=1),
        prop::option::of(0u8..=1),
        prop::option::of(Just("boom")),
        prop::option::of(prop::sample::select(vec![
            "Plausibility",
            "Conformance",
            "Completeness",
            "Other",
        ])),
    )
        .prop_map(|(failed, passed, error, category)| Sr {
            failed,
            passed,
            error,
            category,
        })
}

fn to_row(s: &Sr) -> Row {
    row(json!({
        "failed": s.failed, "passed": s.passed, "error": s.error, "category": s.category,
    }))
}

fn not_false(v: Option<bool>) -> bool {
    v != Some(false)
}

fn and3(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

fn model_count(rows: &[Sr], cond: impl Fn(&Sr) -> Option<bool>) -> i64 {
    rows.iter().filter(|r| not_false(cond(r))).count() as i64
}

fn int(summary: &Row, name: &str) -> i64 {
    get(summary, name)
        .as_i64()
        .unwrap_or_else(|| panic!("{name} is not an integer: {:?}", get(summary, name)))
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn summarize_is_finite_serializable_and_consistent(
        rows in prop::collection::vec(sr(), 0..40)
    ) {
        let input: Vec<Row> = rows.iter().map(to_row).collect();
        let summary = summarize(&input);

        // Serializes and every number is finite; percentages are finite or null.
        let text = serde_json::to_string(&summary).expect("summary serializes");
        prop_assert!(!text.contains("NaN") && !text.contains("inf"));
        for (name, value) in &summary {
            if let Some(n) = value.as_f64() {
                prop_assert!(n.is_finite(), "{name} = {n}");
            } else {
                prop_assert!(value.is_null() && name.starts_with("percent"), "{name}: {value:?}");
            }
        }

        let total = rows.len() as i64;
        let failed = |r: &Sr| r.failed.map(|f| f == 1);
        let passed = |r: &Sr| r.passed.map(|p| p == 1);
        let in_cat = |c: &'static str| move |r: &Sr| r.category.map(|x| x == c);
        let overall = model_count(&rows, failed);

        prop_assert_eq!(int(&summary, "countTotal"), total);
        prop_assert_eq!(int(&summary, "countOverallFailed"), overall);
        prop_assert_eq!(int(&summary, "countPassed"), total - overall);
        prop_assert_eq!(
            int(&summary, "countErrorFailed"),
            rows.iter().filter(|r| r.error.is_some()).count() as i64
        );
        prop_assert_eq!(
            int(&summary, "countThresholdFailed"),
            model_count(&rows, |r| and3(failed(r), Some(r.error.is_none())))
        );
        prop_assert!(int(&summary, "countThresholdFailed") <= overall);

        for category in ["Plausibility", "Conformance", "Completeness"] {
            prop_assert_eq!(
                int(&summary, &format!("countTotal{category}")),
                model_count(&rows, in_cat(category))
            );
            prop_assert_eq!(
                int(&summary, &format!("countFailed{category}")),
                model_count(&rows, |r| and3(in_cat(category)(r), failed(r)))
            );
            prop_assert_eq!(
                int(&summary, &format!("countPassed{category}")),
                model_count(&rows, |r| and3(in_cat(category)(r), passed(r)))
            );
            let total_c = int(&summary, &format!("countTotal{category}"));
            let failed_c = int(&summary, &format!("countFailed{category}"));
            let passed_c = int(&summary, &format!("countPassed{category}"));
            prop_assert!(failed_c <= total_c && passed_c <= total_c);
        }

        // percentPassed / percentFailed: upstream round(n / (passed + failed) * 100, 2).
        if total == 0 {
            prop_assert!(get(&summary, "percentPassed").is_null());
            prop_assert!(get(&summary, "percentFailed").is_null());
        } else {
            let p = get(&summary, "percentPassed").as_f64().unwrap();
            let f = get(&summary, "percentFailed").as_f64().unwrap();
            prop_assert_eq!(p, reference_round2((total - overall) as f64 / total as f64 * 100.0));
            prop_assert_eq!(f, reference_round2(overall as f64 / total as f64 * 100.0));
            prop_assert!((0.0..=100.0).contains(&p) && (0.0..=100.0).contains(&f));
            prop_assert!((p + f - 100.0).abs() <= 0.01 + 1e-9);
        }
    }

    #[test]
    fn summarize_ignores_row_order(
        rows in prop::collection::vec(sr(), 0..30),
        seed in any::<u64>()
    ) {
        let mut shuffled = rows.clone();
        // Deterministic Fisher-Yates driven by the seed.
        let mut state = seed | 1;
        for i in (1..shuffled.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            shuffled.swap(i, (state % (i as u64 + 1)) as usize);
        }
        let a: Vec<Row> = rows.iter().map(to_row).collect();
        let b: Vec<Row> = shuffled.iter().map(to_row).collect();
        prop_assert_eq!(summarize(&a), summarize(&b));
    }
}

// ---------------------------------------------------------------------------
// evaluate_thresholds (R/evaluateThresholds.R) without the NA prerequisites.
// ---------------------------------------------------------------------------

const TABLES: [&str; 3] = ["PERSON", "VISIT_OCCURRENCE", "DRUG_EXPOSURE"];

fn threshold_strategy() -> impl Strategy<Value = Option<f64>> {
    prop::option::of(prop::sample::select(vec![
        0.0, 1.0, 5.0, 33.3, 50.0, 99.5, 100.0,
    ]))
}

fn catalog_for(thresholds: &[Option<f64>]) -> Catalog {
    let tables: Vec<Row> = TABLES
        .iter()
        .zip(thresholds)
        .map(|(table, threshold)| {
            row(json!({"cdmTableName": table, "isRequiredThreshold": threshold}))
        })
        .collect();
    Catalog::from_rows(Vec::new(), tables, Vec::new(), Vec::new())
}

#[derive(Clone, Debug)]
struct Er {
    table: usize,
    num_violated: Option<u32>,
    num_denominator: u32,
    pct: Option<f64>,
    error: Option<&'static str>,
}

fn er() -> impl Strategy<Value = Er> {
    (
        0usize..TABLES.len(),
        prop::option::weighted(0.8, 0u32..1000),
        prop_oneof![Just(0u32), 1u32..1000],
        prop::option::weighted(0.85, prop_oneof![Just(0.0), 0.0f64..=1.0, Just(1.0)]),
        prop::option::weighted(0.25, prop::sample::select(vec!["boom", "no such table: x"])),
    )
        .prop_map(|(table, num_violated, num_denominator, pct, error)| Er {
            table,
            num_violated,
            num_denominator,
            pct,
            error,
        })
}

fn er_row(e: &Er) -> Row {
    row(json!({
        "checkName": "isRequired", "checkLevel": "TABLE", "cdmTableName": TABLES[e.table],
        "numViolatedRows": e.num_violated, "numDenominatorRows": e.num_denominator,
        "pctViolatedRows": e.pct, "error": e.error,
    }))
}

fn assert_no_nan(row: &Row) -> Result<(), TestCaseError> {
    for (name, value) in row {
        if let Some(n) = value.as_f64() {
            prop_assert!(n.is_finite(), "{name} = {n}");
        }
        if let Some(s) = value.as_str() {
            prop_assert!(s != "NaN" && !s.contains("NaN"), "{name} = {s}");
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn evaluate_thresholds_follows_upstream_comparison(
        thresholds in prop::collection::vec(threshold_strategy(), 3),
        rows in prop::collection::vec(er(), 1..12),
    ) {
        let catalog = catalog_for(&thresholds);
        let mut results: Vec<Row> = rows.iter().map(er_row).collect();
        // Upstream (evaluateThresholds.R:172) evaluates `NA * 100 > threshold` in
        // an `else if`, which stops with "missing value where TRUE/FALSE needed".
        let upstream_na_stop = rows.iter().any(|e| {
            let t = thresholds[e.table];
            e.error.is_none() && t.is_some_and(|t| t != 0.0) && e.pct.is_none()
        });
        let outcome = evaluate_thresholds(&mut results, &catalog);
        if upstream_na_stop {
            prop_assert!(outcome.is_err());
            return Ok(());
        }
        prop_assert!(outcome.is_ok(), "{:?}", outcome);
        prop_assert_eq!(results.len(), rows.len());
        for (e, out) in rows.iter().zip(&results) {
            assert_no_nan(out)?;
            for name in ["failed", "passed", "isError", "notApplicable"] {
                prop_assert!(matches!(flag_of(out, name), Some(0 | 1)), "{name}: {:?}", get(out, name));
            }
            prop_assert_eq!(flag_of(out, "notApplicable"), Some(0));
            let threshold = thresholds[e.table];
            prop_assert_eq!(get(out, "thresholdValue").as_f64(), threshold);
            let failed = flag_of(out, "failed") == Some(1);
            let is_error = flag_of(out, "isError") == Some(1);
            if e.error.is_some() {
                // checkName is not cdmTable/cdmField, so any error is an error.
                prop_assert!(is_error && !failed);
            } else {
                prop_assert!(!is_error);
                let expected = match threshold {
                    None | Some(0.0) => e.num_violated.is_some_and(|n| n > 0),
                    Some(t) => e.pct.unwrap() * 100.0 > t,
                };
                prop_assert_eq!(failed, expected);
            }
            prop_assert_eq!(flag_of(out, "passed") == Some(1), !failed && !is_error);
        }
    }

    #[test]
    fn evaluate_thresholds_zero_denominator_never_produces_nan(
        thresholds in prop::collection::vec(threshold_strategy(), 3),
        table in 0usize..TABLES.len(),
        pct_null in any::<bool>(),
        violated in prop::option::of(Just(0u32)),
    ) {
        let catalog = catalog_for(&thresholds);
        let pct = if pct_null { None } else { Some(0.0) };
        let mut results = vec![er_row(&Er {
            table, num_violated: violated, num_denominator: 0, pct, error: None,
        })];
        let outcome = evaluate_thresholds(&mut results, &catalog);
        let needs_pct = thresholds[table].is_some_and(|t| t != 0.0);
        if pct_null && needs_pct {
            // Same R stop as above: a NA percentage with a non-zero threshold.
            prop_assert!(outcome.is_err());
        } else {
            prop_assert!(outcome.is_ok());
            assert_no_nan(&results[0])?;
            prop_assert_eq!(flag_of(&results[0], "failed"), Some(0));
            prop_assert_eq!(flag_of(&results[0], "passed"), Some(1));
            serde_json::to_string(&results[0]).expect("serializes");
        }
    }
}

// ---------------------------------------------------------------------------
// calculate_not_applicable (R/calculateNotApplicableStatus.R) on a coherent
// result set: one cdmTable per table, one cdmField and one
// measureValueCompleteness per (table, field), plus regular checks.
// ---------------------------------------------------------------------------

/// One table: missing, empty, and its fields (missing, empty-ish).
type TablePlan = (bool, bool, Vec<(bool, bool)>);
/// One regular check: table index, field index, and its flags.
type RegularPlan = (usize, usize, bool, &'static str, bool);

#[derive(Clone, Debug)]
struct Plan {
    tables: Vec<TablePlan>,
    regulars: Vec<RegularPlan>,
}

fn plan() -> impl Strategy<Value = Plan> {
    prop::collection::vec(
        (
            any::<bool>(),
            any::<bool>(),
            prop::collection::vec((any::<bool>(), any::<bool>()), 1..3),
        ),
        1..4,
    )
    .prop_flat_map(|tables| {
        let n = tables.len();
        (
            Just(tables),
            prop::collection::vec(
                (
                    0..n,
                    0usize..3,
                    any::<bool>(),
                    prop::sample::select(vec![
                        "isRequired",
                        "measurePersonCompleteness",
                        "plausibleValueLow",
                    ]),
                    any::<bool>(),
                ),
                0..8,
            ),
        )
    })
    .prop_map(|(tables, regulars)| Plan { tables, regulars })
}

fn table_name(i: usize) -> String {
    format!("TABLE_{i}")
}

fn build_na_results(plan: &Plan) -> Vec<Row> {
    let mut rows = Vec::new();
    for (t, (missing, empty, fields)) in plan.tables.iter().enumerate() {
        let name = table_name(t);
        rows.push(row(json!({
            "checkName":"cdmTable","checkLevel":"TABLE","cdmTableName":name,
            "failed": u8::from(*missing), "isError": 0,
        })));
        for (f, (field_missing, field_empty)) in fields.iter().enumerate() {
            let field = format!("field_{f}");
            rows.push(row(json!({
                "checkName":"cdmField","checkLevel":"FIELD","cdmTableName":name,
                "cdmFieldName":field,"failed":u8::from(*field_missing),"isError":0,
            })));
            let denominator = if *empty { 0 } else { 5 };
            let violated = if *field_empty { denominator } else { 0 };
            rows.push(row(json!({
                "checkName":"measureValueCompleteness","checkLevel":"FIELD",
                "cdmTableName":name,"cdmFieldName":field,"failed":0,"isError":0,
                "numDenominatorRows":denominator,"numViolatedRows":violated,
            })));
        }
    }
    for (t, f, is_error, check, concept) in &plan.regulars {
        let fields = plan.tables[*t].2.len();
        rows.push(row(json!({
            "checkName": check, "checkLevel": if *concept { "CONCEPT" } else { "FIELD" },
            "cdmTableName": table_name(*t), "cdmFieldName": format!("field_{}", f % fields),
            "failed": u8::from(*is_error), "isError": u8::from(*is_error),
            "conceptId": "1", "unitConceptId": null,
            "numDenominatorRows": 3, "numViolatedRows": 1,
        })));
    }
    rows
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn calculate_not_applicable_respects_upstream_rules(plan in plan()) {
        let mut results = build_na_results(&plan);
        let before = results.len();
        let outcome = calculate_not_applicable(&mut results);
        prop_assert!(outcome.is_ok(), "{:?}", outcome);
        prop_assert_eq!(results.len(), before);
        let table_missing = |r: &Row| {
            let name = get(r, "cdmTableName").as_str().unwrap();
            let t: usize = name.trim_start_matches("TABLE_").parse().unwrap();
            plan.tables[t].0
        };
        for r in &results {
            assert_no_nan(r)?;
            for name in ["tableIsMissing", "fieldIsMissing", "tableIsEmpty", "fieldIsEmpty",
                         "conceptIsMissing", "conceptAndUnitAreMissing"] {
                prop_assert!(!r.contains_key(name), "{name} leaked");
            }
            let na = flag_of(r, "notApplicable");
            let failed = flag_of(r, "failed");
            let passed = flag_of(r, "passed");
            prop_assert!(matches!(na, Some(0 | 1)) && matches!(failed, Some(0 | 1)) && matches!(passed, Some(0 | 1)));
            let check = get(r, "checkName").as_str().unwrap();
            // Reason present exactly when notApplicable (`ifelse(notApplicable == 1, ..., NA)`).
            prop_assert_eq!(get(r, "notApplicableReason").is_string(), na == Some(1));
            if na == Some(1) {
                prop_assert_eq!(failed, Some(0));
                prop_assert_eq!(passed, Some(0));
            }
            if passed == Some(1) {
                prop_assert!(na == Some(0) && failed == Some(0) && flag_of(r, "isError") == Some(0));
            }
            // `.applyNotApplicable` (calculateNotApplicableStatus.R:54-84).
            if check == "cdmTable" {
                prop_assert_eq!(na, Some(0));
            } else if check == "cdmField" || check == "measurePersonCompleteness" {
                prop_assert_eq!(na == Some(1), table_missing(r));
            } else if table_missing(r) {
                prop_assert_eq!(na, Some(1));
            }
        }
    }
}
