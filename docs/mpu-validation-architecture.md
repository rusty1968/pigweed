# MPU Validation Architecture

This document explains the design of the `validate_mpu()` system in the pw_kernel system generator.

## Overview

The system generator validates memory layouts at build time to catch MPU configuration issues before deployment. This is critical for ARMv7-M targets using PMSAv7, where power-of-2 region alignment can cause regions to "bloat" and overlap.

## Architecture

### The Out-of-Tree Pattern

The system generator uses a **dispatch enum + trait** pattern to support out-of-tree architectures:

```
┌─────────────────────────────────────────────────────────────┐
│                      main.rs (binary)                       │
├─────────────────────────────────────────────────────────────┤
│  enum ArchConfig {                                          │
│      Armv8M(Armv8MConfig),                                  │
│      Armv7M(Armv7MConfig),  ◄── in-tree architectures       │
│      RiscV(RiscVConfig),                                    │
│  }                                                          │
│                                                             │
│  impl ArchConfigInterface for ArchConfig {                  │
│      fn validate_mpu(&self, config) -> Result<()> {         │
│          match self {                                       │
│              Armv8M(c) => c.validate_mpu(config),           │
│              Armv7M(c) => c.validate_mpu(config),  ◄── dispatch │
│              RiscV(c) => c.validate_mpu(config),            │
│          }                                                  │
│      }                                                      │
│  }                                                          │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      lib.rs (library)                       │
├─────────────────────────────────────────────────────────────┤
│  pub trait ArchConfigInterface {                            │
│      // ... other methods ...                               │
│                                                             │
│      /// Validate memory layout for MPU compatibility.      │
│      fn validate_mpu(&self, _config: &BaseConfig) -> Result<()> { │
│          Ok(())  // Default: no validation                  │
│      }                                                      │
│  }                                                          │
│                                                             │
│  impl ArchConfigInterface for Armv7MConfig {                │
│      fn validate_mpu(&self, config: &BaseConfig) -> Result<()> { │
│          // PMSAv7-specific validation logic                │
│          validate_pmsav7_layout(...)                        │
│      }                                                      │
│  }                                                          │
│                                                             │
│  impl ArchConfigInterface for Armv8MConfig {                │
│      // Uses default (no-op) - PMSAv8 has simpler regions   │
│  }                                                          │
│                                                             │
│  impl ArchConfigInterface for RiscVConfig {                 │
│      // Uses default (no-op) - different memory model       │
│  }                                                          │
└─────────────────────────────────────────────────────────────┘
```

### Why This Pattern?

1. **Out-of-tree support**: New architectures can create their own binary with a custom `ArchConfig` enum containing only their architectures, implementing `ArchConfigInterface` for the whole enum.

2. **Default implementations**: Architectures that don't need MPU validation (like ARMv8-M with simpler region alignment) can just use the default no-op implementation.

3. **Centralized trait**: The `ArchConfigInterface` trait in `lib.rs` defines the contract - any architecture must provide these methods.

## Call Flow

```
SystemGenerator::new()
        │
        ▼
    populate_addresses()
        │
        ▼
    populate_memory_mappings()
        │
        ▼
    populate_interrupt_table()
        │
        ▼
    calculate_and_validate()
        │
        ▼
    arch.validate_mpu(&base_config)  ◄── Called here!
        │
        ├── Armv7M: validate_pmsav7_layout()
        │       │
        │       ├── Check kernel/app overlap
        │       ├── Check region bloat
        │       └── Check app-to-app overlap
        │
        ├── Armv8M: Ok(()) (default)
        │
        └── RiscV: Ok(()) (default)
```

## Validation Modes

The `mpu_validation` field in kernel config controls behavior:

| Mode | MPU001/MPU003 (Overlap) | MPU002 (Bloat) |
|------|-------------------------|----------------|
| `strict` | Error - fail build | Error - fail build |
| `warn` | Warning | Info |
| `permissive` | Silent | Silent |

Default is **`strict`** - builds fail on any MPU issues.

## Adding Validation to a New Architecture

1. Implement `ArchConfigInterface` for your config struct in `lib.rs`
2. Override `validate_mpu()` with your architecture-specific logic
3. Add your config to the `ArchConfig` enum in `main.rs`
4. Add the dispatch in the `impl ArchConfigInterface for ArchConfig` block

Example for a hypothetical PMSAv9:
```rust
// In lib.rs
impl ArchConfigInterface for Armv9MConfig {
    fn validate_mpu(&self, config: &BaseConfig) -> Result<()> {
        validate_pmsav9_layout(...)
    }
}

// In main.rs
pub enum ArchConfig {
    Armv8M(...),
    Armv7M(...),
    Armv9M(Armv9MConfig),  // Add here
    RiscV(...),
}

impl ArchConfigInterface for ArchConfig {
    fn validate_mpu(&self, config: &BaseConfig) -> Result<()> {
        match self {
            // ... existing ...
            ArchConfig::Armv9M(c) => c.validate_mpu(config),  // Dispatch
        }
    }
}
```

## Files

| File | Purpose |
|------|---------|
| `lib.rs` | `ArchConfigInterface` trait + per-arch implementations |
| `main.rs` | `ArchConfig` enum + dispatch implementation |
| `mpu_validation/mod.rs` | Common types (`MpuValidationMode`, `MpuIssue`) |
| `mpu_validation/pmsav7.rs` | PMSAv7-specific validation algorithm |
