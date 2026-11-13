//! Behavioural tests for render, split, translate and check. Expected values
//! were produced by running the SqlRender 1.19.7 jar (`SqlRender.renderSql`,
//! `SqlSplit.splitSql`, `SqlTranslate.translateSqlWithPath`/`check`).

use dqd_sqlrender::{
    TranslateError, Translator, render_sql, split_sql, try_render_sql, try_split_sql,
};
use std::sync::LazyLock;

static TRANSLATOR: LazyLock<Translator> = LazyLock::new(Translator::new);

fn render(sql: &str, params: &[(&str, &str)]) -> String {
    let names: Vec<&str> = params.iter().map(|p| p.0).collect();
    let values: Vec<&str> = params.iter().map(|p| p.1).collect();
    render_sql(sql, &names, &values)
}

fn translate(sql: &str, dialect: &str) -> String {
    TRANSLATOR
        .translate(sql, dialect, None)
        .expect("translates")
}

// ---------------------------------------------------------------- render

#[test]
fn if_then_else_selects_branch() {
    assert_eq!(
        render("SELECT 1 {@x == 1}?{A}:{B};", &[("x", "5")]),
        "SELECT 1 B;"
    );
    assert_eq!(
        render("SELECT 1 {@x == 5}?{A}:{B};", &[("x", "5")]),
        "SELECT 1 A;"
    );
}

#[test]
fn if_without_else() {
    assert_eq!(
        render("SELECT 1 {@x == 1}?{ A};", &[("x", "5")]),
        "SELECT 1 ;"
    );
    assert_eq!(
        render("SELECT 1 {@x == 5}?{ A};", &[("x", "5")]),
        "SELECT 1  A;"
    );
}

#[test]
fn nested_conditions() {
    let t = "{@a == 1}?{{@b == 2}?{B2}:{Bother}}:{Anot1}";
    assert_eq!(render(t, &[("a", "1"), ("b", "3")]), "Bother");
    assert_eq!(render(t, &[("a", "2"), ("b", "3")]), "Anot1");
    assert_eq!(render(t, &[("a", "1"), ("b", "2")]), "B2");
}

#[test]
fn several_conditions_in_one_template() {
    assert_eq!(
        render(
            "a {@x}?{1}:{2} b {@y}?{3}:{4} c",
            &[("x", "FALSE"), ("y", "TRUE")]
        ),
        "a 2 b 3 c"
    );
}

#[test]
fn multiline_branches_keep_newlines() {
    assert_eq!(
        render("SELECT\n{@x}?{\n  a,\n}:{\n  b,\n}\nc;", &[("x", "1")]),
        "SELECT\n\n  a,\n\nc;"
    );
}

#[test]
fn boolean_operators_and_parentheses() {
    let and = "{@x == 1 & @y == 2}?{both}:{notboth}";
    assert_eq!(render(and, &[("x", "1"), ("y", "2")]), "both");
    assert_eq!(render(and, &[("x", "1"), ("y", "3")]), "notboth");
    assert_eq!(
        render(
            "{@x == 1 | @y == 2}?{any}:{none}",
            &[("x", "0"), ("y", "2")]
        ),
        "any"
    );
    assert_eq!(
        render(
            "{(@x == 1 | @y == 2) & @z == 3}?{yes}:{no}",
            &[("x", "0"), ("y", "2"), ("z", "3")]
        ),
        "yes"
    );
    assert_eq!(render("{!@x}?{no}:{yes}", &[("x", "TRUE")]), "yes");
    assert_eq!(render("{@x}?{t}:{f}", &[("x", "true")]), "t");
}

