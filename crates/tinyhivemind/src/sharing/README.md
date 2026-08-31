# Continuous sharing

This module prepares attributed transcript additions between a caller-owned
watermark and the next triggering sequence. It reuses the session module's log
port, page validation, and channel/thread filtering.

The module stores nothing. A host owns `SharingState`, calls `prepare_delta`,
accepts the returned messages and current trigger into its agent session, and
only then commits `next_state` under its own serialization or compare-and-swap.
A failed acceptance or lost CAS leaves the prior state reusable for a safe
retry. Reinitialization directs the host back through P4 briefing and history.
Restored state is validated during deserialization, while every public
operation independently rejects a manually constructed present set over the
64-entry bound before reading or mutating anything.

Every raw row counts against the bounded scan even when it belongs to another
conversation. Sequence gaps are valid because the host sequence is global.
The walk succeeds only after observing a row at or below the old watermark;
otherwise it distinguishes an excessive gap from unavailable retained history.
