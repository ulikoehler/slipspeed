use slipstream::{Result, SlipReader, SlipWriter};
use std::io::Cursor;

fn main() -> Result<()> {
    let mut writer = SlipWriter::new(Vec::new());
    writer.write_frame(b"ping")?;
    writer.write_frame(b"pong")?;
    let encoded = writer.into_inner();

    let mut reader = SlipReader::new(Cursor::new(encoded));
    while let Some(frame) = reader.read_frame()? {
        println!("Received frame: {:?}", frame);
    }
    Ok(())
}
