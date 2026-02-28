# System Generator Type System Tour

This document provides a guided tour of the type system in the `pw_kernel`
system generator. The generator reads a JSON5 configuration file and produces
Rust code and linker scripts for a specific hardware target.

## Attribution

The system generator was originally designed by **Dave Roth** (davidroth@google.com)
with the `ArchConfigInterface` trait and implementations for ARMv8-M and RISC-V.

**Anthony Rocha** extended the system with:
- **ARMv7-M architecture support** (`Armv7MConfig` implementation)
- **`validate_mpu()` trait method** — architecture-specific MPU validation hook
- **MPU validation subsystem** (`mpu_validation/` module):
  - `mod.rs` — Common types (`MpuValidationMode`, `MpuIssue`, `MemoryRegion`)
  - `pmsav7.rs` — PMSAv7-specific validation for ARMv7-M's power-of-2 constraints
- **`mpu_validation` config field** in `KernelConfig` (strict/warn/permissive modes)

## Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          SystemConfig<A>                                │
│  ┌──────────────────┐    ┌────────────────────────────────────────────┐ │
│  │   arch: A        │    │              base: BaseConfig              │ │
│  │ (implements      │    │  ┌────────────────┐  ┌──────────────────┐  │ │
│  │ ArchConfigIface) │    │  │ kernel: Kernel │  │ apps: Vec<App>   │  │ │
│  └──────────────────┘    │  │   Config       │  │   Config         │  │ │
│                          │  └────────────────┘  └──────────────────┘  │ │
│                          └────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

## Core Types

### `SystemConfig<A: ArchConfigInterface>`

The top-level configuration type, generic over architecture:

```rust
pub struct SystemConfig<A: ArchConfigInterface> {
    pub arch: A,           // Architecture-specific configuration
    pub base: BaseConfig,  // Common configuration (kernel, apps)
}
```

The generic parameter `A` allows architecture-specific behavior while sharing
common validation and code generation logic.

### `BaseConfig`

Architecture-independent system configuration:

```rust
pub struct BaseConfig {
    pub kernel: KernelConfig,
    pub apps: Vec<AppConfig>,
    pub arch_crate_name: &'static str,  // Populated at runtime
}
```

### `KernelConfig`

Kernel memory layout and settings:

```rust
pub struct KernelConfig {
    pub flash_start_address: u64,
    pub flash_size_bytes: u64,
    pub ram_start_address: u64,
    pub ram_size_bytes: u64,
    pub interrupt_table: Option<InterruptTableConfig>,
    pub mpu_validation: MpuValidationMode,  // strict | warn | permissive
}
```

### `AppConfig`

Per-application configuration:

```rust
pub struct AppConfig {
    pub name: String,
    pub flash_size_bytes: u64,
    pub ram_size_bytes: u64,
    pub process: ProcessConfig,
    pub constants: Vec<ConstConfig>,

    // Calculated fields (populated by SystemGenerator):
    pub flash_start_address: u64,
    pub ram_start_address: u64,
    pub start_fn_address: u64,
    pub initial_sp: u64,
}
```

Note: `flash_start_address`, `ram_start_address`, `start_fn_address`, and
`initial_sp` are derived from the kernel layout and app ordering. They're
marked `#[serde(skip_deserializing)]` so they're not in the JSON5 config.

## Process Configuration

### `ProcessConfig`

Defines a userspace process:

```rust
pub struct ProcessConfig {
    pub name: String,
    pub memory_mappings: Vec<MemoryMapping>,
    pub objects: Vec<ObjectConfig>,
    pub threads: Vec<ThreadConfig>,
}
```

### `MemoryMapping`

MPU-enforced memory regions:

```rust
pub struct MemoryMapping {
    pub name: String,
    pub ty: MemoryMappingType,  // Device | ReadOnlyExecutable | ReadWriteData
    pub start_address: u64,
    pub size_bytes: u64,
}
```

### `ObjectConfig` (Tagged Enum)

Kernel objects owned by a process:

```rust
pub enum ObjectConfig {
    ChannelInitiator(ChannelInitiatorConfig),
    ChannelHandler(ChannelHandlerConfig),
    Interrupt(InterruptConfig),
}
```

This uses Serde's `#[serde(tag = "type")]` for JSON5 like:
```json5
{
  type: "channel_initiator",
  name: "my_channel",
  handler_app: "server",
  handler_object_name: "handler"
}
```

