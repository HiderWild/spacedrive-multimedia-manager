//! # Media benchmark harness
//!
//! `core::testing::media_bench` is a dependency-free timing helper for the media
//! suite. The repo does not use criterion, so this keeps benchmarks runnable as
//! ordinary `#[ignore]`d release tests (`cargo test --release -- --ignored
//! --nocapture`) while still giving comparable numbers.
//!
//! Later tasks plug in by calling [`bench`] with the operation under test:
//! transcode (B-01), rotation (B-04), and effective-tag resolution (A-05) each
//! wrap their hot path in a closure and report the resulting [`BenchStats`].
//!
//! ## Example
//! ```
//! use sd_core::testing::media_bench::bench;
//!
//! let stats = bench("noop", 100, || {
//!     std::hint::black_box(1 + 1);
//! });
//! assert_eq!(stats.iterations, 100);
//! println!("{stats}");
//! ```

use std::fmt;
use std::time::{Duration, Instant};

use tracing::info;

/// Aggregated timing results for a benchmarked operation.
#[derive(Debug, Clone)]
pub struct BenchStats {
	pub name: String,
	pub iterations: u32,
	pub total: Duration,
	pub mean: Duration,
	pub min: Duration,
	pub max: Duration,
}

impl BenchStats {
	/// Throughput in operations per second based on the mean iteration time.
	pub fn ops_per_sec(&self) -> f64 {
		let secs = self.mean.as_secs_f64();
		if secs > 0.0 {
			1.0 / secs
		} else {
			f64::INFINITY
		}
	}
}

impl fmt::Display for BenchStats {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"{:<28} iters={:>6} mean={:>10.3?} min={:>10.3?} max={:>10.3?} ({:.1} ops/s)",
			self.name,
			self.iterations,
			self.mean,
			self.min,
			self.max,
			self.ops_per_sec()
		)
	}
}

/// Run `op` `iterations` times, returning aggregate timing stats.
///
/// Timing is per-iteration so callers get min/mean/max rather than only a total.
/// The closure is expected to be self-contained; set up fixtures before calling.
pub fn bench<F>(name: impl Into<String>, iterations: u32, mut op: F) -> BenchStats
where
	F: FnMut(),
{
	let name = name.into();
	let iterations = iterations.max(1);

	let mut total = Duration::ZERO;
	let mut min = Duration::MAX;
	let mut max = Duration::ZERO;

	for _ in 0..iterations {
		let start = Instant::now();
		op();
		let elapsed = start.elapsed();
		total += elapsed;
		min = min.min(elapsed);
		max = max.max(elapsed);
	}

	let mean = total / iterations;
	let stats = BenchStats {
		name,
		iterations,
		total,
		mean,
		min,
		max,
	};
	info!(
		bench = %stats.name,
		iterations = stats.iterations,
		mean_us = stats.mean.as_micros(),
		"benchmark complete"
	);
	stats
}

/// Run `op` until at least `min_duration` has elapsed, then report stats.
///
/// Useful for fast operations where a fixed iteration count would finish too
/// quickly to measure reliably (for example A-05 cache-hit resolution).
pub fn bench_for<F>(name: impl Into<String>, min_duration: Duration, mut op: F) -> BenchStats
where
	F: FnMut(),
{
	let name = name.into();
	let mut total = Duration::ZERO;
	let mut min = Duration::MAX;
	let mut max = Duration::ZERO;
	let mut iterations: u32 = 0;

	let deadline = Instant::now() + min_duration;
	loop {
		let start = Instant::now();
		op();
		let elapsed = start.elapsed();
		total += elapsed;
		min = min.min(elapsed);
		max = max.max(elapsed);
		iterations += 1;
		if Instant::now() >= deadline {
			break;
		}
	}

	let mean = total / iterations.max(1);
	BenchStats {
		name,
		iterations,
		total,
		mean,
		min,
		max,
	}
}
