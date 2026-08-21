# Getting started

## Reproducible local checks

The repository commits `Cargo.lock`. Use the lockfile for normal builds and
for every CI-equivalent command so contributors and CI resolve the same crate
versions:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --locked
cargo test --locked
```

Run these commands before opening a pull request. The CI workflow runs the
same sequence for pushes and pull requests.

## Updating dependencies

Dependency updates are reviewed changes: update `Cargo.toml` deliberately,
regenerate `Cargo.lock`, inspect both diffs, and run the commands above. Do
not use `--locked` while intentionally regenerating the lockfile; restore it
for validation afterward. Compatibility-sensitive A2A crates are exact pins
and must be reviewed together, including their protocol/API compatibility,
before changing any of their versions.
