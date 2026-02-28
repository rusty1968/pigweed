# The SysTick CALIB Register Is Not Gospel

## The Bug

This code was in pw_kernel's SysTick initialization:

```rust
pub fn systick_init() {
    let systick_regs = Regs::get().systick;
    let ticks_per_10ms = systick_regs.calib.read().tenms();
    info!("Ticks per 10ms: {}", ticks_per_10ms as u32);
    if ticks_per_10ms > 0 {
        pw_assert::eq!(
            (ticks_per_10ms * 100) as u32,
            KernelConfig::SYS_TICK_HZ as u32
        );
    }
}
```

This code says: *"If the hardware's TENMS field of the CALIB register has any value, it must match my configuration exactly, or panic."*

**This is architecturally incorrect.** The CALIB register is an optional, implementation-defined hint that may be zero, inaccurate, or completely irrelevant to your actual clock configuration.

---

## Why This Is Wrong

### 1. ARM Architecture Says It's Optional

Per ARM DDI 0403E.e, Section B3.3.4 (SysTick Calibration Value Register):

> *"**Optionally**, the TENMS field can indicate the reload value to configure the SysTick for a 10ms tick."*

The word "optionally" is critical. ARM does not require vendors to populate this field correctly — or at all.

### 2. ARM Architecture Says It May Be Zero

From the same section:

> *"If calibration information is not known, the field reads as zero."*

A zero value is architecturally valid. The register literally tells you: "I don't know."

### 3. ARM Provides Flags Indicating Unreliability

The CALIB register includes two explicit "don't trust me" flags:

| Flag | Meaning |
|------|---------|
| **SKEW** (bit 30) | `1` = TENMS is **inexact** due to clock skew |
| **NOREF** (bit 31) | `1` = No reference clock; TENMS is **unreliable** |

If ARM trusted CALIB to be accurate, these flags would not exist. Their presence is an architectural admission that TENMS cannot be relied upon.

### 4. CALIB Assumes a Specific Clock Source

Per ARM DDI 0403E.e:

> *"If the **external reference clock** has a frequency that is an exact multiple of 10ms, TENMS provides this reload value."*

TENMS is calibrated for the **external reference clock** (`CLKSOURCE=0`), not the processor clock (`CLKSOURCE=1`).

If your code uses the processor clock (as pw_kernel does for reliability), CALIB describes the wrong clock source entirely.

### 5. CALIB is Static, Clock Configuration is Dynamic

Using STM32F4 as an example (from RM0090):

> *"The SysTick calibration value is **fixed to 18750**, which gives a reference time base of 1 ms with the SysTick clock set to 18.75 MHz (HCLK/8, with HCLK set to 150 MHz)."*

**TENMS = 18750 is hardcoded in silicon.**

But HCLK is runtime-configurable:

```
Clock Source Selection:         HSI (16 MHz) | HSE (8-25 MHz) | PLL (up to 168 MHz)
                                      ↓
                                   SYSCLK
                                      ↓
                              AHB Prescaler (/1, /2, /4, ... /512)
                                      ↓
                                    HCLK
```

CALIB cannot know:
- Which oscillator you selected
- Your PLL multiplier/divider configuration
- Your AHB prescaler setting
- Whether you're still in early boot (HSI) or running at full speed (PLL)

**A static value cannot describe a dynamic clock.**

### 6. Vendor Implementations Vary Wildly

| Vendor/SoC | TENMS | SKEW | NOREF | Notes |
|------------|-------|------|-------|-------|
| STM32F407 | 18750 | 1 | 1 | Fixed value, flags indicate unreliable |
| STM32H7 | Varies | 1 | 1 | Similar to F4 |
| Microchip SAM | ? | 1 | ? | SKEW=1 because "TENMS is not known" |
| NXP LPC | 0 | - | - | Not populated |
| ASPEED AST1030 | 0 | - | 1 | No external reference |
| QEMU Cortex-M | 0 | 0 | 0 | Emulated, not populated |

There is no consistency. Relying on CALIB for portable code is impossible.

### 7. The TENMS Value May Be Mathematically Impossible

Consider STM32F407:
- TENMS = 18750 (hardcoded)
- Implies clock = 1,875,000 Hz (for 10ms period)
- But STM32F407 HCLK can be configured from 16 MHz to 168 MHz

If HCLK = 168 MHz and CLKSOURCE = processor clock:
- Actual ticks per 10ms = 1,680,000
- CALIB says 18750

These values differ by **89.6x**. CALIB is not just inaccurate — it's describing a completely different reality.

---

## What CALIB Actually Is

CALIB is a **convenience hint** for a narrow use case:

1. You use the external reference clock (`CLKSOURCE=0`)
2. The external reference happens to match what the vendor assumed
3. SKEW=0 (exact calibration)
4. NOREF=0 (reference clock present)

If all four conditions are met, TENMS might be useful. In practice, this rarely happens.

---

## What CALIB Is Not

- **Not a source of truth** for your clock frequency
- **Not a validation mechanism** for your configuration
- **Not portable** across vendors or even products from the same vendor
- **Not dynamic** — cannot reflect runtime clock changes

---

## The Correct Mental Model

```
                    ┌─────────────────────────────────────────┐
                    │         Authoritative Sources           │
                    ├─────────────────────────────────────────┤
                    │  1. User configuration (SYS_TICK_HZ)    │
                    │  2. Actual clock register reads         │
                    │  3. Hardware reference manual           │
                    └─────────────────────────────────────────┘
                                        ↑
                                   Trust these
                    
                    ┌─────────────────────────────────────────┐
                    │         Non-Authoritative Hints         │
                    ├─────────────────────────────────────────┤
                    │  • CALIB register (optional, static,    │
                    │    implementation-defined, may be 0,    │
                    │    wrong clock source, self-admits      │
                    │    unreliability via SKEW/NOREF)        │
                    └─────────────────────────────────────────┘
                                        ↑
                              Log for debug only
```

---

## Conclusion

The SysTick CALIB register is:

| Claim | Reality |
|-------|---------|
| Required | **No** — ARM says "optionally" |
| Accurate | **No** — SKEW/NOREF flags exist for a reason |
| For processor clock | **No** — calibrated for external reference |
| Dynamic | **No** — hardcoded in silicon |
| Portable | **No** — varies wildly by vendor |
| Gospel | **Absolutely not** |

**Treat CALIB as a debug hint, not a validation mechanism. Never assert on it.**

---

## References

- **ARM DDI 0403E.e** — ARMv7-M Architecture Reference Manual
  - Section B3.3.4: "SysTick Calibration Value Register"
  - Key quotes establishing optional/implementation-defined nature
- **ST RM0090** — STM32F405/407/415/417 Reference Manual
  - Documents fixed TENMS value and HCLK/8 assumption
  - Clock tree showing dynamic HCLK configuration
- **ARM DDI 0553B** — ARMv8-M Architecture Reference Manual
  - Section D1.2.4: Same optional/implementation-defined language
