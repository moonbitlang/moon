# Use One Handle Namespace

Independent handle allocators can issue the same guest-visible number for
unrelated objects. Moonrun uses one shared generational Handle namespace owned
by the Runtime, while each Host Domain owns its payloads. This prevents
cross-family collisions and rejects stale Handles while preserving separate
domain implementations.
