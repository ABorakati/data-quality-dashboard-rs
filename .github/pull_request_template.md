## Summary

<!-- What does this change, and why? Link the issue if there is one. -->

## Parity impact

<!-- Does this change any output compared with the pinned R package (DataQualityDashboard 2.9.0, commit b9d2a9b4a4736f7ea81415d974c72119b6f0bd11)?
Outputs include check SQL, check results, threshold evaluation, result JSON/CSV/table contents and SQL-only files. -->

- [ ] No output changes. Parity fixtures under `tests/oracle/generated` are unchanged, or they changed only because the fixture generator was rerun and the diff was reviewed.
- [ ] Outputs change, and the change matches the pinned R package. Explain the evidence below.
- [ ] Outputs change on purpose, and the difference is not in the R package. Describe it below and add it to the "Intentional differences" list in README.md.

Evidence or explanation:

## Checks

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace --release` passes (PostgreSQL scenarios too, if the change touches database code: see CONTRIBUTING.md)
- [ ] No file under `tests/oracle/generated` was edited by hand
- [ ] No secrets, credentials, patient-level data or large raw datasets are included
- [ ] CHANGELOG.md has an entry under "Unreleased", if the change is user-visible
- [ ] Documentation is British English and matches the behaviour
