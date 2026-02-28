# Bug Report: SysTick initialization does not account for ARMv7-M vs ARMv8-M architectural differences

## Summary

The current SysTick initialization implementation assumes interrupt masking behavior that only holds on ARMv8-M. On ARMv7-M targets, this causes a race condition where SysTick interrupts can fire before the scheduler is ready, resulting in assertion failures during boot.

The core issue is that `systick_early_init()` enables both the SysTick counter and interrupt simultaneously, relying on PRIMASK to block early interrupts. This works on ARMv8-M (PRIMASK resets to 1) but fails on ARMv7-M (PRIMASK resets to 0).

## Problem Statement

The SysTick initialization code does not cater for the fundamental architectural difference between ARMv7-M and ARMv8-M:

| Architecture | PRIMASK Reset Value | Interrupts at Boot | Current Code Behavior |
|--------------|---------------------|--------------------|-----------------------|
| ARMv7-M      | 0 (unmasked)        | Enabled            | **Race condition**    |
| ARMv8-M      | 1 (masked)          | Disabled           | Works correctly       |

The implementation implicitly depends on ARMv8-M's masked-by-default behavior and does not provide a portable solution that works across both architecture versions.

## Affected Architecture

- **ARMv7-M** (e.g., Cortex-M3, Cortex-M4, Cortex-M7)
- ARMv8-M is **not affected** due to different PRIMASK reset behavior

## Root Cause

`systick_early_init()` enables both the SysTick counter (`ENABLE=1`) and interrupt (`TICKINT=1`) simultaneously. On ARMv7-M, the architecture resets PRIMASK to 0, leaving interrupts unmasked from reset. This allows SysTick to preempt the boot path before the scheduler is initialized.

## Steps to Reproduce

1. Boot an ARMv7-M target with the current SysTick initialization code
2. Observe intermittent assertion failures when SysTick fires before scheduler initialization completes

## Expected Behavior

SysTick interrupts should not be serviced until the scheduler is ready to handle them.

## Actual Behavior

On ARMv7-M, SysTick can fire immediately after `systick_early_init()` because:

- PRIMASK resets to 0 (interrupts unmasked)
- Both ENABLE and TICKINT are set together
- COUNTFLAG triggers before RTOS initialization completes

### Race Timeline

```
Time --->

Reset  sysinit begins  systick_early_init()       COUNTFLAG set   Handler runs
  |             |                |                     |                 |
  V             V                V                     V                 V
[PRIMASK=0]-->[Stack ready]-->[ENABLE=1, TICKINT=1]-->[COUNTFLAG=1]-->[SysTick IRQ]
                                   ^
                                   |
                          Scheduler not ready --> ASSERTION FAILURE
```

## Why ARMv8-M is Immune

ARMv8-M resets both PRIMASK_S and PRIMASK_NS to 1, keeping interrupts masked until firmware explicitly clears them. This prevents the race even with identical code.

| Aspect               | ARMv7-M          | ARMv8-M          |
|----------------------|------------------|------------------|
| PRIMASK reset value  | 0 (unmasked)     | 1 (masked)       |
| Early SysTick race   | **Possible**     | Blocked          |

## Proposed Fix

Defer enabling `TICKINT` until `systick_init()` runs after the scheduler is ready:

1. **In `systick_early_init()`**: Set `ENABLE=1, TICKINT=0`
   ```rust
   csr_val = csr.read().with_enable(true).with_tickint(false);
   ```

2. **In `systick_init()`**: Set `ENABLE=1, TICKINT=1`
   ```rust
   let csr_val = systick_regs
       .csr
       .read()
       .with_enable(true)
       .with_tickint(true);
   ```

This ensures `Clock::now()` remains accurate (counter runs) while preventing premature interrupt delivery.

### Fixed Sequence

```
Reset -> systick_early_init() -> scheduler ready -> systick_init()
  |             |                      |                |
  v             v                      v                v
[PRIMASK=?]  [ENABLE=1, TICKINT=0]  [Init done]   [ENABLE=1, TICKINT=1]
                                                  Interrupts enabled intentionally
```

## References

### ARMv7-M
- **ARM®v7-M Architecture Reference Manual** (ARM DDI 0403E.e or later)
  - Section B1.4.2 "The special-purpose mask registers" — describes PRIMASK
  - Section B1.5.5 "Reset behavior" — specifies that PRIMASK resets to 0
  - Key quote: *"PRIMASK is reset to 0"*
- https://developer.arm.com/documentation/ddi0403/latest

### ARMv8-M
- **ARM®v8-M Architecture Reference Manual** (ARM DDI 0553B or later)
  - Section B3.1 — describes the PRIMASK registers for both Security states
  - Section B3.7 "Reset behavior" — specifies that both PRIMASK_S and PRIMASK_NS reset to 1
  - Key quote: *"PRIMASK_S and PRIMASK_NS are reset to 1"*
