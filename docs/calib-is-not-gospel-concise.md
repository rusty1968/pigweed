# Why SysTick CALIB Is Not Gospel

## The Problem

```rust
pub fn systick_init() {
    let systick_regs = Regs::get().systick;
    let ticks_per_10ms = systick_regs.calib.read().tenms();
    if ticks_per_10ms > 0 {
        pw_assert::eq!(
            (ticks_per_10ms * 100) as u32,
            KernelConfig::SYS_TICK_HZ as u32
        );
    }
}
```

This panics if TENMS doesn't match the kernel config. **This is wrong.**

---

## The CALIB Register

| Field | Bits | Description |
|-------|------|-------------|
| TENMS | 0–23 | Calibration value (ticks per 10ms) |
| SKEW | 30 | `1` = TENMS is **inexact** |
| NOREF | 31 | `1` = No reference clock; TENMS **unreliable** |

The code reads `tenms()` but **ignores `skew()` and `noref()`**.

---

## Why CALIB Cannot Be Trusted

1. **ARM says it's optional** — *"Optionally, TENMS can indicate..."* (DDI 0403E.e §B3.3.4)

2. **SKEW/NOREF flags exist** — If CALIB were reliable, ARM wouldn't include "don't trust me" flags

3. **Wrong clock source** — TENMS is for the external reference clock, not the processor clock that pw_kernel uses

4. **Static value, dynamic clock** — TENMS is hardcoded in silicon; your actual clock is runtime-configurable

5. **Vendors vary wildly**:
   | SoC | TENMS | SKEW | NOREF |
   |-----|-------|------|-------|
   | STM32F407 | 18750 | 1 | 1 |
   | NXP LPC | 0 | - | - |
   | QEMU | 0 | 0 | 0 |

---

## The Fix

Treat CALIB as a debug hint, not a validation mechanism:

```rust
let calib = systick_regs.calib.read();
info!("TENMS: {}, SKEW: {}, NOREF: {}", 
      calib.tenms(), calib.skew(), calib.noref());
// Do NOT assert on these values
```

---

## References

- [ARM DDI 0403E.e §B3.3.4](https://developer.arm.com/documentation/ddi0403/latest) — SysTick Calibration Value Register
- [ARM SysTick Documentation](https://developer.arm.com/documentation/101407/0543/Debugging/Debug-Windows-and-Dialogs/Core-Peripherals/Armv8-M-cores/Armv8-M--System-Tick-Timer)
