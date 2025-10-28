use slipspeed::{decode_frame, encode_frame, Result};

fn main() -> Result<()> {
    let payload = b"hello, slip";
    let frame = encode_frame(payload);
    println!("Encoded frame bytes: {frame:?}");

    let decoded = decode_frame(&frame)?;
    println!("Decoded payload: {}", String::from_utf8_lossy(&decoded));
    Ok(())
}
