# Deployment plan

The plan types and their validation are `tinyhivemind-host`'s
([README](../../../tinyhivemind-host/src/config/README.md)). `mod.rs` re-exports
them crate-internally and implements `HarnessRules` for the linked OpenHuman: its
cron parser and its built-in agent ids. `Validate::validate` applies them.

`test.rs` checks a package's plan under those rules.