#[test]
fn comparison_and_in_operators() {
    assert_eq!(render("{@x != 3}?{ne}:{eq}", &[("x", "3")]), "eq");
    assert_eq!(render("{@x <> 3}?{ne}:{eq}", &[("x", "4")]), "ne");
    assert_eq!(render("{@x IN (1, 2, 3)}?{in}:{out}", &[("x", "9")]), "out");
    // Java quirk: the list items keep their leading blanks, so " 2" != "2".
    assert_eq!(render("{@x IN (1, 2, 3)}?{in}:{out}", &[("x", "2")]), "out");
    assert_eq!(render("{@x IN (1,2,3)}?{in}:{out}", &[("x", "2")]), "in");
    assert_eq!(
        render("{'@x' IN ('a','b')}?{in}:{out}", &[("x", "b")]),
        "in"
    );
}

#[test]
fn unparseable_condition_is_an_error() {
    let error = try_render_sql("{foo}?{a}:{b}", &["x"], &["1"]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Error parsing boolean condition: \"foo\""
    );
    // `{@x}` with x=5 is not a boolean either.
    let error = try_render_sql("SELECT 1 {@x}?{A}:{B};", &["x"], &["5"]).unwrap_err();
    assert_eq!(error.to_string(), "Error parsing boolean condition: \"5\"");
}

#[test]
#[should_panic(expected = "Error parsing boolean condition")]
fn render_sql_panics_where_java_throws() {
    render_sql("{foo}?{a}", &[], &[]);
}

#[test]
fn defaults_apply_unless_overridden() {
    assert_eq!(render("{DEFAULT @x = 5}\nSELECT @x;", &[]), "SELECT 5;");
    assert_eq!(
        render("{DEFAULT @x = 5}\nSELECT @x;", &[("x", "7")]),
        "SELECT 7;"
    );
    assert_eq!(
        render("{DEFAULT @name = 'abc'}\nSELECT '@name';", &[]),
        "SELECT 'abc';"
    );
    assert_eq!(
        render("{DEFAULT @a = 1}\n{DEFAULT @b = 2}\nSELECT @a, @b;", &[]),
        "SELECT 1, 2;"
    );
    assert_eq!(
        render("{DEFAULT @flag = TRUE}\nSELECT 1 {@flag}?{, 2};", &[]),
        "SELECT 1 , 2;"
    );
}

#[test]
fn longest_parameter_name_is_substituted_first() {
    assert_eq!(
        render(
            "SELECT @abc, @ab, @a;",
            &[("a", "1"), ("ab", "2"), ("abc", "3")]
        ),
        "SELECT 3, 2, 1;"
    );
}

#[test]
fn substitution_order_is_visible_when_values_contain_parameters() {
    // `ab` is longer than `a`, so `@ab` is replaced first and its value `@a`
    // is then replaced by the later pass for `a`.
    assert_eq!(
        render("SELECT @ab @a", &[("ab", "@a"), ("a", "X")]),
        "SELECT X X"
    );
}

