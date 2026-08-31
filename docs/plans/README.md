# Implementation plans

Plans turn an accepted specification into a reviewable sequence of small,
verifiable changes. They explain how to build the behavior; the linked
specification remains the source of truth for what the behavior must be.

Use the same kebab-case stem as the specification. A useful plan includes:

- a link to the accepted specification;
- the goal, non-goals, and assumptions relevant to implementation;
- ordered tasks with exact file paths;
- a failing test before each behavior change;
- the minimal implementation needed to pass that test;
- documentation and public-export updates;
- focused and full verification commands;
- a completion checklist updated as tasks land.

Prefer tasks that can be implemented and reviewed independently. Include short
code snippets when they remove ambiguity, but do not paste entire future files
into the plan.

See [`example-retry-policy.md`](example-retry-policy.md) for a test-first sample.

## Plans

- [`chat-identity.md`](chat-identity.md) — implemented P1 conversation identity.
- [`desks.md`](desks.md) — P2 desk DTOs, validation, and membership overlay.
- [`mentions.md`](mentions.md) — P3 roster and mention resolution.
- [`sessions.md`](sessions.md) — P4 attributed session projection and team
  initialization.
- [`continuous-sharing.md`](continuous-sharing.md) — P5 watermark-based
  continuous transcript sharing.
