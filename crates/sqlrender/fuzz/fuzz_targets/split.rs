#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|sql: &str| {
    let _ = dqd_sqlrender::try_split_sql(sql);
});
