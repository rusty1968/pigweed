# ObjectBase Signal/Waiter Bug Fix

## Summary

Fixed a bug in `ObjectBase::signal()` where the `contains` check had its
operands reversed, and corrected the semantics to use `intersects()` per
the `wait_until()` docstring which says "any of the signals".

## The Bug

### Original code in `signal()`

```rust
if waiter.signal_mask.contains(active_signals) {
    // wake the waiter
}
```

### What was wrong

The operands were reversed. `A.contains(B)` checks if A has **all** bits in B.
The code was checking if the waiter's mask contains the active signals, which
is backwards.

### Example scenario

- A waiter calls `wait_until(READABLE)` — they want to know when readable
- The object signals `READABLE | WRITEABLE` — both conditions are active

### Why the bug caused missed wakes

```
waiter.signal_mask.contains(active_signals)
→ READABLE.contains(READABLE | WRITEABLE)
→ false  // READABLE doesn't have the WRITEABLE bit!
```

The waiter waiting for `READABLE` is **not** woken, even though `READABLE`
is now active. This is incorrect.

## The Correct Semantics: `intersects()`

Looking at the `wait_until()` docstring:

> "Blocks the current thread until **any of the signals** in `signal_mask`
> are active"

The key phrase is "any of the signals". This is `intersects()` semantics:
wake if ANY bit in the mask matches ANY active signal.

### The fix

```rust
if self.active_signals.intersects(waiter.signal_mask) {
    // wake the waiter
}
```

Now:

```
active_signals.intersects(waiter.signal_mask)
→ (READABLE | WRITEABLE).intersects(READABLE)
→ true  // they share the READABLE bit
```

The waiter is correctly woken because at least one signal they asked for
is now active.

## Why `intersects()` not `contains()`?

It depends on the desired semantics:

| Semantics | Method | Waiter for `A | B` wakes when... |
|-----------|--------|----------------------------------|
| ANY signal | `intersects()` | Either A or B is active |
| ALL signals | `contains()` | Both A and B are active |

The `wait_until()` docstring says "any of the signals", so we use `intersects()`.

### Example with `intersects()`

```rust
// Waiter wants: READABLE | ERROR (wake on error OR data available)
let mask = Signals::READABLE | Signals::ERROR;

// Only READABLE is active:
let active = Signals::READABLE;

active.intersects(mask) → true  // waiter wakes!
```

This is the correct behavior: the waiter asked to wake on READABLE **or** ERROR,
and READABLE is now active.

## Consistency with `wait_until`

The fix ensures that `signal()`/`raise()` use the same check as `wait_until()`:

```rust
// In wait_until() - early return if signals already satisfy the mask
if state.active_signals.intersects(signal_mask) {
    return Ok(...)  // any requested signal is active
}
```

The signaling side must use the same condition as the waiting side:
**"is any of the waiter's requested signals now active?"**

## Additional refactoring

As part of this fix, we also:

1. **Extracted a shared helper** — `ObjectBaseState::notify_satisfied_waiters()`
   with `#[inline(never)]` to reduce code duplication and binary size

2. **Unified `signal()` and `raise()`** — both now use the same helper after
   modifying `active_signals`:
   - `signal()`: replaces signals entirely, then notifies
   - `raise()`: ORs in new signals, then notifies

### Before

```rust
pub fn signal(&self, kernel: K, active_signals: Signals) {
    let mut state = self.state.lock(kernel);
    state.active_signals = active_signals;

    let _ = state.waiters.for_each(|waiter| -> Result<()> {
        if waiter.signal_mask.contains(active_signals) {  // BUG: reversed
            // ... wake waiter ...
        }
        Ok(())
    });
}

pub fn raise(&self, kernel: K, signals_to_raise: Signals) {
    let mut state = self.state.lock(kernel);
    state.active_signals |= signals_to_raise;

    let _ = state.waiters.for_each(|waiter| -> Result<()> {
        if waiter.signal_mask.intersects(signals_to_raise) {  // checked raised only
            // ... wake waiter ...
        }
        Ok(())
    });
}
```

### After

```rust
impl<K: Kernel> ObjectBaseState<K> {
    /// Wakes all waiters whose `signal_mask` intersects `active_signals`.
    /// "Intersects" means any bit in the mask is present in active signals.
    #[inline(never)]
    fn notify_satisfied_waiters(&mut self) {
        let _ = self.waiters.for_each(|waiter| -> Result<()> {
            if self.active_signals.intersects(waiter.signal_mask) {  // FIXED
                // ... wake waiter ...
            }
            Ok(())
        });
    }
}

pub fn signal(&self, kernel: K, active_signals: Signals) {
    let mut state = self.state.lock(kernel);
    state.active_signals = active_signals;
    state.notify_satisfied_waiters();
}

pub fn raise(&self, kernel: K, signals_to_raise: Signals) {
    let mut state = self.state.lock(kernel);
    state.active_signals |= signals_to_raise;
    state.notify_satisfied_waiters();
}
```

## Why `raise()` now checks all active signals

The original `raise()` only checked if the **newly raised** bits intersected
the waiter's mask. The unified helper checks the **current active signals**.

This is more correct because:
1. A previous `raise()` might have set some bits
2. Those bits might now satisfy a waiter that was added after the raise
3. The new `raise()` should wake waiters if their mask overlaps ANY active bit

## Why existing tests didn't catch it

### 1. Tests only verified signal state, not waiter wakes

The `object_signals.rs` tests called `signal()` and `raise()` on an
`ObjectBase` with **no registered waiters**. They verified:
- `signal()` replaces signals (behavioral)
- `raise()` ORs signals (behavioral)
- No panics occur

But they never:
- Registered a waiter via `wait_until()`
- Verified the waiter actually gets woken

### 2. Integration tests worked by accident

Real tests go through userspace IPC (channels). In channel scenarios, signals
often exactly match what the waiter requested:

```rust
// This case works even with the bug:
waiter wants: READABLE
object signals: READABLE

READABLE.contains(READABLE) → true  // works
READABLE.intersects(READABLE) → true  // also works
```

The bug only shows when **signals are broader than the wait mask**:

```rust
// This case shows the bug:
waiter wants: READABLE
object signals: READABLE | WRITEABLE

READABLE.contains(READABLE | WRITEABLE) → false  // BUG!
(READABLE | WRITEABLE).intersects(READABLE) → true  // CORRECT
```

## New tests added

We added tests in `object_signals.rs` that verify the `intersects()` semantics
directly, documenting and validating the expected behavior:

- `intersects_any_overlap_satisfies` — any shared bit wakes the waiter
- `intersects_exact_match_satisfies` — sanity check
- `intersects_partial_overlap_satisfies` — wakes even if not all bits present
- `intersects_disjoint_does_not_satisfy` — no overlap means no wake
- `intersects_any_of_mask_satisfies` — multi-signal mask behavior

These tests explicitly demonstrate that a waiter waiting for `A | B` wakes
when **either** A or B is active, matching the "any of the signals" semantics.