### `ThreadConfig`

Userspace thread definition:

```rust
pub struct ThreadConfig {
    pub name: String,
    pub stack_size_bytes: u64,
    pub priority: Option<String>,
}
```

---

## Architecture Abstraction

### `ArchConfigInterface` Trait

The key extensibility point for multi-architecture support:

```rust
pub trait ArchConfigInterface {
    /// Returns the architecture crate name (e.g., "arch_arm_cortex_m")
    fn get_arch_crate_name(&self) -> &'static str;

    /// Compute the entry point address (may add thumb bit on ARM)
    fn get_start_fn_address(&self, flash_start_address: u64) -> u64;

    /// Architecture-specific validation and config mutations
    fn calculate_and_validate_config(
        &mut self,
        config: &mut BaseConfig,
    ) -> Result<()>;

    /// Return linker section for interrupt table (None for RISC-V)
    fn get_interrupt_table_link_section(&self) -> Option<String>;

    /// Validate memory layout for MPU compatibility (added by Anthony Rocha)
    fn validate_mpu(&self, _config: &BaseConfig) -> Result<()> {
        Ok(())  // Default: no MPU validation
    }
}
```

### Architecture Implementations

| Type | Arch | MPU Type | Added by | Notes |
|------|------|----------|----------|-------|
| `Armv8MConfig` | ARMv8-M | PMSAv8 | Dave Roth | 32-byte alignment, no subregions |
| `Armv7MConfig` | ARMv7-M | PMSAv7 | Anthony Rocha | Power-of-2 alignment, 8 subregions |
| `RiscVConfig` | RISC-V | PMP | Dave Roth | No MPU validation yet |

#### ARMv7-M Example

```rust
pub struct Armv7MConfig {
    pub nvic: Armv7MNvicConfig,
}

pub struct Armv7MNvicConfig {
    pub vector_table_start_address: u64,
    pub vector_table_size_bytes: u64,
}
```

The `Armv7MConfig` implementation (by Anthony Rocha):
1. Adds `kernel_code` mapping to all apps (for SVC return)
2. Validates PMSAv7 MPU constraints via `validate_mpu()`

---

## MPU Validation Subsystem

*This entire subsystem was added by Anthony Rocha as part of the ARMv7-M port.*

### Module Structure

```
mpu_validation/
├── mod.rs      # Common types (MpuValidationMode, MpuIssue, MemoryRegion)
└── pmsav7.rs   # PMSAv7-specific validation (ARMv7-M)
```

### `MpuValidationMode`

Controls how validation errors are reported:

```rust
pub enum MpuValidationMode {
    Strict,     // Fail build on issues (default)
    Warn,       // Emit warnings, continue
    Permissive, // Silent
}
```

### `MpuIssue`

A detected compatibility problem:

```rust
pub struct MpuIssue {
    pub code: &'static str,      // e.g., "MPU001"
    pub message: String,
    pub region_name: String,
    pub suggestion: Option<String>,
}
```

### PMSAv7 Validation (`pmsav7.rs`)

PMSAv7 has strict constraints:
- Region sizes must be power-of-2 (32B to 4GB)
- Base address must be aligned to region size
- 8 subregions per region (subregion disable via SRD mask)

Key types:

```rust
pub struct Pmsav7Region {
    pub base: u64,           // Aligned base address
    pub size: u64,           // Power-of-2 size
    pub size_field: u32,     // RASR SIZE field (log2(size) - 1)
    pub subregion_size: u64, // size / 8
    pub srd_mask: u8,        // Subregion disable mask
    pub enabled_subregions: Vec<u8>,
}
```

Validation detects:
- **MPU001**: Subregion overlap with kernel memory
- **MPU002**: Excessive region "bloat" (>4x requested size)
- **MPU003**: App-to-app MPU region interference

---

## SystemGenerator

The orchestrator that ties everything together:

```rust
pub struct SystemGenerator<'a, A: ArchConfigInterface> {
    cli: Cli,
    config: SystemConfig<A>,
    env: Environment<'a>,  // minijinja template engine
}
```

### Construction Flow

