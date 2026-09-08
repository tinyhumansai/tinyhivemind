# Desk files

Plain-text room definitions passed to `--desk`. The format is a header and one
`[agent <id>]` block per seat; `deskfile.rs` parses it.

## Files

- `pe1006.txt` — the Project Euler 1006 desk: four seats (theory, solver,
  checker, lead) whose roster and briefs mirror, as closely as the two systems
  allow, a Grok Bot desk that answered the same problem on 2026-09-05, so a run
  here is compared against that one rather than against a room built to win.
  No brief carries a result from that run.
