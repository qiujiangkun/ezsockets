use crate::config::WebsocketConfig;
use crate::Error;
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use std::any::Any;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug, Clone)]
pub enum CloseCode {
    /// Indicates a normal closure, meaning that the purpose for
    /// which the connection was established has been fulfilled.
    Normal,
    /// Indicates that an endpoint is "going away", such as a server
    /// going down or a browser having navigated away from a page.
    Away,
    /// Indicates that an endpoint is terminating the connection due
    /// to a protocol error.
    Protocol,
    /// Indicates that an endpoint is terminating the connection
    /// because it has received a type of data it cannot accept (e.g., an
    /// endpoint that understands only text data MAY send this if it
    /// receives a binary message).
    Unsupported,
    /// Indicates that no status code was included in a closing frame. This
    /// close code makes it possible to use a single method, `on_close` to
    /// handle even cases where no close code was provided.
    Status,
    /// Indicates an abnormal closure. If the abnormal closure was due to an
    /// error, this close code will not be used. Instead, the `on_error` method
    /// of the handler will be called with the error. However, if the connection
    /// is simply dropped, without an error, this close code will be sent to the
    /// handler.
    Abnormal,
    /// Indicates that an endpoint is terminating the connection
    /// because it has received data within a message that was not
    /// consistent with the type of the message (e.g., non-UTF-8 \[RFC3629\]
    /// data within a text message).
    Invalid,
    /// Indicates that an endpoint is terminating the connection
    /// because it has received a message that violates its policy.  This
    /// is a generic status code that can be returned when there is no
    /// other more suitable status code (e.g., Unsupported or Size) or if there
    /// is a need to hide specific details about the policy.
    Policy,
    /// Indicates that an endpoint is terminating the connection
    /// because it has received a message that is too big for it to
    /// process.
    Size,
    /// Indicates that an endpoint (client) is terminating the
    /// connection because it has expected the server to negotiate one or
    /// more extension, but the server didn't return them in the response
    /// message of the WebSocket handshake.  The list of extensions that
    /// are needed should be given as the reason for closing.
    /// Note that this status code is not used by the server, because it
    /// can fail the WebSocket handshake instead.
    Extension,
    /// Indicates that a server is terminating the connection because
    /// it encountered an unexpected condition that prevented it from
    /// fulfilling the request.
    Error,
    /// Indicates that the server is restarting. A client may choose to reconnect,
    /// and if it does, it should use a randomized delay of 5-30 seconds between attempts.
    Restart,
    /// Indicates that the server is overloaded and the client should either connect
    /// to a different IP (when multiple targets exist), or reconnect to the same IP
    /// when a user has performed an action.
    Again,
}

impl From<CloseCode> for u16 {
    fn from(code: CloseCode) -> u16 {
        use self::CloseCode::*;
        match code {
            Normal => 1000,
            Away => 1001,
            Protocol => 1002,
            Unsupported => 1003,
            Status => 1005,
            Abnormal => 1006,
            Invalid => 1007,
            Policy => 1008,
            Size => 1009,
            Extension => 1010,
            Error => 1011,
            Restart => 1012,
            Again => 1013,
        }
    }
}

impl TryFrom<u16> for CloseCode {
    type Error = u16;

    fn try_from(code: u16) -> Result<Self, u16> {
        use self::CloseCode::*;

        Ok(match code {
            1000 => Normal,
            1001 => Away,
            1002 => Protocol,
            1003 => Unsupported,
            1005 => Status,
            1006 => Abnormal,
            1007 => Invalid,
            1008 => Policy,
            1009 => Size,
            1010 => Extension,
            1011 => Error,
            1012 => Restart,
            1013 => Again,
            code => {
                return Err(code);
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct CloseFrame {
    pub code: CloseCode,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<CloseFrame>),
}

#[derive(Debug)]
struct SinkImpl<M, S> {
    sink: S,
    p: PhantomData<M>,
}
impl<M, S> futures::Sink<Message> for SinkImpl<M, S>
where
    M: From<Message> + Unpin,
    S: futures::Sink<M, Error = Error> + Unpin,
{
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink.poll_ready_unpin(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        self.sink.start_send_unpin(item.into())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink.poll_ready_unpin(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink.poll_close_unpin(cx)
    }
}

#[async_trait]
pub trait SinkAndStream: Send {
    async fn next(&mut self) -> Option<Result<Message, Error>>;
    async fn send(&mut self, message: Message) -> Result<(), Error>;
}
#[async_trait]
impl<
        T: futures::Sink<Message, Error = Error>
            + futures::Stream<Item = Result<Message, Error>>
            + Send
            + Unpin,
    > SinkAndStream for T
{
    async fn next(&mut self) -> Option<Result<Message, Error>> {
        StreamExt::next(self).await
    }
    async fn send(&mut self, message: Message) -> Result<(), Error> {
        SinkExt::send(self, message).await
    }
}
#[cfg(feature = "tungstenite")]
#[async_trait]
impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin> SinkAndStream
    for tokio_tungstenite::WebSocketStream<S>
{
    async fn next(&mut self) -> Option<Result<Message, Error>> {
        let element = StreamExt::next(self).await?;
        match element {
            Ok(message) => Some(Ok(message.into())),
            Err(err) => Some(Err(err.into())),
        }
    }

    async fn send(&mut self, message: Message) -> Result<(), Error> {
        SinkExt::send(self, message.into()).await?;
        Ok(())
    }
}

pub struct Socket {
    pub stream: Box<dyn SinkAndStream>,
    pub config: WebsocketConfig,
    pub(crate) disconnected: Option<tokio::sync::mpsc::UnboundedSender<Box<dyn Any + Send>>>,
}

impl Socket {
    pub fn new<S: SinkAndStream + 'static>(socket: S, config: WebsocketConfig) -> Self {
        Self {
            stream: Box::new(socket),
            config,
            disconnected: None,
        }
    }
}
