//! Quick A/B micro-benchmark: `render_ctx` (allocates a new `String` per
//! call) vs `render_ctx_into` (reuses an existing `String` buffer).
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
        "---
params:
  - name = str
  - place = str
---
Hello {{ name }}, welcome to {{ place }}!",
    )
    .unwrap();
    let context = ctx! { name: "Alice", place: "Wonderland" };

    // Warm up before timing so the two measured loops start on equal footing.
    for _ in 0..WARMUP_ITERS {
        black_box(tmpl.render_ctx(black_box(&context)).unwrap());
    }

    let allocating = time(MEASURED_ITERS, || {
        black_box(tmpl.render_ctx(black_box(&context)).unwrap());
    });
    let mut buf = String::with_capacity(64);
    let reused_buf = time(MEASURED_ITERS, || {
        buf.clear();
        tmpl.render_ctx_into(black_box(&context), &mut buf).unwrap();
        black_box(&buf);
    });

    report("render_ctx     ", allocating);
    report("render_ctx_into", reused_buf);
    println!(
        "Buffer-reuse speedup: {:.2}×",
        allocating.as_secs_f64() / reused_buf.as_secs_f64()
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
