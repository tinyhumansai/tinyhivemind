# Implement TinyBus Module Releases

Linked specification: [`../specs/tinybus-module-release.md`](../specs/tinybus-module-release.md)

1. Add the pinned TinyBus host types and module SDK as path dependencies.
2. Export the template greeting behavior through TinyBus module ABI v1.
3. Exercise the declared interface over the real in-memory bus.
4. Replace TinyBus host bundles with tagged `template` module archives for
   every supported platform runner and distribution container.
5. Run the repository validation and coverage contracts, push `main`, and
   trigger a patch release.
