# Team initialization

A team briefing is ephemeral input to a model session, separate from durable
history. It identifies the viewer and desk, lists teammates deterministically,
and explains attribution and mention safety. Hosts can provide richer optional
role and description strings directly, or derive a conservative briefing from
validated core desk and roster snapshots.

The rendered rules are P7-aware: a direct `@agent` mention may cause at most
one child turn only when host policy enables it, while person, desk, and
`@everyone` mentions remain context and never fan out. The briefing does not
carry or choose that policy. The host supplies a finite configurable
`max_hops` for each dispatch decision, and the library imposes no smaller hard
ceiling.

`initialize_session` keeps this briefing separate from projected messages. It
therefore has no sequence number, consumes no history window, and is never
written back through this crate.