#[test]
fn equal_length_names_follow_java_hashmap_order() {
    // 20 equal-length parameters whose values reference each other; the result
    // depends on java.util.HashMap iteration order.
    let names: Vec<String> = (0..20u8)
        .map(|i| format!("p{}", (b'a' + i) as char))
        .collect();
    let values: Vec<String> = (0..20u8)
        .map(|i| format!("@p{}|{}", (b'a' + (i + 1) % 20) as char, i))
        .collect();
    let template: String = names.iter().map(|n| format!("@{n} ")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let values: Vec<&str> = values.iter().map(String::as_str).collect();
    let expected = "@pp|14|13|12|11|10|9|8|7|6|5|4|3|2|1|0 @pp|14|13|12|11|10|9|8|7|6|5|4|3|2|1 @pp|14|13|12|11|10|9|8|7|6|5|4|3|2 @pp|14|13|12|11|10|9|8|7|6|5|4|3 @pp|14|13|12|11|10|9|8|7|6|5|4 @pp|14|13|12|11|10|9|8|7|6|5 @pp|14|13|12|11|10|9|8|7|6 @pp|14|13|12|11|10|9|8|7 @pp|14|13|12|11|10|9|8 @pp|14|13|12|11|10|9 @pp|14|13|12|11|10 @pp|14|13|12|11 @pp|14|13|12 @pp|14|13 @pp|14 @pp|14|13|12|11|10|9|8|7|6|5|4|3|2|1|0|19|18|17|16|15 @pp|14|13|12|11|10|9|8|7|6|5|4|3|2|1|0|19|18|17|16 @pp|14|13|12|11|10|9|8|7|6|5|4|3|2|1|0|19|18|17 @pp|14|13|12|11|10|9|8|7|6|5|4|3|2|1|0|19|18 @pp|14|13|12|11|10|9|8|7|6|5|4|3|2|1|0|19 ";
    assert_eq!(render_sql(&template, &names, &values), expected);
}

#[test]
fn missing_parameters_stay_untouched() {
    assert_eq!(
        render("SELECT @missing, @present;", &[("present", "1")]),
        "SELECT @missing, 1;"
    );
}

#[test]
fn values_are_inserted_literally() {
    assert_eq!(
        render("SELECT '@v';", &[("v", "a$1\\b")]),
        "SELECT 'a$1\\b';"
    );
    assert_eq!(
        render("SELECT @x FROM t {@x == ''}?{WHERE 1}", &[("x", "")]),
        "SELECT  FROM t WHERE 1"
    );
    assert_eq!(
        render("SELECT * FROM t WHERE a IN (@ids);", &[("ids", "1,2,3")]),
        "SELECT * FROM t WHERE a IN (1,2,3);"
    );
}

#[test]
fn braces_without_question_mark_are_kept() {
    assert_eq!(
        render("SELECT '{not a condition}';", &[]),
        "SELECT '{not a condition}';"
    );
}

// ----------------------------------------------------------------- split

fn split(sql: &str) -> Vec<String> {
    split_sql(sql)
}

#[test]
fn splits_on_semicolons_and_drops_them() {
    assert_eq!(split("SELECT 1; SELECT 2;"), ["SELECT 1", "SELECT 2"]);
    assert_eq!(split("SELECT 1; SELECT 2"), ["SELECT 1", "SELECT 2"]);
    assert_eq!(
        split("  SELECT 1 ;\n\n  SELECT 2 ;\n"),
        ["SELECT 1 ", "SELECT 2 "]
    );
    assert_eq!(split("SELECT 1;; SELECT 2;"), ["SELECT 1", "", "SELECT 2"]);
    assert_eq!(split(";"), [";"]);
    assert!(split("").is_empty());
}

#[test]
fn semicolons_inside_literals_do_not_split() {
    assert_eq!(
        split("SELECT 'a;b' AS x; SELECT \"c;d\";"),
        ["SELECT 'a;b' AS x", "SELECT \"c;d\""]
    );
    assert_eq!(
        split("SELECT 'it''s; ok'; SELECT 2;"),
        ["SELECT 'it''s; ok'", "SELECT 2"]
    );
    assert_eq!(
        split("SELECT [a;b] FROM t; SELECT 2;"),
        ["SELECT [a;b] FROM t", "SELECT 2"]
    );
}

#[test]
fn comments_are_skipped() {
    assert_eq!(
        split("SELECT 1; -- c; d\nSELECT 2; /* x; y */ SELECT 3;"),
        ["SELECT 1", "SELECT 2", "SELECT 3"]
    );
}

#[test]
fn begin_end_blocks_stay_whole_and_keep_their_semicolon() {
    assert_eq!(
        split("BEGIN SELECT 1; SELECT 2; END; SELECT 3;"),
        ["BEGIN SELECT 1; SELECT 2; END;", "SELECT 3"]
    );
    assert_eq!(split("BEGIN SELECT 1; END"), ["BEGIN SELECT 1; END"]);
    assert_eq!(
        split("BEGIN BEGIN SELECT 1; END; END; SELECT 2;"),
        ["BEGIN BEGIN SELECT 1; END; END;", "SELECT 2"]
    );
}

#[test]
fn case_end_does_not_keep_semicolon() {
    assert_eq!(
        split("SELECT CASE WHEN a = 1 THEN 2 ELSE 3 END AS x; SELECT 2;"),
        ["SELECT CASE WHEN a = 1 THEN 2 ELSE 3 END AS x", "SELECT 2"]
    );
    assert_eq!(
        split("BEGIN SELECT CASE WHEN a THEN 1 END; END; SELECT 2;"),
        ["BEGIN SELECT CASE WHEN a THEN 1 END; END;", "SELECT 2"]
    );
}

#[test]
fn end_if_is_not_a_block_end() {
    assert_eq!(
        split("IF x THEN SELECT 1; END IF; SELECT 2;"),
        ["IF x THEN SELECT 1", "END IF", "SELECT 2"]
    );
}

#[test]
fn unbalanced_end_is_an_error_like_java_empty_stack() {
    let error = try_split_sql("SELECT 1 END;").unwrap_err();
    assert!(error.to_string().contains("EmptyStackException"));
}

// ------------------------------------------------------------- translate

#[test]
fn translate_matches_java_for_representative_statements() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "postgresql",
            "SELECT TOP 5 a FROM t;",
            "SELECT  a FROM t LIMIT 5;",
        ),
        (
            "sqlite",
            "SELECT TOP 5 a FROM t;",
            "SELECT  a FROM t LIMIT 5;",
        ),
        (
            "postgresql",
            "SELECT a INTO #tmp FROM t;",
            "CREATE TEMP TABLE tmp \nAS\nSELECT\na \nFROM\nt;\nANALYZE tmp ;",
        ),
        (
            "sqlite",
            "SELECT a INTO #tmp FROM t;",
            "CREATE TEMP TABLE tmp \nAS\nSELECT\na \nFROM\nt;\nANALYZE tmp ;",
        ),
        (
            "postgresql",
            "SELECT DATEADD(day, 5, d) FROM t;",
            "SELECT (d + 5*INTERVAL'1 day') FROM t;",
        ),
        (
            "sqlite",
            "SELECT DATEADD(day, 5, d) FROM t;",
            "SELECT CAST(STRFTIME('%s', DATETIME(d, 'unixepoch', (5)||' days')) AS REAL) FROM t;",
        ),
        (
            "sqlite",
            "SELECT DATEDIFF(day, a, b) FROM t;",
            "SELECT (JULIANDAY(b, 'unixepoch') - JULIANDAY(a, 'unixepoch')) FROM t;",
        ),
        (
            "postgresql",
            "SELECT 'a' + b + 'c' FROM t;",
            "SELECT 'a' || b || 'c' FROM t;",
        ),
        (
            "sqlite",
            "SELECT ISNULL(a, 0), LEN(b), GETDATE() FROM t;",
            "SELECT COALESCE(a,0), LENGTH(b), STRFTIME('%s','now') FROM t;",
        ),
        (
            "postgresql",
            "SELECT ISNULL(a, 0), LEN(b), GETDATE() FROM t;",
            "SELECT COALESCE(a,0), CHAR_LENGTH(b), CURRENT_DATE FROM t;",
        ),
        (
            "postgresql",
            "CREATE TABLE #t (a INT, b DATETIME);",
            "CREATE TEMP TABLE t  (a INT, b TIMESTAMP);",
        ),
        (
            "sqlite",
            "CREATE TABLE #t (a VARCHAR(10), b DATETIME, c FLOAT);",
            "CREATE TEMP TABLE t  (a TEXT, b REAL, c REAL);",
        ),
        (
            "postgresql",
            "IF OBJECT_ID('tempdb..#t', 'U') IS NOT NULL DROP TABLE #t;",
            "DROP TABLE IF EXISTS t;",
        ),
        ("sqlite", "TRUNCATE TABLE main.t;", "DELETE FROM main.t;"),
        (
            "postgresql",
            "SELECT '#abc' + x FROM t;",
            "SELECT '#abc' || x FROM t;",
        ),
        (
            "sqlite",
            "SELECT '#abc' FROM t; SELECT #t.a FROM #t;",
            "SELECT '#abc' FROM t; SELECT t.a FROM temp.t;",
        ),
        (
            "postgresql",
            "CREATE CLUSTERED INDEX idx ON #t (a);",
            "CREATE INDEX idx  ON t  (a);\nCLUSTER t  USING idx ;",
        ),
        (
            "postgresql",
            "SELECT 1 -- ISNULL(a,b)\n, ISNULL(a,b);",
            "SELECT 1 -- ISNULL(a,b)\n, COALESCE(a,b);",
        ),
        (
            "postgresql",
            "SELECT 1;\n\n  \nSELECT 2;\n",
            "SELECT 1;\nSELECT 2;\n",
        ),
        (
            "sqlite",
            "SELECT CAST(x AS DATE);",
            "SELECT CAST(STRFTIME('%s', SUBSTR(CAST(x  AS TEXT), 1, 4) || '-' || SUBSTR(CAST(x  AS TEXT), 5, 2) || '-' || SUBSTR(CAST(x  AS TEXT), 7)) AS REAL);",
        ),
        (
            "postgresql",
            "SELECT LOG(a, b), LOG10(c), LOG(d);",
            "SELECT LOG(CAST((b) AS NUMERIC),CAST((a) AS NUMERIC)), LOG(10,CAST((c) AS NUMERIC)), LN(CAST((d) AS REAL));",
        ),
        (
            "sqlite",
            "ALTER TABLE t ADD a INT, b INT;",
            "ALTER TABLE t  ADD a INT; ALTER TABLE t  ADD b INT;",
        ),
        (
            "postgresql",
            "ALTER TABLE t ADD a INT, b INT, c INT;",
            "ALTER TABLE t  ADD COLUMN a INT, ADD COLUMN b INT, ADD COLUMN c INT;",
        ),
        (
            "postgresql",
            "ALTER TABLE t ALTER COLUMN abc VARCHAR(10);",
            "ALTER TABLE t  ALTER COLUMN abc TYPE  VARCHAR(10);",
        ),
        (
            "postgresql",
            "WITH a AS (SELECT 1 AS x) SELECT x INTO #r FROM a;",
            "CREATE TEMP TABLE r \nAS\nWITH a  AS (SELECT 1 AS x)  SELECT\nx \nFROM\na;\nANALYZE r ;",
        ),
        (
            "sqlite",
            "SELECT 1 UNION (SELECT 2);",
            "SELECT 1 UNION SELECT 2;",
        ),
        (
            "sqlite",
            "SELECT * FROM (VALUES (1, 2)) AS drvd(a, b);",
            "SELECT * FROM (SELECT NULL AS a, NULL AS b WHERE (0 = 1) UNION ALL VALUES (1, 2)) AS values_table;",
        ),
        (
            "postgresql",
            "SELECT * FROM (SELECT TOP 10 a FROM t) x;",
            "SELECT * FROM (SELECT  a FROM t LIMIT 10) x;",
        ),
        (
            "postgresql",
            "SELECT DISTINCT TOP 10 a FROM t;",
            "SELECT DISTINCT  a FROM t LIMIT 10;",
        ),
    ];
    for (dialect, sql, expected) in cases {
        assert_eq!(&translate(sql, dialect), expected, "{dialect}: {sql}");
    }
}

