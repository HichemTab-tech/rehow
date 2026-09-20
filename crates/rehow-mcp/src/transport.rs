use futures::SinkExt;
use rmcp::{
    RoleServer,
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{Transport, async_rw::JsonRpcMessageCodec},
};
use std::{borrow::Cow, io, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, BufReader},
    sync::Mutex,
};
use tokio_util::codec::FramedWrite;

pub const MAX_INPUT_FRAME_BYTES: usize = 1024 * 1024;

type Writer<W> = FramedWrite<W, JsonRpcMessageCodec<TxJsonRpcMessage<RoleServer>>>;

pub struct StdioTransport<R, W> {
    reader: BufReader<R>,
    input: Vec<u8>,
    writer: Arc<Mutex<Option<Writer<W>>>>,
    failed: bool,
}

impl<R, W> StdioTransport<R, W>
where
    R: AsyncRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin + 'static,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            input: Vec::new(),
            writer: Arc::new(Mutex::new(Some(FramedWrite::new(
                writer,
                JsonRpcMessageCodec::new(),
            )))),
            failed: false,
        }
    }

    async fn fail_closed(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        self.failed = true;
        self.input.clear();
        self.writer.lock().await.take();
        None
    }
}

impl<R, W> Transport<RoleServer> for StdioTransport<R, W>
where
    R: AsyncRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin + 'static,
{
    type Error = io::Error;

    fn name() -> Cow<'static, str> {
        Cow::Borrowed("rehow-bounded-stdio")
    }

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let writer = self.writer.clone();
        async move {
            let mut writer = writer.lock().await;
            let writer = writer
                .as_mut()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "transport closed"))?;
            writer.send(item).await.map_err(io::Error::from)
        }
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        if self.failed {
            return None;
        }
        loop {
            let remaining = MAX_INPUT_FRAME_BYTES
                .saturating_add(2)
                .saturating_sub(self.input.len());
            if remaining == 0 {
                return self.fail_closed().await;
            }
            let count = {
                let mut limited = (&mut self.reader).take(remaining as u64);
                match limited.read_until(b'\n', &mut self.input).await {
                    Ok(count) => count,
                    Err(_) => return self.fail_closed().await,
                }
            };
            if count == 0 {
                if self.input.is_empty() {
                    self.writer.lock().await.take();
                    return None;
                }
                return self.fail_closed().await;
            }
            if !self.input.ends_with(b"\n") {
                if self.input.len() > MAX_INPUT_FRAME_BYTES {
                    return self.fail_closed().await;
                }
                continue;
            }
            self.input.pop();
            if self.input.ends_with(b"\r") {
                self.input.pop();
            }
            if self.input.is_empty() {
                continue;
            }
            if self.input.len() > MAX_INPUT_FRAME_BYTES {
                return self.fail_closed().await;
            }
            let parsed = serde_json::from_slice(&self.input);
            self.input.clear();
            return match parsed {
                Ok(message) => Some(message),
                Err(_) => self.fail_closed().await,
            };
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.failed = true;
        self.input.clear();
        self.writer.lock().await.take();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded_request(frame_length: usize) -> Vec<u8> {
        let mut value = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#.to_vec();
        assert!(value.len() <= frame_length);
        value.resize(frame_length, b' ');
        value.push(b'\n');
        value
    }

    #[tokio::test]
    async fn accepts_exact_limit_and_crlf() {
        for mut bytes in [padded_request(MAX_INPUT_FRAME_BYTES), {
            let mut value = padded_request(MAX_INPUT_FRAME_BYTES);
            value.insert(value.len() - 1, b'\r');
            value
        }] {
            let (mut input, reader) = tokio::io::duplex(bytes.len() + 1);
            let (_output, writer) = tokio::io::duplex(1024);
            tokio::io::AsyncWriteExt::write_all(&mut input, &bytes)
                .await
                .unwrap();
            drop(input);
            let mut transport = StdioTransport::new(reader, writer);
            assert!(transport.receive().await.is_some());
            bytes.clear();
        }
    }

    #[tokio::test]
    async fn rejects_over_limit_malformed_and_eof_fragment() {
        for bytes in [
            padded_request(MAX_INPUT_FRAME_BYTES + 1),
            b"not-json\n".to_vec(),
            b"{\"jsonrpc\":\"2.0\"}".to_vec(),
        ] {
            let (mut input, reader) = tokio::io::duplex(bytes.len() + 1);
            let (_output, writer) = tokio::io::duplex(1024);
            tokio::io::AsyncWriteExt::write_all(&mut input, &bytes)
                .await
                .unwrap();
            drop(input);
            let mut transport = StdioTransport::new(reader, writer);
            assert!(transport.receive().await.is_none());
        }
    }
}
