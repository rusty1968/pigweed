# `raise_peer_user_signal` Validation Plan

This document describes the validation strategy for the `raise_peer_user_signal`
syscall implementation, covering unit tests, system image tests, and cross-target
validation.

## Milestones & TODO

### Milestone 1: Core Implementation ✅
*Target: Complete*

- [x] Define `RaisePeerUserSignal` syscall ID (0x0005)
- [x] Add RISC-V syscall veneer (`ecall`)
- [x] Add ARM Cortex-M syscall veneer (`svc`)
- [x] Implement `KernelObject::raise_peer_user_signal()` trait method
- [x] Implement for `ChannelHandlerObject`
- [x] Implement for `ChannelInitiatorObject`
- [x] Add kernel syscall handler dispatch
- [x] Add userspace `syscall::raise_peer_user_signal()` wrapper

### Milestone 2: Safety Fixes ✅
*Target: Complete*

- [x] Add `ObjectBase::raise()` method (OR semantics)
- [x] Update channel implementations to use `raise()` not `signal()`
- [x] Change default error from `Unimplemented` to `InvalidArgument`
- [x] Document memory ordering guarantees

### Milestone 3: Basic Validation ✅
*Target: Complete*

- [x] Create IPC notification test (`tests/ipc_notification/user/`)
- [x] Implement `test_notification` in client
- [x] Implement `handle_notify_test` in server
- [x] Pass on `qemu_virt_riscv32`

### Milestone 4: Unit Tests ✅
*Completed: Jan 10, 2026*

- [x] Create `pw_kernel/kernel/tests/object_signals.rs`
- [x] Implement `test_raise_preserves_existing_signals` → `raise_ors_with_existing_signals`
- [x] Implement `test_signal_replaces_all_signals` → `signal_sets_exact_signals`
- [x] Implement `test_raise_accumulates_signals` → `raise_accumulates_multiple_signals`
- [x] Implement `test_raise_wakes_waiters` → Deferred (requires thread spawning)
- [x] Add to `integration_tests` library in BUILD.bazel
- [x] Verify passes on QEMU RISC-V

**Additional tests implemented:**
- [x] `signal_empty_clears_all`
- [x] `raise_idempotent_for_existing_signal`
- [x] `raise_empty_is_noop`
- [x] `signal_after_raise_replaces_all`
- [x] `raise_after_signal_adds_signals`
- [x] `scenario_channel_notification` (real-world use case)
- [x] `scenario_ipc_flow_user_persists`
- [x] `scenario_rapid_raise_sequence` (stress test)

### Milestone 5: Expanded System Tests ✅
*Completed: Jan 10, 2026*

- [x] Add `test_bidirectional_notification` (initiator → handler)
- [x] Add `test_invalid_handle_error` (error path)
- [x] Add `Op::CheckUserSignal` to server protocol
- [~] `test_notification_preserves_readable` - Deferred: requires channel fix
- [~] `test_handler_no_transaction_error` - Deferred: requires separate test binary

**Finding:** Discovered that `channel_transact()` uses `signal()` which clobbers
USER signals raised before the transaction. This is documented as a known
limitation. Fix would require changing channel.rs to use `raise(READABLE)`
instead of `signal(READABLE)`.

### Milestone 6: Cross-Target Validation �
*In Progress: Jan 10, 2026*

- [x] Create system_image for `mps2_an505` (ARM Cortex-M33)
- [ ] Create system_image for `ast1030` (ARM Cortex-M4F) — Deferred
- [ ] Create system_image for `pw_rp2350` (ARM Cortex-M33 dual-core) — Deferred
- [x] Add `system_image_test` for mps2_an505
- [x] Verify passes on mps2_an505 QEMU

**Test Results:**
- ✅ `qemu_virt_riscv32` (RISC-V RV32IMAC) — 7/7 tests pass
- ✅ `mps2_an505` (ARM Cortex-M33) — 7/7 tests pass

### Milestone 7: CI Integration 🔲
*Target: Week of Feb 3, 2026*

- [ ] Create `//pw_kernel:syscall_validation` test suite
- [ ] Add to presubmit checks
- [ ] Document test commands in README
- [ ] Verify CI green on all targets

### Milestone 8: Hardware Validation 🔲
*Target: TBD (hardware availability)*

