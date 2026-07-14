//! Rendering pre-compiled template segments.
//!
//! Walks the compiled segment tree and produces output text by
//! evaluating expressions, loops, conditionals, and includes at
//! render time.
//!
//! The renderer is split into focused submodules:
//! - [`segments`]: the core segment-tree dispatch loop.
//! - [`expr`]: compiled expression evaluation and filter application.
//! - [`value`]: writing a resolved [`Value`](crate::value::Value) into output.
//! - [`float`]: fast fixed-precision float formatting.
//! - [`control`]: for-loop and conditional rendering.
//! - [`matching`]: `{% match %}` block rendering and variant resolution.
//! - [`condition`]: condition evaluation and numeric comparison.
//! - [`include`]: `{% include %}` directive rendering.

mod condition;
mod control;
mod expr;
mod float;
mod include;
mod matching;
mod segments;
mod value;

#[cfg(all(test, feature = "std"))]
pub(crate) use condition::eval_condition;
#[cfg(feature = "std")]
pub(crate) use control::register_loop_meta;
pub use segments::estimate_output_capacity;
pub(crate) use segments::render_interpolated_str;
#[cfg(not(feature = "std"))]
pub(crate) use segments::render_segments_into_no_std;
#[cfg(feature = "std")]
pub(crate) use segments::{render_segments, render_segments_into};

#[cfg(all(test, feature = "std"))]
#[path = "../render_tests.rs"]
mod render_tests;
