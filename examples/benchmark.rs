//! Micro-benchmark example for SLIP encoding/decoding.
//!
//! This program generates a fixed sequence of pseudo-random frames (Linear
//! Congruential Generator seeded with 0xDEADBEEF) and measures the time taken
//! to (1) encode each frame using `encode_frame` and (2) decode the concatenated
//! stream back into frames with `decode_frames`.
//!
//! Notes:
//! - The RNG uses a fixed seed so the benchmark is reproducible.
//! - The default `FRAME_COUNT` is large to produce stable timings; reduce it if
//!   you want faster iterations during development.
//!
//! Run with:
//!
//! ```text
//! cargo run --example benchmark
//! ```
//!
//! To run the example with fewer frames for a quick smoke-test, edit
//! `FRAME_COUNT` near the top of the example (or set it to e.g. 20_000).
use std::time::Instant;

fn main() -> slipspeed::Result<()> {
    // Default number of frames for the benchmark. You can override this for
    // quick smoke-tests by setting the BENCH_FRAMES environment variable,
    // e.g. `BENCH_FRAMES=20_000 cargo run --example benchmark`.
    const FRAME_COUNT: usize = 5_000_000;
    // Fixed frame length to exercise fixed-size frames as requested.
    const FRAME_LEN: usize = 128;

    // Allow overriding the frame count for quick local runs.
    let frame_count = std::env::var("BENCH_FRAMES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(FRAME_COUNT);

    // Frames with arbitrary random bytes (full 0..=255)
    let frames_random = make_frames(frame_count, FRAME_LEN, 0xDEADBEEF, |rng: &mut Lcg| rng.next_u8());

    // Frames with ASCII-only random bytes (printable 0x20..=0x7E)
    let frames_ascii = make_frames(frame_count, FRAME_LEN, 0xDEADBEEF, |rng: &mut Lcg| {
        let v = rng.next_u8() % (0x7E - 0x20 + 1) as u8;
        0x20u8.wrapping_add(v)
    });

    // Run the benchmark twice and report labeled results.
    run_bench("random bytes", &frames_random)?;
    run_bench("ASCII-only bytes", &frames_ascii)?;

    Ok(())
}

fn ns_per_item(duration: std::time::Duration, count: usize) -> f64 {
    duration.as_nanos() as f64 / count as f64
}

fn run_bench(label: &str, frames: &[Vec<u8>]) -> slipstream::Result<()> {
    let frame_count = frames.len();

    let start = Instant::now();
    let encoded: Vec<Vec<u8>> = frames.iter().map(|frame| slipspeed::encode_frame(frame)).collect();
    let encode_elapsed = start.elapsed();

    let concatenated: Vec<u8> = encoded.iter().flat_map(|frame| frame.iter().copied()).collect();

    let start = Instant::now();
    let decoded = slipspeed::decode_frames(&concatenated)?;
    let decode_elapsed = start.elapsed();

    assert_eq!(frames, &decoded, "round-trip mismatch for {label}");

    println!("--- Benchmark: {label} ---");
    println!("Frames processed: {}", frame_count);
    println!("Encoded bytes: {}", concatenated.len());
    println!(
        "Encoding took: {:?} ({:.2} ns/frame)",
        encode_elapsed,
        ns_per_item(encode_elapsed, frame_count)
    );
    let encoded_bytes = concatenated.len();
    let encode_secs = encode_elapsed.as_secs_f64();
    let encode_mbps = if encode_secs > 0.0 {
        (encoded_bytes as f64 / 1_000_000.0) / encode_secs
    } else {
        0.0
    };
    println!("Encoding throughput: {:.2} MB/s", encode_mbps);
    println!(
        "Decoding took: {:?} ({:.2} ns/frame)",
        decode_elapsed,
        ns_per_item(decode_elapsed, frame_count)
    );
    let decoded_bytes = concatenated.len();
    let decode_secs = decode_elapsed.as_secs_f64();
    let decode_mbps = if decode_secs > 0.0 {
        (decoded_bytes as f64 / 1_000_000.0) / decode_secs
    } else {
        0.0
    };
    println!("Decoding throughput: {:.2} MB/s", decode_mbps);
    println!();

    Ok(())
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        // Constants from Numerical Recipes LCG.
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        self.state
    }

    fn next_u8(&mut self) -> u8 {
        (self.next() >> 24) as u8
    }
}

fn make_frames<F>(frame_count: usize, frame_len: usize, seed: u64, mut next_byte: F) -> Vec<Vec<u8>>
where
    F: FnMut(&mut Lcg) -> u8,
{
    let mut r = Lcg::new(seed);
    (0..frame_count)
        .map(|_| (0..frame_len).map(|_| next_byte(&mut r)).collect::<Vec<u8>>())
        .collect::<Vec<Vec<u8>>>()
}