#[test]
fn temp_emulation_schema_does_not_affect_sqlite_or_postgresql() {
    let sql = "SELECT * FROM main.dbo.t;";
    for schema in [None, Some("tmp_schema")] {
        assert_eq!(
            TRANSLATOR.translate(sql, "postgresql", schema).unwrap(),
            "SELECT * FROM main.t;"
        );
    }
}

#[test]
fn sql_server_is_identity() {
    let sql = "SELECT TOP 5 a INTO #t FROM x;\n\n\n";
    assert_eq!(translate(sql, "sql server"), sql);
}

#[test]
fn other_dialects_are_unsupported() {
    let error = TRANSLATOR
        .translate("SELECT 1;", "oracle", None)
        .unwrap_err();
    assert_eq!(
        error,
        TranslateError::UnsupportedDialect("oracle".to_owned())
    );
    assert!(error.to_string().contains("oracle"));
}

#[test]
fn translator_is_send_sync_and_shareable() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Translator>();
    let results: Vec<String> = std::thread::scope(|scope| {
        (0..4)
            .map(|_| scope.spawn(|| translate("SELECT TOP 3 a FROM t;", "sqlite")))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    });
    assert!(results.iter().all(|r| r == "SELECT  a FROM t LIMIT 3;"));
}

// ----------------------------------------------------------------- check

