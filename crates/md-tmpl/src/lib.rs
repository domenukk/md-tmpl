#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![cfg_attr(feature = "std", doc = include_str!("../README.md"))]

// Re-export everything from the core engine crate.
#[doc(inline)]
pub use md_tmpl_core::*;

#[cfg(feature = "std")]
#[doc = include_str!("../SPEC.md")]
#[doc(hidden)]
pub mod spec {}

/// Re-export proc macros from `md-tmpl-macros` so users can write
/// `md_tmpl::include_template!` instead of importing `md_tmpl_macros` separately.
#[cfg(feature = "macros")]
pub use md_tmpl_macros::{include_template, template};
