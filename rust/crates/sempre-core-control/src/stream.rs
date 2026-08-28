use std::time::Duration;

use futures_util::StreamExt as _;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{
        Message,
        client::IntoClientRequest as _,
        http::{HeaderValue, header},
        protocol::WebSocketConfig,
    },
};

use crate::{Client, ControlError, MAX_RESPONSE_SIZE};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct EventStream {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Client {
    pub async fn stream(&self, topic: &str) -> Result<EventStream, ControlError> {
        let path = match topic {
            "traffic" => "traffic",
            "memory" => "memory",
            "connections" => "connections",
            "logs" => "logs?level=debug",
            _ => return Err(ControlError::UnsupportedTopic(topic.into())),
        };
        let mut endpoint = self.base.join(path).map_err(|_| {
            ControlError::InvalidMetadata("base URL cannot join stream path".into())
        })?;
        endpoint
            .set_scheme("ws")
            .map_err(|()| ControlError::InvalidMetadata("stream URL scheme is invalid".into()))?;
        let mut request = endpoint.as_str().into_client_request()?;
        request.headers_mut().insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.secret)).map_err(|_| {
                ControlError::InvalidMetadata("core control secret is not a valid header".into())
            })?,
        );
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_RESPONSE_SIZE))
            .max_frame_size(Some(MAX_RESPONSE_SIZE));
        let (socket, _) = tokio::time::timeout(
            CONNECT_TIMEOUT,
            connect_async_with_config(request, Some(config), false),
        )
        .await
        .map_err(|_| ControlError::StreamTimeout)??;
        Ok(EventStream { socket })
    }
}

impl EventStream {
    pub async fn next_json(&mut self) -> Result<Value, ControlError> {
        loop {
            let message = self
                .socket
                .next()
                .await
                .ok_or(ControlError::StreamClosed)??;
            let value = match message {
                Message::Text(data) => serde_json::from_slice(data.as_bytes()),
                Message::Binary(data) => serde_json::from_slice(data.as_ref()),
                Message::Close(_) => return Err(ControlError::StreamClosed),
                _ => continue,
            };
            if let Ok(value) = value {
                return Ok(value);
            }
        }
    }

    pub async fn next_connections(&mut self) -> Result<crate::ConnectionSnapshot, ControlError> {
        let value = self.next_json().await?;
        serde_json::from_value::<crate::model::RawConnections>(value)
            .map(Into::into)
            .map_err(ControlError::Decode)
    }
}

#[cfg(test)]
mod tests {
    use futures_util::SinkExt as _;
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::{
            Message, handshake::server::ErrorResponse, handshake::server::Request,
            handshake::server::Response,
        },
    };

    use super::*;
    use crate::Endpoint;

    #[allow(clippy::result_large_err, clippy::unnecessary_wraps)]
    fn authorize(request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        assert_eq!(request.uri().path(), "/traffic");
        assert_eq!(
            request.headers().get(header::AUTHORIZATION),
            Some(&HeaderValue::from_static("Bearer secret"))
        );
        Ok(response)
    }

    #[tokio::test]
    async fn stream_authenticates_and_skips_invalid_json() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.expect("connection");
            let mut socket = accept_hdr_async(socket, authorize).await.expect("upgrade");
            socket
                .send(Message::Text("not json".into()))
                .await
                .expect("invalid frame");
            socket
                .send(Message::Text(r#"{"up":7,"down":9}"#.into()))
                .await
                .expect("json frame");
        });
        let client = Client::new(Endpoint {
            core: "mihomo".into(),
            protocol: "clash-rest".into(),
            base_url: format!("http://{address}"),
            secret: "secret".into(),
        })
        .expect("client");
        let mut stream = client.stream("traffic").await.expect("stream");
        assert_eq!(
            stream.next_json().await.expect("event"),
            serde_json::json!({ "up": 7, "down": 9 })
        );
        server.await.expect("server");
    }
}
