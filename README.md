# Rust Template

A production-ready Rust 2024 TinyBus module template used by TinyHumans AI. It
ships the workspace layout, TinyBus ABI adapter, error handling, testing,
documentation, CI, and multi-platform release workflow that every new
integration in this organization starts from.

It is a two-crate cargo workspace. `crates/template-bus` is the wire contract —
member names, payload types, and the contract version, with no transport and no
behavior — and `crates/template` is the implementation, built as both an `rlib`
and the `cdylib` TinyBus loads. A host that only makes calls depends on the
contract crate alone and compiles neither the module nor `tinybus` itself.

## Use This Template

Choose **Use this template** on GitHub, create a repository, then work through
the checklist at the top of [`AGENTS.md`](AGENTS.md):

- rename the `crates/template` and `crates/template-bus` directories and the
  `name` fields in their manifests, and set the shared `description`,
  `repository`, `keywords`, and `categories`;
- update this README and the crate documentation in `crates/template/src/lib.rs`;
- replace the placeholder `greeting` module with the first real feature area, in
  both crates: the payload types in the contract, the behavior in the module;
- rename the TinyBus interface, object path, and member constants in
  `crates/template-bus/src/names/`, and the matching `provides` / `methods`
  declarations in `crates/template/src/tinybus_module/`;
- update the security contact and repository links in the community files;
- replace `ROADMAP.md` with the real plan, or delete it;
- change the license if GPL-3.0-only is not appropriate.

Search for `template` and `template_bus` to find every remaining
template-specific value.

## What You Get

| Area | What is configured |
| --- | --- |
| Layout | A cargo workspace under `crates/`, split into a dependency-light wire contract and the module that implements it; directory modules with `mod.rs` / `types.rs` / `test.rs`, a crate-wide error type, integration tests, and a runnable example |
| Lints | `unsafe_code` forbidden, `missing_docs`, clippy `all` + `pedantic`, no `unwrap`/`expect`/`panic`/`todo` in library code — all declared once in `[workspace.lints]` so every crate, local run, and CI run agree |
| CI | Format, clippy, build, test (default and all features), a run of the bundled example, an assertion that the contract crate stays transport-free, at least 90% line coverage in every source file, rustdoc with `-D warnings`, an MSRV build, and a `cargo-deny` supply-chain check |
| Release | Manual `workflow_dispatch` bump that validates, versions, tags, and creates installable native module packages for every supported platform |
| Community | Issue and pull request templates, Dependabot, contributing, security, support, and code of conduct docs |
| Agents | [`AGENTS.md`](AGENTS.md) as the single source of truth, symlinked as `CLAUDE.md`, plus a `.claude/settings.json` allowlist for the standard commands |
| Vendor | TinyBus host types and module SDK pinned as the `vendor/tinybus` build-time submodule |

## Layout

```text
Cargo.toml              # virtual workspace: members, shared metadata, lints
crates/
├── template-bus/       # the wire contract — what crosses the bus
│   ├── README.md       # why the contract is its own crate
│   └── src/
│       ├── lib.rs      # crate docs + the entire public re-export surface
│       ├── names/      # interface, object path, one constant per member
│       ├── greeting/   # payload types, one directory per family
│       │   ├── mod.rs
│       │   ├── types.rs
│       │   └── test.rs
│       └── version/    # contract version and the host bind rule
└── template/           # the module — behavior, adapter, and the cdylib
    ├── src/
    │   ├── lib.rs      # crate docs + public surface, re-exporting the contract
    │   ├── error/      # crate-wide `Error` and `Result<T>`
    │   ├── greeting/   # one directory per feature area
    │   └── tinybus_module/   # bus interface, setup, and ABI v1 exports
    ├── tests/
    │   └── public_api.rs     # integration tests against the public API only
    └── examples/
        ├── basic.rs                  # ordinary library API usage
        ├── verify_module.rs          # local dynamic-module verification
        └── verify_github_release.rs  # tagged-release download and bus call
vendor/
└── tinybus/            # pinned TinyBus git submodule
docs/
├── README.md           # documentation index and conventions
├── specs/              # behavior and architecture specifications
├── plans/              # implementation-ordered delivery plans
└── adr/                # immutable architecture decision records
```

The split is the point. A payload type describes what a frame carries; the
behavior that answers it is a different obligation. `template` depends on
`template-bus` and re-exports all of it, so `template::GreetRequest` and
`template_bus::GreetRequest` are the *same* type rather than structural twins,
and a host is never forced to choose between linking the whole module and
redefining the vocabulary. See
[`crates/template-bus/README.md`](crates/template-bus/README.md).

Within each crate, feature areas use directory modules: implementation and
exports live in `mod.rs`, substantial types move to `types.rs`, and unit tests
live in `test.rs`. [`AGENTS.md`](AGENTS.md) holds the complete repository
guidance, and `CLAUDE.md` is a symlink to it so every coding agent reads one
source of truth.

## Development

Clone with submodules, or initialize them before building:

```sh
git submodule update --init --recursive
```

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
cargo run -p template --example basic
cargo build -p template --release --lib   # produces the installable cdylib
```

Those four checks are exactly what CI runs. Optional extras:

```sh
cargo doc --no-deps --all-features   # CI builds this with RUSTDOCFLAGS="-D warnings"
cargo deny check all                 # supply-chain check; see deny.toml
cargo install cargo-llvm-cov         # once, before running the coverage gate
.github/scripts/check-file-coverage.sh 90 coverage.json
```

## Releasing

Run the **Release** workflow from the Actions tab with a `patch`, `minor`, or
`major` bump. Use `current` only to resume an interrupted release whose version
commit and tag already exist. The workflow revalidates the workspace, versions
and tags it — one `[workspace.package]` version that every member inherits —
builds `crates/template` as a TinyBus `cdylib`, and creates a GitHub release.
Assets follow `template-<version>-<platform>.<tar.gz|zip>` and contain the
native module, its SHA-256 `modules.toml`, license, and
[`MODULE.md`](MODULE.md). Every release also publishes `checksum.toml`, which
TinyBus uses to verify an archive before extraction. The workflow loads the
published Ubuntu archive through TinyBus's GitHub release API and calls its
`Greet` method before declaring the release successful. TinyBus itself is not
shipped by this repository; the pinned submodule is the build-time SDK. The stable native
matrix covers Ubuntu 22.04 and 24.04 on x86_64 and ARM64; Fedora 43 and 44 on
x86_64 and ARM64; rolling Arch Linux on its officially supported x86_64
architecture; macOS 15 and 26 on Intel and Apple Silicon; Windows Server 2022
and 2025 on x86_64; and Windows 11 on ARM64. Preview, deprecated, and unofficial
architecture images are not release gates. Do not hand-edit the version in the
root `Cargo.toml`.

## Documentation

- [`AGENTS.md`](AGENTS.md) — repository guidelines for humans and agents
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to propose a change
- [`docs/specs/`](docs/specs/README.md) — behavior and architecture specs
- [`docs/plans/`](docs/plans/README.md) — test-first implementation plans
- [`docs/adr/`](docs/adr/0001-record-architecture-decisions.md) — architecture
  decision records
- [`SECURITY.md`](SECURITY.md) — how to report a vulnerability

## License

GPL-3.0-only. See [LICENSE](LICENSE).
