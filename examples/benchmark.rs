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
    println!(
        "Decoding took: {:?} ({:.2} ns/frame)",
        decode_elapsed,
        ns_per_item(decode_elapsed, FRAME_COUNT)
    );

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
