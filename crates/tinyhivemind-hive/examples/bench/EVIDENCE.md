# The evidence-first opening

What `--blind-evidence` changes about a member's first turn, and why a deposit
is not a position. Split out of [`README.md`](README.md) when that file passed
the repository's 500-line cap; nothing here changed with the move.


`--blind-evidence` changes one thing about the *participants* and nothing about
the library: while the room is still `Visibility::Blind`, a member's first turn
deposits `!evidence #topic` — its own reading of the topic it knows best, with
no citation, because nothing is visible to cite — instead of proposing an
option. Proposals begin once the room goes to `Visibility::Full`.

The finding it exists to state is short: **without an evidence-first opening, a
room whose members share a bias reaches quorum inside the blind round, and no
floor mechanism can act.** A `!propose` counts as a supporter, so four members
who privately favour the same planted decoy carry it before anybody has read
anybody; the episode's first non-blind turn is a commit turn, and a fact
arriving then has nothing left to change. That is not a hypothesis — the live
rooms recorded it (in every correct episode of the 2026-09-01 run the five
blind turns were five `!evidence` lines, one per member) and the federation
reached it from the other side (moving a desk's question to *before* it had
backed anything was the difference between failing outright and 77.5%).

It is off by default, so every published number that does not ask for it is
unchanged, and what it buys is measured rather than assumed:

```text
5000 rooms                        hive+   hive+dir   vote   ladder   ladder+dir
uniform                            82.1       82.1   78.5     57.6         49.5
uniform  --blind-evidence          75.3       75.7   78.5     57.6         98.8
--specialists 2                    74.2       74.2   71.1     52.6         45.1
--specialists 2 --blind-evidence   67.6       68.0   71.1     52.6         84.4
--hidden-profile                   15.3       15.3   15.0     35.1         34.6
--hidden-profile --blind-evidence  66.3       65.8   15.0     35.1         64.1
```

On an ordinary room it **costs** about seven points: five of the fifteen turns
go on deposits nobody needed, and `hive+` fails to decide 7% of the time rather
than 0.6%. On the hidden profile it is the difference between 15% and 66%. That
is the trade, stated rather than tuned away.

Two side effects are worth reading before the numbers are:

- **`ladder+dir` on a uniform room is an artifact, not a result.** The arm
  tells its router which *topic* the call turns on, and that topic is the
  correct option. With an evidence-first opening the directory records "who
  deposited a reading of `#truth`", and a member who deposited on `#truth` is
  usually a member whose favourite *is* `#truth` — so routing to the heaviest
  holder returns the right answer 98.8% of the time by construction. The
  92-point swing from the same arm's 49.5% under the ordinary opening is the
  size of the leak, not the size of the mechanism. Read the `--specialists`
  row instead, where the deposit is a specialist's tight reading rather than a
  vote, and even there read it knowing the topic was named.
- **The two refutation arms fall further.** `hive+ref` and `hive+ev` lose about
  twenty-five points under the flag. A blind round spent depositing is a blind
  round not spent proposing, and both arms already had the tightest budget.

