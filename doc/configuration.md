# Configuration

## Dependency and lockfile policy

`Cargo.lock` is part of this binary crate's reproducible build configuration.
CI builds and tests with `--locked`, so a dependency declaration change must
be accompanied by the resulting lockfile update.

The `a2a-lf`, `a2a-client-lf`, and `a2a-server-lf` dependencies are an A2A
compatibility surface and use exact version requirements in `Cargo.toml`.
Review them as a set: verify the selected versions remain mutually compatible
with the A2A protocol and server/client integration, update their exact pins
only when that review is complete, regenerate `Cargo.lock`, and inspect the
full dependency diff.

For every dependency update, run the local CI-equivalent commands:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --locked
cargo test --locked
```

Do not hand-edit `Cargo.lock`; let Cargo regenerate it from the reviewed
manifest and commit the result in the same change.
