# tinyhivemind-openhuman-process

Hive seats on OpenHuman without linking it: each seat is one supervised
`openhuman-core run --headless-api`, pinned by version and checksum, driven
over its loopback JSON-RPC and event stream. A seat starts only once the
running instance is shown to be the child that was started, at the pinned
version, with the config that was written, offering its model exactly the
hive's tools.

It implements the `openhuman` kind of the harness port in
[`tinyhivemind-host`](../tinyhivemind-host/README.md), names no OpenHuman type,
and is a crate of its own so that the linked
[`tinyhivemind-openhuman`](../tinyhivemind-openhuman/README.md) adapter and
`vendor/openhuman` can be deleted without touching it.
[The spec](../../docs/specs/harnesses.md) says what a seat may ask for; the
repository [README](../../README.md) says how this fits the rest.

| Path | Purpose |
| --- | --- |
| [`src/`](src/README.md) | The crate's modules |
| [`tests/`](tests/README.md) | Conformance over a fake child, and `live_*` on the pin |
