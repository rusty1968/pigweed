# Channel Design Asymmetry: Handler → Initiator Signaling

## Summary

The current channel implementation has an asymmetry in `raise_peer_user_signal()`:
- **Initiator → Handler**: Always works
- **Handler → Initiator**: Only works during an active transaction

This breaks the primary use case where a handler wants to notify the initiator
to start a transaction.

## Current Architecture

```
                 INITIATOR                         HANDLER
              ┌─────────────┐                   ┌─────────────┐
              │ Initiator   │                   │  Handler    │
              │   Object    │                   │   Object    │
              │             │                   │             │
              │  handler: ──┼───────────────────┼──►          │
              │  ForeignRc  │   PERMANENT REF   │             │
              └─────────────┘                   └─────────────┘
                    │                                 │
                    │     DURING TRANSACTION          │
                    │                           ┌─────▼─────┐
                    │                           │Transaction│
                    │                           │           │
                    ◄───────────────────────────┼──initiator│
                       TEMPORARY REF            │ForeignRc  │
                       (only exists during      └───────────┘
                        active transaction)
```

## The Problem

### Initiator → Handler: ✓ Always Works

```rust
// ChannelInitiatorObject
fn raise_peer_user_signal(&self, kernel: K) -> Result<()> {
    // Has permanent reference to handler
    self.handler.base.raise(kernel, Signals::USER);
    Ok(())
}
```

### Handler → Initiator: ✗ Fails Outside Transaction

```rust
// ChannelHandlerObject  
fn raise_peer_user_signal(&self, kernel: K) -> Result<()> {
    let active_transaction = self.active_transaction.lock();
    let Some(ref transaction) = *active_transaction else {
        // No active transaction = no initiator reference!
        return Err(Error::FailedPrecondition);
    };
    transaction.initiator.base.raise(kernel, Signals::USER);
    Ok(())
}
```

## Why This Matters

The primary use case for `raise_peer_user_signal()` is:

> **Handler**: "Hey initiator, I have data for you - start a transaction!"

But this requires signaling **before** a transaction exists, exactly when the
handler has no reference to the initiator.

### Timeline

```
   Handler wants to          Handler has           Transaction        Ref gone
   notify initiator          no ref to             completes          again
   to start txn              initiator!
        │                        │                      │                │
        ▼                        ▼                      ▼                ▼
   ┌─────────┐             ┌───────────┐          ┌─────────┐      ┌─────────┐
   │ BLOCKED │ ─► txn ───► │  CAN NOW  │ ─► end ─►│ BLOCKED │ ───►│ BLOCKED │
   │  No ref │   starts    │  SIGNAL   │   txn    │  No ref │      │  No ref │
   └─────────┘             └───────────┘          └─────────┘      └─────────┘
```

## Proposed Fix

Store the initiator reference at channel creation/connect time, not just during
transactions:

```
              ┌─────────────┐                   ┌─────────────┐
              │ Initiator   │                   │  Handler    │
              │   Object    │                   │   Object    │
              │             │                   │             │
              │  handler: ──┼───────────────────┼──►          │
              │  ForeignRc  │                   │             │
              │             │                   │  initiator:◄┼─── NEW!
              │         ◄───┼───────────────────┼──ForeignRc  │
              └─────────────┘   BIDIRECTIONAL   └─────────────┘
```

### Implementation Sketch

```rust
pub struct ChannelHandlerObject<K: Kernel> {
    base: ObjectBase<K>,
    active_transaction: Mutex<K, Option<Transaction<K>>>,
    // NEW: Persistent reference to connected initiator
    initiator: Option<ForeignRc<K::AtomicUsize, ChannelInitiatorObject<K>>>,
}

impl<K: Kernel> KernelObject<K> for ChannelHandlerObject<K> {
    fn raise_peer_user_signal(&self, kernel: K) -> Result<()> {
        // Use persistent reference, not transaction reference
        let Some(ref initiator) = self.initiator else {
            return Err(Error::FailedPrecondition);
        };
        initiator.base.raise(kernel, Signals::USER);
        Ok(())
    }
}
```

## Considerations

1. **1:1 vs 1:N channels**: This fix assumes 1:1 channels. For 1:N (multiple
   initiators per handler), a different approach is needed.

2. **Circular references**: With bidirectional `ForeignRc`, care is needed to
   avoid reference cycles that prevent cleanup.

3. **Connection lifecycle**: Need to define when the initiator ref is set
   (at creation? on first transaction?) and cleared.
