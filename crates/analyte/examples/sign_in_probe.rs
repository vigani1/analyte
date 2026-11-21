//! Asks the engine's Windows sign-in pipe once, as the app will, and prints the answer line; the Windows CI end-to-end step runs it as local test users (ADR 0021).
//!
//! Usage: `sign_in_probe \\.\pipe\analyte-sign-in-<port>`. It connects at
//! identification level (tokio's default), sends one request line, prints the
//! answer line on standard output and exits; every wait is bounded.

/// winerror.h `ERROR_PIPE_BUSY`: the instance is between callers.
#[cfg(windows)]
const PIPE_BUSY: i32 = 231;

#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{io::Write as _, time::Duration};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let name = std::env::args()
        .nth(1)
        .ok_or("usage: sign_in_probe <pipe name>")?;
    let answer = tokio::time::timeout(Duration::from_secs(10), async {
        // The single instance is busy between callers; wait for it.
        let mut pipe = loop {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
                Ok(pipe) => break pipe,
                Err(error) if error.raw_os_error() == Some(PIPE_BUSY) => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(error) => return Err(error),
            }
        };
        pipe.write_all(b"{\"v\":1,\"next\":\"/\"}\n").await?;
        let mut answer = Vec::new();
        let mut byte = [0u8; 1];
        while answer.len() <= 1024 && pipe.read(&mut byte).await? == 1 {
            answer.extend_from_slice(&byte);
            if byte == *b"\n" {
                break;
            }
        }
        Ok(answer)
    })
    .await??;
    std::io::stdout().write_all(&answer)?;
    Ok(())
}

/// The pipe exists only on Windows.
#[cfg(not(windows))]
fn main() {}
