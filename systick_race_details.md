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
