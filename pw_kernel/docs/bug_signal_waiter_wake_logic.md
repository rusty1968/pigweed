# Bug: ObjectBase::signal() waiter wake logic uses wrong condition

**What were you trying to do:**

Wait on multiple signals (e.g., `READABLE | ERROR`) on a kernel object, expecting to wake when ANY of those signals becomes active.

**Steps followed:**

1. Thread A calls `wait_until(READABLE | ERROR)` on a channel object
2. Thread B calls `channel_transact()` which internally calls `signal(READABLE)` 
3. Thread A should wake because READABLE is now active

**Expected result:**

Thread A wakes up because READABLE (one of the requested signals) is now active. The `wait_until()` docstring says "blocks until any of the signals in signal_mask are active".

**Actual result:**

Thread A never wakes. The check in `ObjectBase::signal()` uses:

```rust
if waiter.signal_mask.contains(active_signals)
```

This evaluates to:

```
(READABLE | ERROR).contains(READABLE) → false
```

The operands are reversed. The mask (0x5) doesn't contain all bits of active (0x1) because that's checking the wrong direction.

**Root cause:**

The condition should use `intersects()` to match "any of" semantics:

```rust
if self.active_signals.intersects(waiter.signal_mask)
```

Or if `contains()` is intended, the operands should be:

```rust
if active_signals.contains(waiter.signal_mask)  // all requested bits are present
```

**Impact:**

Any wait operation using a multi-signal mask (OR'd signals) will fail to wake when only a subset of those signals is raised. This affects error handling patterns where code waits for `READABLE | ERROR`.

**Host environment:**

Linux, Bazel build

**Target Device:**

QEMU RISC-V 32-bit (qemu_virt_riscv32), likely affects all targets

**Affected code:**

`pw_kernel/kernel/object.rs` - `ObjectBaseState::notify_satisfied_waiters()` (previously inline in `signal()`)