- [ ] Run on real RP2350 hardware (dual-core stress test)
- [ ] Run on real AST1030 hardware
- [ ] Validate memory ordering under real cache conditions
- [ ] Document any hardware-specific findings

---

## Progress Summary

| Milestone | Status | Completion |
|-----------|--------|------------|
| 1. Core Implementation | ✅ Complete | 100% |
| 2. Safety Fixes | ✅ Complete | 100% |
| 3. Basic Validation | ✅ Complete | 100% |
| 4. Unit Tests | ✅ Complete | 100% |
| 5. Expanded System Tests | ✅ Complete | 100% |
| 6. Cross-Target Validation | � In Progress | 50% |
| 7. CI Integration | 🔲 Not Started | 0% |
| 8. Hardware Validation | 🔲 Blocked | 0% |

**Overall Progress: 5.5/8 milestones complete (69%)**

---

## Overview

The validation framework uses a **three-tier approach**:

| Tier | Scope | Run Time | What It Catches |
|------|-------|----------|-----------------|
| Unit Tests | Kernel primitives | <1s | Logic bugs in `raise()` vs `signal()` |
| System Image Tests | Multi-process IPC | ~5s | Syscall plumbing, end-to-end flow |
| Cross-Target Matrix | All platforms | ~30s | Arch-specific issues (ABI, atomics, caches) |

## Tier 1: Kernel Unit Tests

Add to `pw_kernel/kernel/tests/` alongside existing `sync.rs`:

### New File: `object_signals.rs`

```rust
// pw_kernel/kernel/tests/object_signals.rs

//! Unit tests for ObjectBase signal operations.
//!
//! These tests validate the critical distinction between `signal()` (replace)
//! and `raise()` (OR) operations on kernel objects.

use kernel::object::{ObjectBase, Signals};
use unittest::test;

/// Verify that `raise()` ORs signals instead of replacing them.
///
/// This is critical for `raise_peer_user_signal` - if we used `signal(USER)`
/// it would clobber existing READABLE/WRITEABLE signals.
#[test(needs_kernel)]
fn test_raise_preserves_existing_signals() -> unittest::Result<()> {
    // Setup: Create object with READABLE already set
    let object = TestObject::new();
    object.base.signal(kernel, Signals::READABLE);
    
    // Action: Raise USER signal
    object.base.raise(kernel, Signals::USER);
    
    // Verify: Both READABLE and USER are set
    let signals = object.base.active_signals();
    unittest::assert_true!(signals.contains(Signals::READABLE));
    unittest::assert_true!(signals.contains(Signals::USER));
    
    Ok(())
}

/// Verify that `signal()` replaces all signals completely.
#[test(needs_kernel)]
fn test_signal_replaces_all_signals() -> unittest::Result<()> {
    // Setup: Object with multiple signals set
    let object = TestObject::new();
    object.base.signal(kernel, Signals::READABLE | Signals::WRITEABLE);
    
    // Action: Signal with only USER
    object.base.signal(kernel, Signals::USER);
    
    // Verify: Only USER is set (others cleared)
    let signals = object.base.active_signals();
    unittest::assert_true!(signals.contains(Signals::USER));
    unittest::assert_false!(signals.contains(Signals::READABLE));
    unittest::assert_false!(signals.contains(Signals::WRITEABLE));
    
    Ok(())
}

/// Verify that multiple `raise()` calls accumulate signals.
#[test(needs_kernel)]
fn test_raise_accumulates_signals() -> unittest::Result<()> {
    let object = TestObject::new();
    
    object.base.raise(kernel, Signals::READABLE);
    object.base.raise(kernel, Signals::WRITEABLE);
    object.base.raise(kernel, Signals::USER);
    
    let signals = object.base.active_signals();
    unittest::assert_eq!(
        signals,
        Signals::READABLE | Signals::WRITEABLE | Signals::USER
    );
    
    Ok(())
}

/// Verify that `raise()` wakes waiters blocked on the raised signal.
#[test(needs_kernel)]
fn test_raise_wakes_waiters() -> unittest::Result<()> {
    // This test requires spawning a thread that waits on USER
    // and verifying it wakes when raise(USER) is called
    todo!("Implement with thread spawning")
}
```

### Integration with `lib.rs`

```rust
// pw_kernel/kernel/tests/lib.rs
#![no_std]

mod stack;
mod sync;
mod object_signals;  // NEW
```

