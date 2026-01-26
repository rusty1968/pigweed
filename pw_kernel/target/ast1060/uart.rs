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
#![allow(unused)]
#![no_std]

use ast1060_pac::Uart;
use embedded_io::ErrorKind;
use embedded_io::ErrorType;
use core::result::Result::{self, Ok, Err};
use core::convert::TryFrom;
use core::convert::From;
use core::fmt::Debug;
use core::cmp::PartialEq;
use core::clone::Clone;
use core::marker::Copy;

pub enum Uart16550Error {
    Overrun,
    Parity,
    Framing,
    Break,
    Unknown,
}

impl core::fmt::Debug for Uart16550Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Overrun => f.write_str("Overrun"),
            Self::Parity => f.write_str("Parity"),
            Self::Framing => f.write_str("Framing"),
            Self::Break => f.write_str("Break"),
            Self::Unknown => f.write_str("Unknown"),
        }
    }
}

impl embedded_io::Error for Uart16550Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Uart16550Error::Overrun | Uart16550Error::Parity | Uart16550Error::Framing => {
                ErrorKind::InvalidData
            }

            Uart16550Error::Break => ErrorKind::Interrupted,
            Uart16550Error::Unknown => ErrorKind::Other,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Parity {
    None,
    Even,
    Odd,
}

impl PartialEq for Parity {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Even, Self::Even) => true,
            (Self::Odd, Self::Odd) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StopBits {
    One,
    Two,
}

impl PartialEq for StopBits {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::One, Self::One) => true,
            (Self::Two, Self::Two) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WordLength {
    Five,
    Six,
    Seven,
    Eight,
}

impl PartialEq for WordLength {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Five, Self::Five) => true,
            (Self::Six, Self::Six) => true,
            (Self::Seven, Self::Seven) => true,
            (Self::Eight, Self::Eight) => true,
            _ => false,
        }
    }
}

pub struct Config {
    pub baud_rate: u32,
    pub word_length: WordLength,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub clock: u32,
}

pub struct UartController {
    uart: Uart,
}

impl UartController {
    pub unsafe fn init(&self, config: &Config) {
        // Calculate baud divisor
        let raw = (config.clock / 13) / (16 * config.baud_rate);
        let baud_divisor = u16::try_from(raw).unwrap();

        // Enable DLAB to access divisor latch registers
        self.uart.uartlcr().write(|w| w.dlab().set_bit());

        // Set Divisor Latch Low and High
        self.uart.uartdll().write(|w| {
            w.the_lsbof_the_bd_divisor_latch()
                .bits((baud_divisor & 0xFF) as u8)
        });
        self.uart.uartdlh().write(|w| {
            w.the_msbof_the_bd_divisor_latch()
                .bits((baud_divisor >> 8) as u8)
        });

        // Disable DLAB to access other registers
        self.uart.uartlcr().write(|w| w.dlab().clear_bit());

        // Enable FIFO and set trigger level
        self.uart.uartfcr().write(|w| {
            w.enbl_uartfifo().set_bit();
            w.rx_fiforst().set_bit();
            w.tx_fiforst().set_bit();
            w.define_the_rxr_fifointtrigger_level().bits(0b10) // Example trigger level
        });

        // Configure Line Control Register
        self.uart.uartlcr().write(|w| {
            w.cls().bits(config.word_length as u8);
            w.stop().bit(config.stop_bits == StopBits::Two);
            match config.parity {
                Parity::None => w.pen().clear_bit(),
                Parity::Even => {
                    w.pen().set_bit();
                    w.eps().set_bit()
                }
                Parity::Odd => {
                    w.pen().set_bit();
                    w.eps().clear_bit()
                }
            }
        });

        // Enable interrupts (optional, based on application needs)

        self.uart.uartier().write(|w| {
            w.erbfi().set_bit(); // Enable Received Data Available Interrupt
            w.etbei().set_bit(); // Enable Transmitter Holding Register Empty Interrupt
            w.elsi().set_bit(); // Enable Receiver Line Status Interrupt
            w.edssi().set_bit() // Enable Modem Status Interrupt
        });

        // Additional configurations can be added here
    }

    /// Sends a byte using the FIFO.
    pub fn send_byte_fifo(&mut self, data: u8) {
        // Wait until the Transmitter Holding Register (THR) is empty
        while self.uart.uartlsr().read().thre().bit_is_clear() {}

        // Write the byte to the Transmit Holding Register (THR)
        self.uart
            .uartthr()
            .write(|w| unsafe { w.bits(u32::from(data)) });
    }

    pub fn read_byte(&mut self) -> Result<u8, Uart16550Error> {
        // Wait until there is data available in the receiver buffer
        while self.uart.uartlsr().read().dr().bit_is_clear() {}

        // Read the byte from the Receiving Buffer Register (UARTRBR)
        let byte = self.uart.uartrbr().read().uartrbr().bits();
        Ok(byte)
    }

    pub fn flush(&mut self) -> Result<(), Uart16550Error> {
        // Wait until the Transmitter Holding Register (THR) is empty
        while self.uart.uartlsr().read().thre().bit_is_clear() {}

        Ok(())
    }
}

impl UartController {
    // Wait until the Transmitter Holding Register (THR) is empty
    pub fn new(uart: Uart) -> Self {
        Self { uart }
    }
    pub fn wait_until_thr_empty(&mut self) {
        while self.uart.uartlsr().read().thre().bit_is_clear() {
             // Basic busy loop instead of delay_ns since we don't have a DelayNs implementation available
             // and we are likely in a kernel context where simple busy waiting is acceptable for now.
             // Ideally we'd use a timer or yield.
             core::hint::spin_loop();
        }
    }
}

impl ErrorType for UartController {
    type Error = Uart16550Error;
}

impl embedded_io::Read for UartController {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let mut count = 0;
        for byte in buf.iter_mut() {
            match self.read_byte() {
                Ok(data) => {
                    *byte = data;
                    count += 1;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(count)
    }

    fn read_exact(
        &mut self,
        buf: &mut [u8],
    ) -> Result<(), embedded_io::ReadExactError<Self::Error>> {
        let mut count = 0;
        for byte in buf.iter_mut() {
            match self.read_byte() {
                Ok(data) => {
                    *byte = data;
                    count += 1;
                }
                Err(e) => return Err(embedded_io::ReadExactError::Other(e)),
            }
        }
        if count == buf.len() {
            Ok(())
        } else {
            Err(embedded_io::ReadExactError::Other(Uart16550Error::Unknown))
        }
    }
}

impl embedded_io::Write for UartController {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        for &byte in buf {
            self.send_byte_fifo(byte);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
