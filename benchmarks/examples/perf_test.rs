//! Quick A/B micro-benchmark: `render_ctx_allowing_extra` (validates the
//! context against frontmatter on every call) vs `render_ctx_unchecked`
//! (skips validation entirely). Prints ns/op for each path and the relative
//! validation overhead.
//!
//! Run with: `cargo run --release --example perf_test`
use md_tmpl::{Template, ctx};
use std::hint::black_box;
use std::time::{Duration, Instant};

/// Untimed iterations to warm caches / branch predictor before measuring.
const WARMUP_ITERS: usize = 1_000;
/// Timed iterations per measured path.
const MEASURED_ITERS: usize = 100_000;

fn main() {
    let tmpl = Template::from_source(
        "---\nparams:\n  - name = str\n  - place = str\n---\nHello {{ name }}, welcome to {{ place }}!",
    )
    .unwrap();
    let context = ctx! { name: "Alice", place: "Wonderland" };

    // Warm up before timing so the two measured loops start on equal footing.
    for _ in 0..WARMUP_ITERS {
        black_box(tmpl.render_ctx_allowing_extra(black_box(&context)).unwrap());
    }

    let validated = time(MEASURED_ITERS, || {
        black_box(tmpl.render_ctx_allowing_extra(black_box(&context)).unwrap());
    });
    let unchecked = time(MEASURED_ITERS, || {
        black_box(tmpl.render_ctx_unchecked(black_box(&context)).unwrap());
    });

    report("render_ctx_allowing_extra", validated);
    report("render_ctx_unchecked     ", unchecked);
    println!(
        "Validation overhead: {:.2}×",
        validated.as_secs_f64() / unchecked.as_secs_f64()
    );
}

/// Run `f` `iters` times and return the total elapsed wall-clock time.
fn time(iters: usize, mut f: impl FnMut()) -> Duration {
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed()
}

/// Print a labelled total duration alongside the per-operation cost.
fn report(label: &str, total: Duration) {
    let ns_per_op = total.as_nanos() as f64 / MEASURED_ITERS as f64;
    println!("{label}: {total:?} ({ns_per_op:.0} ns/op)");
}
