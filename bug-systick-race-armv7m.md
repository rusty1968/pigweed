**Title:** pw_kernel: SysTick IRQ race condition on ARMv7-M due to PRIMASK=0 at boot

**What were you trying to do:**
Boot pw_kernel on an ARMv7-M target (AST1030 QEMU).

**Steps followed:**
1. Build pw_kernel for an ARMv7-M target (Cortex-M3/M4/M7)
2. Flash and boot the device
3. Observe boot failure due to assertion in SysTick handler

**Expected result:**
Kernel boots successfully with SysTick interrupts handled after scheduler initialization.

**Actual result:**
Intermittent assertion failure during boot. SysTick fires before the scheduler is ready because:
- ARMv7-M resets PRIMASK to 0 (interrupts unmasked)
- `systick_early_init()` enables both counter and interrupt (`ENABLE=1, TICKINT=1`)
- SysTick IRQ preempts boot path before scheduler initialization completes

ARMv8-M is not affected because PRIMASK resets to 1, keeping interrupts masked.

**Host environment:**
Linux (any)

**Target Device:**
ARMv7-M: Cortex-M3, Cortex-M4, Cortex-M7 (e.g., AST1030 QEMU)
