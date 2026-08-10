# Golden-test harness

`assert_vector` compares deterministic numerical output with a committed,
line-oriented fixture using a caller-supplied absolute tolerance. The example integration
test uses `1e-12`; S2/S3 oracle tests can select tolerances appropriate to each
greek or surface quantity.

Normal verification is read-only and fails on an unapproved numerical change:

```bash
just golden
```

To approve an intentional change, first inspect the failure and computed change,
then regenerate the fixture and review its Git diff:

```bash
just golden-update
git diff -- rust/crates/golden-test/fixtures
just golden
```

`golden-update` sets `UPDATE_GOLDEN=1` for the focused test. Do not use that
environment variable in CI: committed fixtures should only change through an
explicit review and approval commit. The normal `just test` workspace run also
executes the golden assertion, making it part of the CI-ready test gate.