- https://developer.arm.com/documentation/ddi0553/latest

## Additional Context

See `/docs/armv7m_systick_race.md` for detailed architectural analysis.

---

# Bug Report: CALIB register validation does not account for SKEW and NOREF flags

## Summary

The `systick_init()` function unconditionally asserts that `TENMS * 100 == SYS_TICK_HZ` when the CALIB register reports a non-zero TENMS value. This fails on hardware where the CALIB register indicates the calibration value is inexact (SKEW=1) or no reference clock is present (NOREF=1).

## Problem Statement

The SysTick CALIB register contains three relevant fields:

| Field  | Description                                                   |
|--------|---------------------------------------------------------------|
| TENMS  | Calibration value for 10ms tick period (may be 0 if unknown)  |
| SKEW   | 1 = TENMS is inexact due to clock skew                        |
| NOREF  | 1 = No reference clock provided; TENMS may be unreliable      |

The current implementation only checks `TENMS > 0` before validating against `SYS_TICK_HZ`, ignoring the SKEW and NOREF flags that indicate the calibration value should not be trusted.

## Affected Targets

- **STM32F407** and other STM32F4 series (SKEW=1, NOREF=1 typical)
- **AST1030** and other vendor-specific Cortex-M implementations
- Any target where silicon vendor did not provide accurate CALIB values

## Root Cause

```rust
// Current code (main branch)
pub fn systick_init() {
    let systick_regs = Regs::get().systick;
    let ticks_per_10ms = systick_regs.calib.read().tenms();
    info!("Ticks per 10ms: {}", ticks_per_10ms as u32);
    if ticks_per_10ms > 0 {
        pw_assert::eq!(                           // <-- Fails on SKEW=1 or NOREF=1
            (ticks_per_10ms * 100) as u32,
            KernelConfig::SYS_TICK_HZ as u32
        );
    }
}
```

The assertion fires even when the hardware explicitly indicates the calibration value is unreliable.

## Steps to Reproduce

1. Build for STM32F407-Discovery or similar target with vendor-specific CALIB values
2. Boot the kernel
3. Observe assertion failure in `systick_init()` due to TENMS mismatch

## Expected Behavior

The CALIB validation should only trigger when the hardware reports a reliable calibration:
- TENMS > 0 (value present)
- SKEW = 0 (no clock skew)
- NOREF = 0 (reference clock available)

## Actual Behavior

Assertion fires unconditionally when TENMS > 0, causing boot failure on hardware with imprecise or vendor-specific calibration values.

## Proposed Fix

Check all three conditions before validating CALIB against `SYS_TICK_HZ`:

```rust
pub fn systick_init() {
    let mut systick_regs = Regs::get().systick;

    // Enable SysTick interrupt now that the kernel is initialized
    let csr_val = systick_regs
        .csr
        .read()
        .with_enable(true)
        .with_tickint(true);
    systick_regs.csr.write(csr_val);

    let calib = systick_regs.calib.read();
    let ticks_per_10ms = calib.tenms();
    info!("Ticks per 10ms: {}", ticks_per_10ms as u32);
    
    // Only validate CALIB against SYS_TICK_HZ when the hardware reports a
    // reliable value: TENMS nonzero, no skew, and a reference clock present.
    if ticks_per_10ms > 0 && !calib.skew() && !calib.noref() {
        pw_assert::eq!(
            (ticks_per_10ms * 100) as u32,
            KernelConfig::SYS_TICK_HZ as u32
        );
    }
}
```

## References

### ARMv7-M
- **ARM®v7-M Architecture Reference Manual** (ARM DDI 0403E.e)
  - Section B3.3.1 "SysTick Control and Status Register"
  - Section B3.3.4 "SysTick Calibration Value Register" — describes TENMS, SKEW, NOREF
  - Key quote: *"If SKEW is set, the TENMS value is inexact... If NOREF is set, the SYST_CALIB.TENMS field is UNKNOWN"*

### ARMv8-M
- **ARM®v8-M Architecture Reference Manual** (ARM DDI 0553B)
  - Section D1.2.4 "SysTick Calibration Value Register"

---

# Consolidated Timer Portability Summary

| Bug | Issue | Affected Arch | Root Cause | Status |
|-----|-------|---------------|------------|--------|
| 1 | SysTick IRQ race at boot | ARMv7-M | PRIMASK resets to 0; TICKINT enabled too early | Fixed in `rusty1968/fix-systick-race-armv7m` |
| 2 | CALIB validation failure | Vendor-specific HW | SKEW/NOREF flags ignored | Fix in `clockdomain/debug-stm32` (not yet in `rusty1968/fix-systick-race-armv7m`) |

