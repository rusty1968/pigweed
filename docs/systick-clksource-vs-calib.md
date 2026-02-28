# SysTick Clock Source Selection vs. CALIB Register Correlation

## Overview

This document explains the rationale for explicitly selecting the processor clock (`CLKSOURCE = 1`) for SysTick operation in pw_kernel, and why this choice inherently invalidates the CALIB register's calibration data on most hardware.

---

## SysTick Clock Source Options

The SysTick Control and Status Register (SYST_CSR) bit 2 (`CLKSOURCE`) selects the timer's clock source:

| CLKSOURCE | Clock Source | Description |
|-----------|--------------|-------------|
| 0 | External reference | Implementation-defined; typically HCLK/8 on STM32 |
| 1 | Processor clock | Always HCLK; guaranteed available |

### Reset Value

Per ARM architecture, `CLKSOURCE` reset value is **implementation-defined**. This means:
- Some implementations reset to 0 (external reference)
- Some implementations reset to 1 (processor clock)
- Code cannot assume either default

---

## Why pw_kernel Uses Processor Clock (CLKSOURCE = 1)

### 1. Guaranteed Availability

The processor clock is **always present** — it's the same clock driving the CPU. The external reference clock is implementation-defined and may not be connected.

Per ARM DDI 0403E.e, Section B3.3.1:
> *"If the SysTick timer does not have an external reference clock, CLKSOURCE reads as 1 and ignores writes."*

Some implementations (e.g., AST1030, AST1060) don't provide an external reference at all.

### 2. Deterministic Behavior

Using the processor clock provides consistent, predictable timing across all Cortex-M implementations. The external reference varies by vendor:

| Vendor/SoC | External Reference | Notes |
|------------|-------------------|-------|
| STM32F4 | HCLK/8 | 1/8th of core clock |
| STM32H7 | HCLK/8 | Same as F4 |
| NXP LPC | Varies | Some use dedicated oscillator |
| ASPEED AST1030 | Not connected | Falls back to processor clock |

### 3. Maximum Resolution

The processor clock provides the highest tick rate, enabling finer-grained timing:

| Clock Source | STM32F407 @ 168 MHz | Resolution |
|--------------|---------------------|------------|
| Processor (HCLK) | 168,000,000 Hz | ~6 ns |
| External (HCLK/8) | 21,000,000 Hz | ~48 ns |

For an RTOS kernel, higher resolution means more accurate scheduling deadlines.

### 4. Simplified Configuration

Using processor clock means `SYS_TICK_HZ` directly equals the core clock frequency — a value developers already know and configure. No division factor to track.

---

## The CALIB Register Problem

### What CALIB Contains

The CALIB register (SYST_CALIB, address 0xE000E01C) provides:

| Field | Bits | Description |
|-------|------|-------------|
| TENMS | 23:0 | Reload value for 10ms period (implementation-defined) |
| SKEW | 30 | 1 = TENMS is inexact |
| NOREF | 31 | 1 = No reference clock; TENMS unreliable |

### Critical Detail: TENMS Assumes External Reference

Per ARM DDI 0403E.e, Section B3.3.4:
> *"Optionally, the TENMS field can indicate the reload value to configure the SysTick for a 10ms tick.**If the external reference clock has a frequency that is an exact multiple of 10ms**, TENMS provides this reload value."*

**TENMS is calibrated for the external reference clock, not the processor clock.**

### STM32F4 Example (from RM0090)

> *"The SysTick calibration value is fixed to 18750, which gives a reference time base of 1 ms with the SysTick clock set to 18.75 MHz (HCLK/8, with HCLK set to 150 MHz)."*

Calculation:
- TENMS = 18750 (for 10ms at 18.75 MHz external reference)
- This equals 1,875,000 ticks/second
- **Not** the processor clock frequency

### CALIB is Static, HCLK is Dynamic

The STM32 clock tree allows multiple clock sources and runtime configuration:

```
HSI (16 MHz) ─┐
HSE (8 MHz)  ─┼─→ SYSCLK ─→ AHB Prescaler ─→ HCLK (core clock)
PLL          ─┘
```

**HCLK frequency depends on runtime configuration:**
- Which oscillator is selected (HSI, HSE, or PLL)
- PLL multiplier/divider settings
- AHB prescaler value

