# memory

Defines portable memory identities, lifecycle and seat read/remember requirements. Stored entries remain owned by the host memory engine.

| File | Purpose |
| --- | --- |
| types.rs | Portable wire types and their documented fields. |
| mod.rs | Public memory API and implementation. |

Read reaches use explicit namespaces within the package root. `inherit` must
be false because engine ancestor inheritance includes the global root; list
permitted ancestors as separate reaches instead. Hosts enforce read-only,
recall budgets, remembered categories and fork lifecycle from lowering.
