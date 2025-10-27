#[cfg(not(feature = "tokio-codec"))]
fn main() {
    eprintln!(
        "Enable the `tokio-codec` feature to build this example:\n    cargo run --example tokio_codec --features tokio-codec"
    );
}

#[cfg(feature = "tokio-codec")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> slipstream::Result<()> {
    use futures::{SinkExt, StreamExt};
    use slipstream::tokio_codec::SlipCodec;
    use tokio::io::duplex;
    use tokio_util::codec::Framed;

    let (client, server) = duplex(1024);
    let mut writer = Framed::new(client, SlipCodec::new());
    let mut reader = Framed::new(server, SlipCodec::new());

    writer.send(b"hello".to_vec()).await?;
    writer.send(b"world".to_vec()).await?;
    writer.flush().await?;

    while let Some(frame) = reader.next().await.transpose()? {
        println!("Received frame: {:?}", String::from_utf8_lossy(&frame));
    }

    Ok(())
}