**But TENMS = 18750 is hardcoded in silicon** — it assumes a specific configuration (HCLK = 150 MHz).

This means even if CALIB was intended for the processor clock, it would be wrong if you:
- Run at 168 MHz instead of 150 MHz
- Use a different PLL configuration  
- Boot from HSI (16 MHz) before switching to PLL
- Apply a different AHB prescaler

The CALIB value **cannot adapt to runtime clock configuration**. It's a static hint for one assumed configuration, not a reliable source of truth.

---

## The Inherent Mismatch

When pw_kernel:
1. Sets `CLKSOURCE = 1` (processor clock)
2. Configures `SYS_TICK_HZ` = processor clock frequency (e.g., 168 MHz)

And the hardware:
1. Populates TENMS assuming external reference (HCLK/8)

**The values will never match:**

```
STM32F407 @ 168 MHz:

SYS_TICK_HZ = 168,000,000 (processor clock)
TENMS * 100 = 18750 * 100 = 1,875,000 (external reference calibration)

168,000,000 ≠ 1,875,000

Ratio: 168,000,000 / 1,875,000 = 89.6x difference
       (approximately HCLK / (HCLK/8) * adjustment factor)
```

This is not a bug or misconfiguration — it's a fundamental incompatibility between:
- **Our choice**: Use processor clock for reliability and resolution
- **CALIB assumption**: Calibrated for external reference clock

---

## Why Asserting on CALIB is Wrong

The original code:
```rust
if ticks_per_10ms > 0 {
    pw_assert::eq!(
        (ticks_per_10ms * 100) as u32,
        KernelConfig::SYS_TICK_HZ as u32
    );
}
```

This asserts that hardware calibration matches our configuration. But:

1. **We deliberately use a different clock source** than CALIB assumes
2. **CALIB is implementation-defined** — vendors can put anything there
3. **SKEW/NOREF flags exist** because ARM knows TENMS may be wrong

The assertion punishes correct configuration because we chose a more reliable clock source.

---

## Correct Approach

### Option 1: Warning (Implemented)

Log the mismatch for debugging but don't block boot:

```rust
if ticks_per_10ms > 0 && (ticks_per_10ms * 100) as u32 != KernelConfig::SYS_TICK_HZ as u32 {
    warn!(
        "SysTick CALIB mismatch: TENMS*100={} != SYS_TICK_HZ={}. \
         CALIB is implementation-defined and may be unreliable.",
        (ticks_per_10ms * 100) as u32,
        KernelConfig::SYS_TICK_HZ as u32
    );
}
```

### Option 2: Remove Check Entirely

Since we use processor clock and CALIB describes external reference, the comparison is meaningless. The check could be removed entirely.

### Option 3: Check Only When Using External Reference

If a future kernel configuration allowed `CLKSOURCE = 0`, the check would make sense — but only then:

```rust
if using_external_reference && !calib.skew() && !calib.noref() && ticks_per_10ms > 0 {
    // Validation meaningful only for external reference with reliable CALIB
}
```

---

## Summary

| Aspect | Processor Clock (CLKSOURCE=1) | External Reference (CLKSOURCE=0) |
|--------|-------------------------------|----------------------------------|
| Availability | Always present | Implementation-defined |
| Frequency | Core clock (HCLK) | Varies (often HCLK/8) |
| CALIB validity | **Mismatch expected** | May match if SKEW=0, NOREF=0 |
| pw_kernel choice | **Selected** | Not used |
| Resolution | Maximum | Reduced |

**Conclusion:** Using processor clock is the correct engineering choice for portability and reliability. This choice inherently invalidates CALIB-based validation, making the original assertion architecturally incorrect for pw_kernel's design.

---

## References

- **ARM DDI 0403E.e** — ARMv7-M Architecture Reference Manual
  - Section B3.3.1: SysTick Control and Status Register (CLKSOURCE)
  - Section B3.3.4: SysTick Calibration Value Register (TENMS, SKEW, NOREF)
- **ST RM0090** — STM32F405/407 Reference Manual
  - Section on SysTick calibration value
- **ARM DDI 0553B** — ARMv8-M Architecture Reference Manual
  - Section D1.2: SysTick register descriptions
