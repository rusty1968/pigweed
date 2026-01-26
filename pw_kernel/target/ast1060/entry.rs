// Copyright 2025 The Pigweed Authors
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

//! Entry point for ASPEED AST1060 target.

#![no_std]
#![no_main]

use arch_arm_cortex_m::Arch;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn pw_assert_HandleFailure() -> ! {
    use kernel::Arch as _;
    Arch::panic();
}

mod console_backend {
    unsafe extern "Rust" {
        pub fn console_backend_init();
        pub fn console_backend_write_all(buf: &[u8]) -> pw_status::Result<()>;
    }
}

#[cortex_m_rt::entry]
fn main() -> ! {
    kernel::static_init_state!(static mut INIT_STATE: InitKernelState<Arch>);

    // SAFETY: `main` is only executed once, so we never generate more than one
    // `&mut` reference to `INIT_STATE`.
    #[allow(static_mut_refs)]
    unsafe {
        // Initialize UART console
        console_backend::console_backend_init();
        let _ = console_backend::console_backend_write_all(b"\r\nHello World!\r\n");
        let _ = console_backend::console_backend_write_all(b"ast1060 pigweed fw is running!\r\n");
        kernel::main(Arch, &mut INIT_STATE)
    };
}

