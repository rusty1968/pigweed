**Title:** pw_kernel: SysTick CALIB validation should not block boot

**What were you trying to do:**
Boot pw_kernel on STM32F407-Discovery or other vendor-specific Cortex-M hardware.

**Steps followed:**
1. Build pw_kernel for a target where SysTick CALIB reports SKEW=1 or NOREF=1
2. Flash and boot the device
3. Observe assertion failure in `systick_init()`

**Expected result:**
Kernel boots successfully. CALIB validation is debug-only or logs a warning.

**Actual result:**
`systick_init()` asserts `(TENMS * 100) == SYS_TICK_HZ` unconditionally when `TENMS > 0`, blocking boot on many real hardware platforms.

**Root cause:**
CALIB register content is **implementation-defined** per ARM specification - it varies by silicon vendor, not by architecture. The kernel should not rely on unreliable vendor metadata to gate boot.

Per ARM architecture reference:
- TENMS: Implementation-defined (may be 0)
- SKEW=1: TENMS is inexact due to clock skew
- NOREF=1: No reference clock; TENMS may be unreliable

Many vendor implementations (STM32F4 series, AST1030) set these flags or leave TENMS=0.

**Recommendation:**
Either:
1. Make CALIB check debug-only (`#[cfg(debug_assertions)]`)
2. Log a warning instead of asserting - inform but don't crash

The kernel already trusts user-configured `SYS_TICK_HZ`; refusing to boot based on unreliable silicon metadata is counterproductive.

**Host environment:**
Linux (any)

**Target Device:**
STM32F407-Discovery, AST1030, and other Cortex-M targets with vendor-specific CALIB values
