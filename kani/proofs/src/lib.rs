//! Kani harnesses for the Ballista program. Every harness is behind `cfg(kani)`; a plain build of
//! this crate is empty. See `kani/README.md` for the commands, bounds and timings.

#[cfg(kani)]
mod accounts;
#[cfg(kani)]
mod encoding;
#[cfg(kani)]
mod executor;
#[cfg(kani)]
mod group;
#[cfg(kani)]
mod invoke;
#[cfg(kani)]
mod lifecycle;
#[cfg(kani)]
mod math;
#[cfg(kani)]
mod muldiv;
#[cfg(kani)]
mod pda;
#[cfg(kani)]
mod records;
#[cfg(kani)]
mod registry;
#[cfg(kani)]
mod stubs;
#[cfg(kani)]
mod typing;
#[cfg(kani)]
mod util;
#[cfg(kani)]
mod wire;