```rust
impl SystemGenerator {
    pub fn new(cli: Cli, config: SystemConfig<A>) -> Result<Self> {
        // 1. Setup Jinja environment
        // 2. populate_addresses()      - Stack apps after kernel
        // 3. populate_memory_mappings() - Add flash/ram to each app
        // 4. populate_interrupt_table() - Build IRQ handler table
        // 5. calculate_and_validate()  - Generic + arch validation
        // 6. validate_mpu()            - Architecture-specific MPU check
    }
}
```

### Address Layout

Apps are stacked sequentially after the kernel in both flash and RAM:

```
Flash:
┌──────────────────┬──────────────────┬──────────────────┐
│  Kernel Flash    │    App 0 Flash   │    App 1 Flash   │
│  (configured)    │    (calculated)  │    (calculated)  │
└──────────────────┴──────────────────┴──────────────────┘

RAM:
┌──────────────────┬──────────────────┬──────────────────┐
│  Kernel RAM      │    App 0 RAM     │    App 1 RAM     │
│  (configured)    │    (calculated)  │    (calculated)  │
└──────────────────┴──────────────────┴──────────────────┘
```

---

## Type Relationships Diagram

```
                    ┌─────────────────────┐
                    │ ArchConfigInterface │◄──── Trait
                    │     (trait)         │
                    └─────────────────────┘
                              ▲
          ┌──────────────────┼──────────────────┐
          │                  │                  │
 ┌────────┴───────┐ ┌───────┴────────┐ ┌───────┴──────┐
 │  Armv8MConfig  │ │  Armv7MConfig  │ │  RiscVConfig │
 │  (PMSAv8)      │ │  (PMSAv7)      │ │  (PMP)       │
 └────────────────┘ └────────────────┘ └──────────────┘
          │                  │
          │                  ▼
          │         ┌────────────────────┐
          │         │ validate_pmsav7_   │
          │         │ layout()           │
          │         └────────────────────┘
          │                  │
          │                  ▼
          │         ┌────────────────────┐
          │         │   MpuIssue         │
          │         │   Pmsav7Region     │
          │         └────────────────────┘
          │
          ▼
 ┌────────────────────────────────────────────────┐
 │            SystemConfig<A>                     │
 │  ┌─────────┐    ┌───────────────────────────┐  │
 │  │ arch: A │    │      base: BaseConfig     │  │
 │  └─────────┘    │  ┌─────────┐  ┌────────┐  │  │
 │                 │  │ kernel  │  │ apps[] │  │  │
 │                 │  └─────────┘  └────────┘  │  │
 │                 └───────────────────────────┘  │
 └────────────────────────────────────────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │   SystemGenerator<A> │
              │   - populate_*()     │
              │   - validate()       │
              │   - render()         │
              └──────────────────────┘
```

---

## Adding a New Architecture

1. **Define config struct** in `system_config.rs`:
   ```rust
   #[derive(Clone, Debug, Deserialize, Serialize)]
   pub struct NewArchConfig { /* ... */ }
   ```

2. **Implement `ArchConfigInterface`** in `lib.rs`:
   ```rust
   impl ArchConfigInterface for NewArchConfig {
       fn get_arch_crate_name(&self) -> &'static str { "arch_new" }
       fn get_start_fn_address(&self, addr: u64) -> u64 { addr }
       // ...
   }
   ```

3. **(Optional) Add MPU validation** in `mpu_validation/`:
   - Create `mpu_validation/newarch.rs`
   - Add `pub mod newarch;` to `mpu_validation/mod.rs`
   - Implement `validate_mpu()` calling your validation function

4. **Add binary entrypoint** in `BUILD.bazel`:
   ```python
   rust_binary(
       name = "system_generator_newarch",
       srcs = ["main_newarch.rs"],
       # ...
   )
   ```

---

## JSON5 Config Example

```json5
{
  arch: {
    vector_table_start_address: 0x08000000,
    vector_table_size_bytes: 0x400,
  },
  kernel: {
    flash_start_address: 0x08000000,
    flash_size_bytes: 0x10000,
    ram_start_address: 0x20000000,
    ram_size_bytes: 0x8000,
    mpu_validation: "strict",
  },
  apps: [
    {
      name: "blinky",
      flash_size_bytes: 0x4000,
      ram_size_bytes: 0x2000,
      process: {
        name: "blinky",
        threads: [
          { name: "main", stack_size_bytes: 1024 }
        ],
        objects: [],
      },
    },
  ],
}
```

---

## Running Tests

```bash
bazelisk test --config k_host //pw_kernel/tooling/system_generator:mpu_validation_test
```
