#![no_main]
use libfuzzer_sys::fuzz_target;
use std::sync::LazyLock;

static TRANSLATOR: LazyLock<dqd_sqlrender::Translator> =
    LazyLock::new(dqd_sqlrender::Translator::new);

fuzz_target!(|sql: &str| {
    for dialect in ["sqlite", "postgresql", "sql server"] {
        let _ = TRANSLATOR.translate(sql, dialect, None);
    }
    let _ = TRANSLATOR.translate(sql, "sqlite", Some("scratch"));
});
