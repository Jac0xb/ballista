#[cfg(not(feature = "spec-api"))]
mod execute;
#[cfg(feature = "spec-api")]
pub mod execute;

#[cfg(not(feature = "spec-api"))]
mod introspect;
#[cfg(feature = "spec-api")]
pub mod introspect;

#[cfg(not(feature = "spec-api"))]
mod math;
#[cfg(feature = "spec-api")]
pub mod math;

#[cfg(not(feature = "spec-api"))]
mod registry;
#[cfg(feature = "spec-api")]
pub mod registry;

pub use execute::run;
