# Template TinyBus Module

This package contains the native `template` module for TinyBus module ABI
v1. Install only the archive matching the host operating system and
architecture.

The module claims `ai.tinyhumans.template.Greeting`, serves the object at
`/ai/tinyhumans/template/Greeting`, and provides the `Greet` method. The
method accepts a `GreetRequest` and returns a `GreetResponse` carrying
`Hello, <name>!`; empty names are rejected. Both payload types, the interface
name, the object path, and the member names are published as the `template-bus`
crate, so a host names them from a library rather than by string literal.

The archive contains one `.so`, `.dylib`, or `.dll` plus `modules.toml`. Keep
those files together when copying them into a TinyBus module directory. The
allowlist binds the native library filename to its SHA-256 digest so TinyBus can
reject a missing, renamed, or modified artifact before initialization.

The GitHub release also publishes `checksum.toml` as a separate asset. TinyBus
checks that manifest before downloading and extracting the selected platform
archive. Install directly from a tagged release with:

```sh
tinybus modules load-github \
  https://github.com/tinyhumansai/rust-template/releases/tag/v0.1.5 \
  template-0.1.5-ubuntu-24.04-x86_64.tar.gz \
  <archive-sha256>
```

TinyBus modules are trusted in-process code. Install release artifacts only
from a trusted source and restart the host after replacing a loaded module.
