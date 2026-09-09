# Fold the room by size, and give a joining seat the account

Linked specification: [`../specs/folding-by-size.md`](../specs/folding-by-size.md)

## Goal

Make the standing account fire when the room is *large* rather than only when it
is *long*, so a host can say "fold once the scrollback would cost 50,000 tokens"
and a seat joining a big room is always handed an account of it.

One field on `DigestPolicy`, one constructor, one extra arm in `plan_digest`.
No new port, no tokenizer, no change to what a fold may cover.

## Test-first tasks

1. Add failing tests in `crates/tinyhivemind/src/digest/test.rs` for the size
   trigger before the field exists: a channel of 24 rows under `keep_live: 30,
   fold_after: 20` that today plans `Current` and must plan `Fold` once its
   desk-visible content exceeds `fold_after_chars`; a channel of 40 short rows
   that still folds on the row trigger with `fold_after_chars: 0`; and a channel
   whose *live tail alone* is over the threshold, which must still plan
   `Current`, because the tail is never folded. Then add `fold_after_chars` to
   `DigestPolicy` and the arm to `plan_digest`.

2. Add a failing test that the character count is taken over desk-visible rows
   only — an aside of 100k characters must not trigger a fold — then implement
   it by counting through the same audience filter `collect_digest_input`
   already applies. A size trigger that counted private rows would let one aside
   spend the room's summarization budget on content the account may not contain.

3. Add failing tests for `DigestPolicy::from_token_budget(50_000)`: it sets
   `fold_after_chars` to the documented multiple, leaves the other fields at
   `DEFAULT`, and saturates rather than overflowing on `usize::MAX`. Implement
   it as a `const fn` with the ratio named as a constant and documented as a
   proxy, not a measurement.

4. Set `DigestPolicy::DEFAULT.fold_after_chars` to a generous ceiling rather
   than to `0`, and add a test pinning the value. A host that never touches the
   policy should gain a safety net, not a behavior change: the default must be
   high enough that every existing test's fixture still plans what it planned.

5. Add a failing test that a seat with no prior turns is composed a prompt
   containing the account, in `crates/tinyhivemind/tests/`, using a stub
   digester. This is acceptance criterion 4 and is the one the example currently
   satisfies by construction rather than by contract.

6. Update the doc comment on `DigestPolicy`, the `digest` module docs, and
   `crates/tinyhivemind/src/digest/README.md` to state the character proxy and
   its ratio in one place, and note that the trigger measures message content
   and not the composed prompt.

7. Wire `--fold-tokens <n>` through `examples/desk/cli.rs` to
   `from_token_budget`, defaulting to the value that would have folded run 28,
   and print the plan's reason when a fold fires so a live run says which
   trigger bound.

8. Update [`../specs/thoughts-and-channels.md`](../specs/thoughts-and-channels.md)
   to point its "should `fold_after` be measured in tokens" open question at the
   answer, and add the phase row to `ROADMAP.md`.

## Validation

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
.github/scripts/assert-pure.sh
```

Then a live run on PE 1006 with `--fold-tokens 50000`, which is what discharges
acceptance criterion 6 — no live run has ever folded.

## What this plan deliberately leaves out

- **Measuring whether the account is any good.** A lossy account that drops the
  fact the next turn needed has no error message, and nothing here counts how
  often a seat goes back to `desk_read` for the rows.
- **Per-seat accounts.** One account per channel, unchanged.
- **A smaller live window.** The size trigger makes that trade cheap to test and
  this plan does not test it.
