# `arms/`

Tests for [`../arms.rs`](../arms.rs), the control arms the deliberation is
measured against.

| file | what it holds |
| --- | --- |
| `test.rs` | what the controls are *charged*: that a ladder taking the `Select` rung pays for the router's call as a round of its own, that one answering from the roster alone is priced exactly as before, and that a blind arm's turns each read only the operator's brief |

A control priced too cheaply makes every arm measured against it look worse
than it is, which is why the pricing has tests of its own rather than being
left to the comparison that consumes it.
