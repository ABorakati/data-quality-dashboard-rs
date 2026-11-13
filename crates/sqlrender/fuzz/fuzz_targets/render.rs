#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    template: String,
    params: Vec<(String, String)>,
}

fuzz_target!(|input: Input| {
    let names: Vec<&str> = input
        .params
        .iter()
        .take(4)
        .map(|(n, _)| n.as_str())
        .collect();
    let values: Vec<&str> = input
        .params
        .iter()
        .take(4)
        .map(|(_, v)| v.as_str())
        .collect();
    let _ = dqd_sqlrender::try_render_sql(&input.template, &names, &values);
    // Same template with conventional names so `{@x}?{..}:{..}` conditionals fire.
    let _ = dqd_sqlrender::try_render_sql(
        &input.template,
        &["x", "y", "z"],
        &[
            values.first().copied().unwrap_or("1"),
            values.get(1).copied().unwrap_or("0"),
            values.get(2).copied().unwrap_or("a,b"),
        ],
    );
});
