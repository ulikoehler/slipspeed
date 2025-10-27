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

fn main() -> slipstream::Result<()> {
    const FRAME_COUNT: usize = 1_000_000;
    const MAX_FRAME_LEN: usize = 128;

    let mut rng = Lcg::new(0xDEADBEEF);
    let frames: Vec<Vec<u8>> = (0..FRAME_COUNT)
        .map(|_| {
            let len = (rng.next_u8() as usize % MAX_FRAME_LEN) + 1;
            (0..len).map(|_| rng.next_u8()).collect::<Vec<u8>>()
        })
        .collect();

    let start = Instant::now();
    let encoded: Vec<Vec<u8>> = frames
        .iter()
        .map(|frame| slipstream::encode_frame(frame))
        .collect();
    let encode_elapsed = start.elapsed();

    let concatenated: Vec<u8> = encoded
        .iter()
        .flat_map(|frame| frame.iter().copied())
        .collect();

    let start = Instant::now();
    let decoded = slipstream::decode_frames(&concatenated)?;
    let decode_elapsed = start.elapsed();

    assert_eq!(frames, decoded, "round-trip mismatch");

    println!("Frames processed: {FRAME_COUNT}");
    println!("Encoded bytes: {}", concatenated.len());
    println!(
        "Encoding took: {:?} ({:.2} ns/frame)",
        encode_elapsed,
        ns_per_item(encode_elapsed, FRAME_COUNT)
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
        ns_per_item(decode_elapsed, FRAME_COUNT)
    );
    let decoded_bytes = concatenated.len();
    let decode_secs = decode_elapsed.as_secs_f64();
    let decode_mbps = if decode_secs > 0.0 {
        (decoded_bytes as f64 / 1_000_000.0) / decode_secs
    } else {
        0.0
    };
    println!("Decoding throughput: {:.2} MB/s", decode_mbps);

    Ok(())
}

fn ns_per_item(duration: std::time::Duration, count: usize) -> f64 {
    duration.as_nanos() as f64 / count as f64
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
