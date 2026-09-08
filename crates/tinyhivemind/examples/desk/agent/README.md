# Agent tests

Unit tests for `../agent.rs`: folding one agent CLI's `--format json` event
stream into a turn.

## Files

- `test.rs` — that the fold counts `read` calls and records each written path
  once, and that `extract_post` recovers a message from both the asymmetric
  `<<<POST … POST>>>` fence the brief asks for and the symmetric
  `<<<POST>>> … <<<POST>>>` one a seat writes anyway. The second is a
  regression: taking only the first marker cost run 26 a verified result,
  which reached the room as the three characters `>>>`.
