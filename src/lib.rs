//! The platform-independent half of minikey: pure logic, no I/O.

#![no_std]
#![forbid(unsafe_code)]

mod engine;

pub use engine::{Edit, Engine, Method, CAPACITY};
