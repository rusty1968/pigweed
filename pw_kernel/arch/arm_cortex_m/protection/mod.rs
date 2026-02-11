// Copyright 2026 The Pigweed Authors
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not
// use this file except in compliance with the License. You may obtain a copy of
// the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations under
// the License.

//! Cortex-M MPU memory protection configuration.
//!
//! Architecture-specific register encoding is in [`v7`] (PMSAv7/ARMv7-M) and
//! [`v8`] (PMSAv8/ARMv8-M). This module provides the shared [`MemoryConfig`]
//! type, whose `MpuRegion` storage is selected at compile time via the
//! `armv7m` or `armv8m` crate feature.

use kernel_config::{CortexMKernelConfigInterface as _, KernelConfig};
use memory_config::{MemoryRegion, MemoryRegionType};

use crate::regs::Regs;

#[cfg(feature = "armv7m")]
pub mod v7;
#[cfg(feature = "armv8m")]
pub mod v8;

#[cfg(feature = "armv7m")]
use v7::MpuRegion;
#[cfg(feature = "armv8m")]
use v8::MpuRegion;

pub(super) const LOG_MPU: bool = false;

/// Cortex-M MPU memory protection configuration.
///
/// Represents a complete set of MPU region descriptors built from a list of
/// generic [`MemoryRegion`] entries. The register encoding of each region is
/// architecture-specific: PMSAv7 for ARMv7-M targets, PMSAv8 for ARMv8-M.
pub struct MemoryConfig {
    mpu_regions: [MpuRegion; KernelConfig::NUM_MPU_REGIONS],
    generic_regions: &'static [MemoryRegion],
}

impl MemoryConfig {
    /// Create a new `MemoryConfig` in a `const` context.
    ///
    /// # Panics
    /// Will panic if the current target's MPU does not support enough regions
    /// to represent `regions`.
    #[must_use]
    pub const fn const_new(regions: &'static [MemoryRegion]) -> Self {
        let mut mpu_regions = [MpuRegion::const_default(); KernelConfig::NUM_MPU_REGIONS];
        let mut i = 0;
        while i < regions.len() {
            mpu_regions[i] = MpuRegion::from_memory_region(&regions[i]);
            i += 1;
        }
        Self {
            mpu_regions,
            generic_regions: regions,
        }
    }

    /// Write this memory configuration to the MPU registers.
    ///
    /// # Safety
    /// Caller must ensure that it is safe and sound to update the MPU with this
    /// memory config.
    pub unsafe fn write(&self) {
        let mut mpu = Regs::get().mpu;

        mpu.ctrl.write(
            mpu.ctrl
                .read()
                .with_enable(false)
                .with_hfnmiena(false)
                .with_privdefena(true),
        );

        log_if::info_if!(
            LOG_MPU,
            "Programming {} MPU regions",
            self.mpu_regions.len() as usize
        );

        for (index, region) in self.mpu_regions.iter().enumerate() {
            region.write_to_mpu(&mut mpu, index);
        }

        // A DSB followed by ISB is required after MPU configuration to ensure
        // subsequent instructions use the new MPU settings.
        //
        // SAFETY: These are memory barrier instructions with no side effects
        // other than ordering guarantees.
        unsafe {
            core::arch::asm!("dsb sy", options(nostack, preserves_flags));
        }

        mpu.ctrl.write(mpu.ctrl.read().with_enable(true));

        // Instruction barrier ensures the pipeline is flushed and refetched
        // with the new MPU configuration active.
        unsafe {
            core::arch::asm!("isb sy", options(nostack, preserves_flags));
        }
    }

    /// Log the details of the memory configuration.
    pub fn dump(&self) {
        for (index, region) in self
            .mpu_regions
            .iter()
            .take(self.generic_regions.len())
            .enumerate()
        {
            region.dump(index);
        }
    }
}

/// Initialize the MPU for supporting user space memory protection.
pub fn init() {
    #[cfg(feature = "armv8m")]
    v8::init_mair();
}

impl memory_config::MemoryConfig for MemoryConfig {
    // PMSAv7-specific limit: the RASR SIZE field encodes region size as
    // 2^(SIZE+1). SIZE=0b11111 (31) would give 4GB but is UNPREDICTABLE per
    // the ARM spec, so the maximum valid value is SIZE=0b11110 (30) = 2^31 =
    // 2GB (0x8000_0000).
    //
    // Hardware enforcement: a second region is not needed because write()
    // sets PRIVDEFENA=1, enabling the default memory map as a background
    // region for privileged code and granting the kernel full 4GB access.
    //
    // Software validation: range_has_access() checks only the explicitly
    // defined generic_regions, not PRIVDEFENA. The 2GB region here ensures
    // that software range checks on the lower address space (where all current
    // target Flash, SRAM, and peripherals reside) return the correct result.
    // If kernel code needs to validate addresses above 2GB, a second region
    // covering 0x8000_0000-0xFFFF_FFFF should be added here.
    #[cfg(feature = "armv7m")]
    const KERNEL_THREAD_MEMORY_CONFIG: Self = Self::const_new(&[MemoryRegion::new(
        MemoryRegionType::ReadWriteExecutable,
        0x0000_0000,
        0x8000_0000,
    )]);

    #[cfg(feature = "armv8m")]
    const KERNEL_THREAD_MEMORY_CONFIG: Self = Self::const_new(&[MemoryRegion::new(
        MemoryRegionType::ReadWriteExecutable,
        0x0000_0000,
        0xffff_ffff,
    )]);

    fn range_has_access(
        &self,
        access_type: MemoryRegionType,
        start_addr: usize,
        end_addr: usize,
    ) -> bool {
        let validation_region = MemoryRegion::new(access_type, start_addr, end_addr);
        MemoryRegion::regions_have_access(self.generic_regions, &validation_region)
    }
}
