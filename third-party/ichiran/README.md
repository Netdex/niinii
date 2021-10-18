# ichiran-rs
Rust wrapper around ichiran-cli.

## Testing
Unit tests assume a fixed location for ichiran-cli and postgres relative to the
project root. 

Tests must not be run in parallel since they share a database.
```
cargo test -- --test-threads 1
```