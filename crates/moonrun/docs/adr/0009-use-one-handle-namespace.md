# Use One Handle Namespace

Moonrun will use a single Handle namespace owned by the Async Host. Guest-visible Handles for Resources, Jobs, Workers, poll instances, Host Buffers, address-info results, Completion Sources, and similar moonrun objects are allocated from one primary `SlotMap<HandleKey, HandleKind>`, so a Resource Handle cannot accidentally also be a Job or poll handle.

Payload storage should not mint identity. Family-specific state lives in `SecondaryMap<HandleKey, Payload>` tables, and lookup-only structures such as Windows overlapped-pointer maps are secondary indexes back to Handles. The Handle table validates the expected `HandleKind` before any payload map is touched; generation bits on `HandleKey` still reject stale Handles after removal.

Resource policy is a Resource-layer concern, not a reason to split the Handle namespace. Files, TCP sockets, UDP sockets, and future resource families have different operation permissions; the current Resource payload carries a Resource Class, and future sandbox policy can extend that payload with capability metadata. Wrong-class operations are rejected after the Handle table has validated that the ABI value is a Resource Handle and before moonrun calls into the OS.

Workers may acquire Resources before running and may create new OS Resources such as open-file results, but publishing guest-visible Handles belongs to the guest-thread event-loop side. An open worker returns a completed Job containing an unpublished Resource, and the Async Host publishes the Resource Handle when the result is observed; worker threads must not mutate the Handle table.

The single namespace does not require one giant payload enum or a long-lived full table lock. Keep payload storage split where that improves locality, but allocate and free Handles through the Handle table under short locks. Resolve Resource Handles into Acquired Resources before blocking work and run Jobs on those references.