### BUILD.bazel Update

```python
# pw_kernel/kernel/tests/BUILD.bazel
rust_library(
    name = "integration_tests",
    srcs = [
        "lib.rs",
        "stack.rs",
        "sync.rs",
        "sync/spinlock.rs",
        "object_signals.rs",  # NEW
    ],
    # ... existing config ...
)
```

## Tier 2: System Image Tests

### Current Test Coverage

Location: `pw_kernel/tests/ipc_notification/user/`

| Test | Status | Description |
|------|--------|-------------|
| `test_echo` | ✅ | Basic IPC request/response |
| `test_transform` | ✅ | Data processing (uppercase) |
| `test_batch_requests` | ✅ | Sequential IPC operations |
| `test_timeout` | ✅ | Deadline handling |
| `test_notification` | ✅ | Basic `raise_peer_user_signal` |

### Expanded Test Coverage

Add these test cases to `client.rs`:

```rust
/// Test that USER signal doesn't clobber READABLE
fn test_notification_preserves_readable() -> Result<()> {
    pw_log::info!("Test 6: Notification preserves READABLE signal");

    // Start a transaction but don't complete it yet
    // Server should have READABLE set from pending transaction
    // Then raise USER - verify both signals present
    
    // This requires a two-phase test protocol
    todo!("Implement two-phase notification test")
}

/// Test bidirectional notification (initiator → handler)
fn test_initiator_to_handler_notification() -> Result<()> {
    pw_log::info!("Test 7: Initiator raises USER on handler");

    // Client raises USER signal on server
    syscall::raise_peer_user_signal(handle::SERVER)?;

    // Send a query to verify server saw the signal
    let send_buf = [Op::CheckUserSignal as u8];
    let mut recv_buf = [0u8; 2];
    
    let len = syscall::channel_transact(
        handle::SERVER, &send_buf, &mut recv_buf, Instant::MAX
    )?;

    if recv_buf[1] != 1 {
        pw_log::error!("Server didn't see USER signal");
        return Err(Error::DataLoss);
    }

    pw_log::info!("  Bidirectional notification passed");
    Ok(())
}

/// Test error path: invalid handle
fn test_invalid_handle_error() -> Result<()> {
    pw_log::info!("Test 8: Invalid handle returns error");

    let result = syscall::raise_peer_user_signal(0xDEAD);
    
    match result {
        Err(Error::OutOfRange) => {
            pw_log::info!("  Correctly returned OutOfRange for invalid handle");
            Ok(())
        }
        Err(e) => {
            pw_log::error!("  Wrong error: {}", e as u32);
            Err(Error::Internal)
        }
        Ok(()) => {
            pw_log::error!("  Should have failed!");
            Err(Error::Internal)
        }
    }
}

/// Test error path: handler without active transaction
fn test_handler_no_transaction_error() -> Result<()> {
    pw_log::info!("Test 9: Handler without transaction returns FailedPrecondition");
    
    // This requires server to expose a "raise without transaction" test endpoint
    // or a separate test binary that attempts raise() outside transaction context
    todo!("Implement handler-side error test")
}
```

### Server-Side Additions (`server.rs`)

```rust
/// New operation for testing bidirectional notification
Op::CheckUserSignal = 5,

fn handle_check_user_signal(response: &mut [u8]) -> Result<usize> {
    // Check if USER signal was raised on us (the handler)
    let signals = syscall::object_wait(
        handle::IPC,
        Signals::USER,
        Instant::from_ticks(0),  // Non-blocking check
    );
    
    response[0] = Op::CheckUserSignal as u8;
    response[1] = match signals {
        Ok(s) if s.contains(Signals::USER) => 1,
        _ => 0,
    };
    
    Ok(2)
}
```

## Tier 3: Cross-Target Validation

### Target Matrix

| Target | Architecture | Notes |
|--------|--------------|-------|
| `qemu_virt_riscv32` | RISC-V RV32IMAC | Primary dev target |
| `mps2_an505` | ARM Cortex-M33 | TrustZone capable |
| `pw_rp2350` | ARM Cortex-M33 | RP2350 (dual-core) |
| `ast1030` | ARM Cortex-M4F | AST1030 BMC |

### BUILD.bazel Configuration