Both issues stem from assuming ARMv8-M / reference-design behavior on all Cortex-M targets.

## Source Commits

- Bug 1: `b10830949 pw_kernel/arm_cortex_m: Defer SysTick interrupt enable to init`
- Bug 2: Same commit includes CALIB fix (in `clockdomain/debug-stm32`)

---

# Upstream Contribution Plan

## Prerequisites (One-time Setup)

1. **Sign the CLA**
   - Visit https://cla.developers.google.com/
   - Sign with the same email as your Gerrit account

2. **Set up Gerrit credentials**
   ```bash
   # Get login cookie
   open https://pigweed.googlesource.com/new-password
   ```

3. **Install Gerrit commit hook** (if not already done)
   ```bash
   f=`git rev-parse --git-dir`/hooks/commit-msg && mkdir -p $(dirname $f) && \
   curl -Lo $f https://gerrit-review.googlesource.com/tools/hooks/commit-msg && chmod +x $f
   ```

4. **Add upstream remote** (if using GitHub fork)
   ```bash
   git remote add upstream https://pigweed.googlesource.com/pigweed/pigweed
   git fetch upstream
   ```

## Change Preparation

### CL 1: SysTick Race Condition Fix (Bug 1)

**Status:** Ready on `rusty1968/fix-systick-race-armv7m` branch

**Commit message format:**
```
pw_kernel: Fix SysTick race condition on ARMv7-M

Defer enabling TICKINT until systick_init() after the scheduler is ready.

On ARMv7-M, PRIMASK resets to 0 leaving interrupts unmasked at boot.
If TICKINT is enabled in systick_early_init(), SysTick can fire before
the scheduler is initialized, causing assertion failures.

ARMv8-M is not affected because PRIMASK resets to 1, keeping interrupts
masked until firmware explicitly enables them.

Changes:
- systick_early_init(): Set ENABLE=1, TICKINT=0 (counter runs for Clock::now())
- systick_init(): Set ENABLE=1, TICKINT=1 (after scheduler ready)

Bug: b/<ISSUE_NUMBER>
Test: Boot on STM32F407-Discovery (ARMv7-M) without SysTick assertion failure
```

### CL 2: CALIB Register Validation Fix (Bug 2)

**Status:** Needs cherry-pick from `clockdomain/debug-stm32`

**Commit message format:**
```
pw_kernel: Relax SysTick CALIB validation for vendor-specific hardware

Only assert TENMS against SYS_TICK_HZ when the hardware indicates a
reliable calibration value (SKEW=0, NOREF=0).

Many targets (STM32F4 series, AST1030) report SKEW=1 or NOREF=1,
indicating the TENMS value is inexact or no reference clock is present.
The current assertion fails on these platforms.

Bug: b/<ISSUE_NUMBER>
Test: Boot on STM32F407-Discovery without CALIB assertion failure
```

## Submission Steps

1. **Rebase on latest main**
   ```bash
   git fetch upstream main
   git rebase upstream/main
   ```

2. **Verify commit message has Change-Id** (auto-added by hook)
   ```bash
   git log -1  # Should show Change-Id in footer
   ```

3. **Push to Gerrit**
   ```bash
   git push upstream HEAD:refs/for/main
   ```

4. **Add reviewers**
   - `pw_kernel` owners: `konkers@google.com`, `travisg@google.com`
   - Or add `gwsq-pigweed@pigweed.google.com.iam.gserviceaccount.com` for auto-assignment

5. **Request presubmit dry run**
   - Ask a committer to kick off presubmit checks
   - Fix any lint/format/test failures

6. **Address review feedback**
   ```bash
   # Make changes, then amend (not new commit)
   git commit --amend
   git push upstream HEAD:refs/for/main
   ```

7. **Merge** (requires 2 committer approvals)

## Decision: Single CL vs Two CLs

| Approach | Pros | Cons |
|----------|------|------|
| **Single CL** (both fixes) | Simpler review, one merge | Harder to bisect if issues arise |
| **Two CLs** (separate) | Clean separation, easier rollback | More review overhead |

**Recommendation:** Submit as **two separate CLs**. The fixes address different root causes (PRIMASK behavior vs CALIB flags) and affect different code paths.

## File to Update

- `pw_kernel/arch/arm_cortex_m/timer.rs`

## Testing Checklist

- [ ] Boot test on ARMv7-M target (e.g., STM32F407, Cortex-M4)
- [ ] Boot test on ARMv8-M target (to verify no regression)
- [ ] Verify `Clock::now()` returns correct values after fix
- [ ] Run any existing pw_kernel unit tests
