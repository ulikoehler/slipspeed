# SLIPstream-codec

A pure-Rust implementation of the **Serial Line Internet Protocol (SLIP)** encoder and decoder with support for **in-memory buffers** as well as **streaming I/O**.

The crate exposes convenience helpers for encoding and decoding byte slices as well as `SlipWriter` and `SlipReader` wrappers for any `std::io::Write` or `std::io::Read` implementor.

A C/C++ port of this library, focused on embedded systems, is available at [libslipstream](https://github.com/ulikoehler/libslipstream), (but sadly, the [slipstream](https://crates.io/crates/slipstream) crate name on crates.io is already taken.

## Performance

The integrated benchmark encodes random frames of varying lengths and reports throughput for encoding and decoding. Typical performance is often higher as random frames may include lots of characters that need escaping. To run the benchmark example:

```sh
cargo run --release --example benchmark
```

Example results on a AMD Ryzen 5 3600 CPU:

```text
Frames processed: 1000000
Encoded bytes: 65952678
Encoding took: 224.759907ms (224.76 ns/frame)
Encoding throughput: 293.44 MB/s
Decoding took: 279.670968ms (279.67 ns/frame)
Decoding throughput: 235.82 MB/s
```

## Frame Structure

SLIP is specified in [RFC 1055](https://datatracker.ietf.org/doc/html/rfc1055) and uses the following conventions:

- Frames are delimited by the byte `0xC0` (`END`).
- Literal `END` bytes in the payload are escaped as the two-byte sequence `0xDB 0xDC` (`ESC`, `ESC_END`).
- Literal escape bytes `0xDB` (`ESC`) are encoded as `0xDB 0xDD` (`ESC`, `ESC_ESC`).
- The decoder clears its buffer whenever it encounters an `END`, emitting the accumulated payload as a frame.

**Note:** This implementation does not add or verify any checksums. If you require integrity checks, consider adding a CRC or similar mechanism to your payloads.

In contrast to the [simple_slip](https://crates.io/crates/simple_slip) crate, we do not add extra `END` bytes before a frame. This allows for more efficient streaming scenarios where frames are sent back-to-back.

## Quick Start

```rust
use slipstream::{decode_frame, encode_frame, Result};

fn main() -> Result<()> {
	let frame = encode_frame(b"hello");
	let payload = decode_frame(&frame)?;
	assert_eq!(payload, b"hello");
	Ok(())
}
```

Run `cargo run --example basic` for a complete program.

## Streaming I/O

```rust
use slipstream::{SlipReader, SlipWriter, Result};
use std::io::Cursor;

fn main() -> Result<()> {
	let mut writer = SlipWriter::new(Vec::new());
	writer.write_frame(b"ping")?;
	writer.write_frame(b"pong")?;
	let encoded = writer.into_inner();

	let mut reader = SlipReader::new(Cursor::new(encoded));
	while let Some(frame) = reader.read_frame()? {
		println!("Frame: {:?}", frame);
	}
	Ok(())
}
```

See `examples/stream.rs` for the full example.

## Utilities

- `encode_frame`, `decode_frames`, and `decode_frames_with_remainder` for slice-based workflows.
- `encoded_len` and `decoded_lengths` to inspect frame sizes without materialising payloads.
- `SlipReader::read_frame_length` and `SlipReader::take_remainder` for streaming scenarios that require sizing or recovery after truncated input.

## Cargo Features

- `async-codec` enables a `slipstream::async_codec::SlipCodec` implementing the `asynchronous_codec` traits for runtime-agnostic async I/O.
- `tokio-codec` enables a `slipstream::tokio_codec::SlipCodec` compatible with `tokio_util::codec` Framed adapters.

## Additional Examples

- `cargo run --example async_codec --features async-codec` demonstrates the runtime-agnostic `asynchronous_codec` integration.
- `cargo run --example tokio_codec --features tokio-codec` showcases usage with Tokio's `duplex` streams and `tokio_util::codec::Framed`.
- `cargo run --example benchmark` performs a reproducible encoding and decoding micro-benchmark over pseudo-random frames.

### Benchmark example

The `examples/benchmark.rs` program performs a reproducible micro-benchmark using a fixed-seed
LCG (Linear Congruential Generator). By default it generates a large number of small random
frames and reports:

- total frames processed
- total encoded bytes
- wall-clock time for encoding and decoding
- per-frame average (ns/frame)

This example is intended as a simple throughput sanity check rather than a rigorous
benchmark (it prints elapsed times to stdout). To run the example:

```text
cargo run --example benchmark
```

To make the example quicker during development, lower the `FRAME_COUNT` constant at the
top of the example file (for example to `20_000`). The RNG seed is fixed (0xDEADBEEF)
for reproducible runs.
