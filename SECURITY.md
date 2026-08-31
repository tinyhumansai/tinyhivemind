# Security Policy

## Supported Versions

This crate is pre-1.0. Security fixes target the `main` branch until the
project starts maintaining release branches.

## Reporting A Vulnerability

Please do not open a public issue for a suspected vulnerability.

Use GitHub's [private vulnerability reporting][gh-pvr] for this repository, or
email `security@tinyhumans.ai`. Include:

- a description of the issue;
- affected versions or commits;
- reproduction steps or a proof of concept;
- an impact assessment;
- any suggested fix or mitigation.

We will acknowledge reports as quickly as practical and coordinate disclosure
before publishing details.

[gh-pvr]: https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability

## Scope

In scope:

- memory-safety or soundness issues in this crate;
- unsafe handling of credentials, secrets, or user data;
- input handling that allows unintended file, process, or network access;
- dependency vulnerabilities with a practical exploit path through this crate's
  public API.

Generally out of scope:

- vulnerabilities in downstream applications that use this crate incorrectly;
- issues that require an attacker to already control the host;
- advisories in dependencies with no reachable path from this crate.

## Practices

- `unsafe` code is forbidden crate-wide by the lint configuration in
  `Cargo.toml`. Relaxing that is a deliberate, reviewed change.
- CI runs `cargo-deny` against advisories, licenses, bans, and sources on every
  push and pull request.
- Dependabot proposes weekly Cargo and GitHub Actions updates.
- Secrets never enter the repository. `.env` is git-ignored, and
  `.env.example` documents variables with placeholder values only.
