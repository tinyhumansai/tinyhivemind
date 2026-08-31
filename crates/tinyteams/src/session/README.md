# Session projection

This module walks a host-owned, globally sequenced log without owning storage.
The `SessionLog` port returns newest-first pages through boxed futures so it is
object-safe and independent of an executor. `project_session` validates every
page, counts all raw rows against a fixed scan budget, filters one desk/channel
or direct-child thread, and returns attributed messages chronologically.

The host must treat `before` as exclusive and return `next_before` equal to a
nonempty page's oldest sequence. Empty pages end a walk. A malformed page is a
typed error rather than a reason to retry or silently accept incomplete order.
