use async_trait::async_trait;
use ezsockets::ClientConfig;
use ezsockets::CloseCode;
use ezsockets::CloseFrame;
use ezsockets::Error;
use std::io::BufRead;

struct Client {}

#[async_trait]
impl ezsockets::ClientExt for Client {
    type Call = ();

    async fn on_text(&mut self, text: String) -> Result<(), Error> {
        tracing::info!("received message: {text}");
        Ok(())
    }

    async fn on_binary(&mut self, bytes: Vec<u8>) -> Result<(), Error> {
        tracing::info!("received bytes: {bytes:?}");
        Ok(())
    }

    async fn on_call(&mut self, params: Self::Call) -> Result<(), Error> {
        let () = params;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = ClientConfig::new("ws://127.0.0.1:8080");
    let (handle, future) = ezsockets::connect(|_| Client {}, config).await;
    tokio::spawn(async move {
        future.await.unwrap();
    });

    let stdin = std::io::stdin();
    let lines = stdin.lock().lines();
    for line in lines {
        let line = line.unwrap();
        if line == "exit" {
            tracing::info!("exiting...");
            handle
                .close(Some(CloseFrame {
                    code: CloseCode::Normal,
                    reason: "adios!".to_string(),
                }))
                .await;
            return;
        }
        tracing::info!("sending {line}");
        handle.text(line);
    }
}
