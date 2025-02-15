//! # Loadstone Library
#![feature(never_type)]
#![feature(bool_to_option)]
#![feature(associated_type_bounds)]
#![feature(alloc_error_handler)]
#![cfg_attr(test, allow(unused_imports))]
#![cfg_attr(target_arch = "arm", no_std)]

#[cfg(target_arch = "arm")]
use alloc_cortex_m::CortexMHeap;

/// Loadstone uses the Cortex M heap allocator, for the purposes of
/// ECDSA signature verification.
#[cfg(target_arch = "arm")]
#[global_allocator]
pub static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[cfg(target_arch = "arm")]
#[alloc_error_handler]
fn oom(_: core::alloc::Layout) -> ! {
    defmt::error!("Out of heap memory!");
    loop {}
}

#[cfg(target_arch = "arm")]
use panic_semihosting as _;

#[cfg(target_arch = "arm")]
use defmt_rtt as _; // global logger

pub mod devices;
pub mod error;

#[cfg(feature = "cortex_m_any")]
pub mod ports;

#[cfg(all(target_arch = "arm", not(feature = "cortex_m_any")))]
compile_error!(
    "Loadstone can't be built for `arm` without further target specification \
               Either run tests with `cargo test` natively, or define a target through the \
               appropriate configuration and/or feature flags."
);
