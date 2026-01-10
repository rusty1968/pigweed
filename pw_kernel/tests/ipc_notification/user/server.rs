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

//! IPC Notification Test - Server (Handler) Side
//!
//! This test demonstrates the handler side of the IPC notification pattern.
//! The server waits for requests using object_wait(), reads the request with
//! channel_read(), processes it, and responds with channel_respond().
//!
//! Pattern demonstrated:
//! 1. Wait for READABLE signal (transaction pending)
//! 2. Read request data via channel_read()
//! 3. Process request based on operation code
//! 4. Respond via channel_respond()
//!
//! This models a typical driver server like an I2C controller driver.

#![no_main]
#![no_std]

use app_server::handle;
use pw_status::{Error, Result};
use userspace::entry;
use userspace::syscall::{self, Signals};
use userspace::time::Instant;

/// Operation codes matching the client
#[repr(u8)]
enum Op {
    Echo = 1,
    Transform = 2,
    Batch = 3,
    NotifyTest = 4,
    CheckUserSignal = 5,
}

impl TryFrom<u8> for Op {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Op::Echo),
            2 => Ok(Op::Transform),
            3 => Ok(Op::Batch),
            4 => Ok(Op::NotifyTest),
            5 => Ok(Op::CheckUserSignal),
            _ => Err(Error::InvalidArgument),
        }
    }
}

/// Handle echo operation - return data unchanged
fn handle_echo(request: &[u8], response: &mut [u8]) -> Result<usize> {
    // Echo back the entire request including op code
    let len = request.len().min(response.len());
    response[..len].copy_from_slice(&request[..len]);
    Ok(len)
}

/// Handle transform operation - convert to uppercase
fn handle_transform(request: &[u8], response: &mut [u8]) -> Result<usize> {
    if request.is_empty() {
        return Err(Error::InvalidArgument);
    }

    let len = request.len().min(response.len());
    response[0] = request[0]; // Copy op code

    // Transform payload to uppercase
    for i in 1..len {
        response[i] = request[i].to_ascii_uppercase();
    }

    Ok(len)
}

/// Handle batch operation - return computed result
fn handle_batch(request: &[u8], response: &mut [u8]) -> Result<usize> {
    if request.len() < 3 {
        return Err(Error::InvalidArgument);
    }

    // Request: [op, a, b]
    // Response: [op, a, b, a+b]
    response[0] = request[0];
    response[1] = request[1];
    response[2] = request[2];
    response[3] = request[1].wrapping_add(request[2]);

    Ok(4)
}

/// Handle notification test - raise USER signal before responding
fn handle_notify_test(response: &mut [u8]) -> Result<usize> {
    // Raise USER signal on the initiator (client) before responding
    // This demonstrates the async notification pattern
    syscall::raise_peer_user_signal(handle::IPC)?;

    // Respond with success
    response[0] = 0;
    Ok(1)
}

/// Handle check user signal - report if USER signal was raised on us
///
/// This is used to test bidirectional notification (client -> server).
/// We check if the USER signal is currently set on our handle.
fn handle_check_user_signal(response: &mut [u8]) -> Result<usize> {
    // Check if USER signal is set on our IPC handle
    // Use a zero timeout to do a non-blocking check
    let user_signal_set = match syscall::object_wait(
        handle::IPC,
        Signals::USER,
        Instant::from_ticks(0), // Non-blocking
    ) {
        Ok(signals) => signals.contains(Signals::USER),
        Err(Error::DeadlineExceeded) => false, // Timeout means signal not set
        Err(_) => false,
    };

    response[0] = Op::CheckUserSignal as u8;
    response[1] = if user_signal_set { 1 } else { 0 };
    Ok(2)
}

/// Main server loop
fn server_loop() -> Result<()> {
    pw_log::info!("Server starting - waiting for IPC requests");

    loop {
        // Wait for an IPC request (READABLE signal indicates pending transaction)
        syscall::object_wait(handle::IPC, Signals::READABLE, Instant::MAX)?;

        // Read the request from the channel
        let mut request = [0u8; 64];
        let req_len = syscall::channel_read(handle::IPC, 0, &mut request)?;

        if req_len == 0 {
            pw_log::error!("Received empty request");
            // Respond with error status
            syscall::channel_respond(handle::IPC, &[0xFF])?;
            continue;
        }

        // Parse operation code
        let op = match Op::try_from(request[0]) {
            Ok(op) => op,
            Err(_) => {
                pw_log::error!("Unknown operation: {}", request[0] as u32);
                syscall::channel_respond(handle::IPC, &[0xFE])?;
                continue;
            }
        };

        // Process the request
        let mut response = [0u8; 64];
        let resp_len = match op {
            Op::Echo => handle_echo(&request[..req_len], &mut response),
            Op::Transform => handle_transform(&request[..req_len], &mut response),
            Op::Batch => handle_batch(&request[..req_len], &mut response),
            Op::NotifyTest => handle_notify_test(&mut response),
            Op::CheckUserSignal => handle_check_user_signal(&mut response),
        };

        // Send response
        match resp_len {
            Ok(len) => {
                syscall::channel_respond(handle::IPC, &response[..len])?;
            }
            Err(e) => {
                pw_log::error!("Request processing error: {}", e as u32);
                syscall::channel_respond(handle::IPC, &[0xFD])?;
            }
        }
    }
}

#[entry]
fn entry() -> ! {
    pw_log::info!("🔄 IPC Notification Test - Server Starting");

    if let Err(e) = server_loop() {
        pw_log::error!("Server error: {}", e as u32);
        let _ = syscall::debug_shutdown(Err(e));
    }

    loop {}
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    pw_log::error!("PANIC");
    let _ = syscall::debug_shutdown(Err(Error::Internal));
    loop {}
}
