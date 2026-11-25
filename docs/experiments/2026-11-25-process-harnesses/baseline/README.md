# Linked baseline

| File | Does |
| --- | --- |
| `main.rs` | one linked OpenHuman agent with a native `hivemind_post`, eight plain and eight tool turns, the medians of turns 4 to 8, and its own memory |
| `manifest.sh` | `manifest.sh <repo root> <out dir>` writes the crate: features, `lto = "thin"`, `codegen-units = 1`, and the repository's patch tables and lock file |

The pin it links is OpenHuman `5f8b7c23`. `a5ed63fc`, the branch's base, has that
submodule commit. Build in release, with the target directory under the
repository's own `target/`:

```sh
$D/baseline/manifest.sh . target/repro/baseline
cd target/repro/baseline
cargo build --release --target-dir ../baseline-target
env -i PATH=/usr/bin:/bin HOME="$PWD/home" ../baseline-target/release/baseline http://127.0.0.1:19501
```

The argument is the fixture's base URL. Run it three times. The spike's exact shell
line for this program was not saved; the cleared environment and private `HOME`
above are so it cannot read a real login. The lock file is a
copy of the repository's, so unrelated patch versions may differ from the spike's
own lock.

Built from `manifest.sh` and run once on 2026-11-25, on the same loaded machine:
boot 1054 ms, idle 64 MB RSS and 62 PSS, plain turn 104 ms, tool turn 119 ms, 188 MB
RSS and 185 PSS after 16 turns.
