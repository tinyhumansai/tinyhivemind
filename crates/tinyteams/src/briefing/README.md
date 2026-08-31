# Team initialization

A team briefing is ephemeral input to a model session, separate from durable
history. It identifies the viewer and desk, lists teammates deterministically,
and explains attribution and mention safety. Hosts can provide richer optional
role and description strings directly, or derive a conservative briefing from
validated core desk and roster snapshots.

`initialize_session` keeps this briefing separate from projected messages. It
therefore has no sequence number, consumes no history window, and is never
written back through this crate.
