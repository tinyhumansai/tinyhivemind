# multi_hive packages

Two packages, deployed together on one runtime with
`HiveDeployment::from_packages`, each one hive. They share the `shared` seat
and the same runtime, provider and model, which a shared seat requires.
See the [package index](../README.md).

| Directory | Hive | Seats |
| --- | --- | --- |
| `hive-a/` | `a` (Implementation) | `shared` as `implementer`, `architect` as `planner`, `helper` as `assistant` |
| `hive-b/` | `b` (Review) | `shared` as `observer` under the `readonly` permission set, `reviewer` as `reviewer` |

Every seat has 8 iterations. `shared` declares the same prompt, model and
limits in both packages; only its role, `permissions` and `context` differ.

`hive-b` declares `context/review.md` for `shared`, as the retired manifest
gave that membership. The adapter currently builds a shared seat's prompt from
the first package that declares it (`hive-a` here) and gives a later hive's
membership no context, so this note does not yet reach the seat's prompt.
