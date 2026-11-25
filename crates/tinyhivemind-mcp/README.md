# tinyhivemind-mcp

Serves a seat's hive tools and its package's executable tools to a harness
that runs as a child process, over the Streamable HTTP transport of the Model
Context Protocol, on a random port of 127.0.0.1
([ADR 0036](../../docs/adr/0036-hive-tools-are-served-over-loopback-mcp.md)).

Every seat gets its own random bearer, and the bearer alone decides who is
calling. A state-changing call also names its `turn` and a `call` of its own
choosing: a call from a turn that is not running is refused, and a retry of a
call returns the first result instead of acting again. Package hooks, tool
permissions and the `autonomy` gate run here, before dispatch, so they hold
whichever harness the seat runs on.

It holds no episode state (the coordinator in
[`tinyhivemind-hives`](../tinyhivemind-hives/README.md) does), starts no
harness, and links none; `.github/scripts/assert-pure.sh` checks the last. It
opens a listening socket, so it is not one of the pure crates.

The behaviour is specified in [the harnesses spec](../../docs/specs/harnesses.md#hive-tools);
the repository [README](../../README.md) says how this fits the rest.

| Path | Purpose |
| --- | --- |
| [`src/`](src/README.md) | The crate's modules |
| [`tests/`](tests/README.md) | Loopback HTTP tests of the whole contract |
