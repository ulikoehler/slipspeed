#[cfg(not(feature = "async-codec"))]
fn main() {
    eprintln!(
        "Enable the `async-codec` feature to build this example:\n    cargo run --example async_codec --features async-codec"
    );
}

#[cfg(feature = "async-codec")]
fn main() -> slipstream::Result<()> {
    use asynchronous_codec::{FramedRead, FramedWrite};
    use futures::{executor::block_on, io::Cursor, sink::SinkExt, stream::StreamExt};
    use slipstream::async_codec::SlipCodec;

    block_on(async move {
        let cursor = Cursor::new(Vec::new());
        let mut writer = FramedWrite::new(cursor, SlipCodec::new());
        writer.send(b"ping".to_vec()).await?;
        writer.send(b"pong".to_vec()).await?;
        let encoded_cursor = writer.into_inner();
        let encoded = encoded_cursor.into_inner();

        println!("Encoded bytes: {encoded:?}");

        let mut reader = FramedRead::new(Cursor::new(encoded), SlipCodec::new());
        while let Some(frame) = reader.next().await.transpose()? {
            println!("Decoded frame: {:?}", String::from_utf8_lossy(&frame));
        }

        Ok(())
    })
}