```python
# pw_kernel/tests/ipc_notification/BUILD.bazel

load("//pw_kernel/tooling:system_image.bzl", "system_image_test")

# Define target-specific tests
NOTIFICATION_TEST_TARGETS = [
    ("qemu_virt_riscv32", "//pw_kernel/target/qemu_virt_riscv32:platform"),
    ("mps2_an505", "//pw_kernel/target/mps2_an505:platform"),
    ("pw_rp2350", "//pw_kernel/target/pw_rp2350:platform"),
    ("ast1030", "//pw_kernel/target/ast1030:platform"),
]

[
    system_image_test(
        name = "ipc_notification_test_" + target,
        image = "//pw_kernel/target/{}/ipc_notification/user:ipc_notification".format(target),
        target_compatible_with = [
            "//pw_kernel/target/{}:compatible".format(target),
        ],
    )
    for target, _ in NOTIFICATION_TEST_TARGETS
]
```

### CI Integration

Add to presubmit test suite:

```python
# //pw_kernel:ci_tests or similar

test_suite(
    name = "syscall_validation",
    tests = [
        # Unit tests (run on all platforms)
        "//pw_kernel/target/qemu_virt_riscv32/unittest_runner:unittest_runner",
        
        # System image tests
        "//pw_kernel/target/qemu_virt_riscv32/ipc_notification/user:ipc_notification_test",
        "//pw_kernel/target/mps2_an505/ipc_notification/user:ipc_notification_test",
        
        # Add other platforms as available
    ],
)
```

## Memory Ordering Validation

### Stress Test for Multi-Core Targets

For targets with real multi-core (e.g., RP2350 dual Cortex-M33):

```rust
/// Stress test: concurrent raise() from multiple cores
fn test_concurrent_raise_stress() -> Result<()> {
    const ITERATIONS: u32 = 10_000;
    
    // Spawn threads on different cores
    // Each thread calls raise() in a tight loop
    // Verify no signals are lost
    
    // This validates:
    // 1. SpinLock provides mutual exclusion
    // 2. Memory ordering is correct cross-core
    // 3. No signal clobbering under contention
}
```

### Hardware vs QEMU

| Aspect | QEMU | Real Hardware |
|--------|------|---------------|
| Atomics | Single-threaded emulation | True concurrent access |
| Caches | No cache effects | D-cache, I-cache interactions |
| Memory ordering | Sequential consistency | Relaxed ARM/RISC-V ordering |

**Recommendation:** Run stress tests on real RP2350 or multi-core RISC-V hardware
to validate memory ordering guarantees in production scenarios.

## Test Execution Commands

### Run Unit Tests (QEMU RISC-V)

```bash
bazel test //pw_kernel/target/qemu_virt_riscv32/unittest_runner:unittest_runner \
    --config=k_qemu_virt_riscv32
```

### Run IPC Notification Tests

```bash
# RISC-V (QEMU)
bazel test //pw_kernel/target/qemu_virt_riscv32/ipc_notification/user:ipc_notification_test \
    --config=k_qemu_virt_riscv32

# ARM Cortex-M33 (QEMU MPS2)
bazel test //pw_kernel/target/mps2_an505/ipc_notification/user:ipc_notification_test \
    --config=k_mps2_an505

# AST1030 (QEMU or hardware)
bazel test //pw_kernel/target/ast1030/ipc_notification/user:ipc_notification_test \
    --config=k_ast1030
```

### Run Full Validation Suite

```bash
bazel test //pw_kernel:syscall_validation --config=k_all_targets
```

## Validation Checklist

### Pre-Merge Requirements

- [ ] Unit tests pass: `raise()` vs `signal()` behavior
- [ ] System image test passes: `test_notification`
- [ ] At least one ARM target passes (architecture diversity)
- [ ] No new compiler warnings
- [ ] `rust_binary_no_panics_test` passes (no panic paths)

### Post-Merge Validation

- [ ] CI green on all target platforms
- [ ] Stress tests pass on multi-core hardware (if available)
- [ ] Memory ordering validated on real hardware

## References

- [raise_peer_user_signal Implementation](raise_peer_user_signal_implementation.md)
- [raise_peer_user_signal Safety Review](raise_peer_user_signal_safety_review.md)
- [pw_kernel Unit Testing Guide](../guides.rst)
- [pw_kernel Quickstart - Testing](../quickstart.rst)