const SESSION_NOTE: &str = "characters to prevent some DMBSs from throwing an error.";

#[test]
fn check_has_no_warnings_for_short_names() {
    assert!(TRANSLATOR.check("SELECT 1", "postgresql").is_empty());
    // Java measures the name including its leading blank, so 62 chars (63 with the blank) is the limit.
    let sql = format!("CREATE TABLE {} (a INT);", "a".repeat(62));
    assert!(TRANSLATOR.check(&sql, "postgresql").is_empty());
}

#[test]
fn check_warns_about_long_temp_table_names() {
    let name = format!("#{}", "a".repeat(54));
    let sql = format!("SELECT * INTO {name} FROM t;");
    assert_eq!(
        TRANSLATOR.check(&sql, "postgresql"),
        [format!(
            "Temp table name '{name}' is too long. Temp table names should be shorter than 55 {SESSION_NOTE}"
        )]
    );
    // One character shorter is fine.
    let sql = format!("SELECT * INTO #{} FROM t;", "a".repeat(53));
    assert!(TRANSLATOR.check(&sql, "postgresql").is_empty());
}

#[test]
fn check_warns_about_long_table_names_with_leading_blank() {
    let name = format!("{}gggg", "abcdefghij".repeat(6));
    for sql in [
        format!("CREATE TABLE {name} (a INT);"),
        format!("drop table {name};"),
        format!("TRUNCATE TABLE {name};"),
    ] {
        assert_eq!(
            TRANSLATOR.check(&sql, "postgresql"),
            [format!(
                "Table name ' {name}' is too long. Table names should be shorter than 63 {SESSION_NOTE}"
            )],
            "{sql}"
        );
    }
}

#[test]
fn check_orders_multiple_warnings_like_java_hash_sets() {
    let mut sql = String::new();
    for i in 0..12 {
        sql.push_str(&format!("SELECT 1 INTO #{}{} FROM x;\n", "t".repeat(60), i));
    }
    for (i, keyword) in [(1, "CREATE TABLE"), (2, "CREATE TABLE"), (3, "DROP TABLE")] {
        sql.push_str(&format!("{keyword} {}{} (a INT);\n", "n".repeat(70), i));
    }
    let warnings = TRANSLATOR.check(&sql, "sqlite");
    let suffix = |w: &str| -> String {
        let quoted = w.split('\'').nth(1).unwrap();
        quoted
            .trim_start_matches(['#', ' '])
            .trim_start_matches(['t', 'n'])
            .to_owned()
    };
    let temp: Vec<String> = warnings[..12].iter().map(|w| suffix(w)).collect();
    assert_eq!(
        temp,
        ["0", "1", "2", "7", "8", "9", "3", "4", "5", "11", "6", "10"]
    );
    let tables: Vec<String> = warnings[12..].iter().map(|w| suffix(w)).collect();
    assert_eq!(tables, ["3", "2", "1"]);
}
