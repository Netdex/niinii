# openai
Rust wrapper around OpenAI compatible APIs.

## Testing
```
export OPENAI_KEY="sk_..."
cargo test
RUST_LOG=trace cargo test test_stream -- --nocapture
```
