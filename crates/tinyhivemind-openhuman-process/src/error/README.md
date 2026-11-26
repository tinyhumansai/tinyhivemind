# Errors

The crate's internal `Error`. At the port a refusal becomes
`harness::Error::Refused` holding the `capability::Refusal`, supervision,
mismatch and resume failures keep the port's own variants, and the rest
become `harness::Error::Harness` with their message. No message holds a token,
a bearer or a config value.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Error`, `Result` and the conversion |
| `test.rs` | Every variant through the port |
