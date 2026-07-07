//! Server-Sent Events (SSE) parser for reqwest HTTP client.
//!
//! Adapted from pi-agent-rust (src/sse.rs).
//! Implements the SSE protocol on top of reqwest's streaming response.

use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

const MAX_EVENT_DATA_BYTES: usize = 100 * 1024 * 1024;

/// A parsed SSE event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

impl Default for SseEvent {
    fn default() -> Self {
        Self {
            event: "message".to_string(),
            data: String::new(),
            id: None,
            retry: None,
        }
    }
}

/// Wraps a byte stream into an SSE event stream.
pub struct SseStream {
    inner: Pin<Box<dyn Stream<Item = std::io::Result<Vec<u8>>> + Send>>,
    buffer: Vec<u8>,
    max_event_data_bytes: usize,
}

impl SseStream {
    /// Create a new SSE stream from a byte stream.
    pub fn new(stream: Pin<Box<dyn Stream<Item = std::io::Result<Vec<u8>>> + Send>>) -> Self {
        Self {
            inner: stream,
            buffer: Vec::new(),
            max_event_data_bytes: MAX_EVENT_DATA_BYTES,
        }
    }

    /// Attempt to parse the next complete SSE event from the buffer.
    fn parse_event(&mut self) -> Option<SseEvent> {
        if let Some(pos) = self.buffer.windows(2).position(|w| w == b"\n\n") {
            let consumed = pos + 2;
            let event_str = String::from_utf8_lossy(&self.buffer[..pos]);

            let mut event = SseEvent::default();
            let mut data_lines: Vec<&str> = Vec::new();

            for line in event_str.lines() {
                if let Some(value) = line.strip_prefix("event:") {
                    event.event = value.trim().to_string();
                } else if let Some(value) = line.strip_prefix("data:") {
                    let data = value.strip_prefix(' ').unwrap_or(value);
                    data_lines.push(data);
                } else if let Some(value) = line.strip_prefix("id:") {
                    event.id = Some(value.trim().to_string());
                } else if let Some(value) = line.strip_prefix("retry:") {
                    event.retry = value.trim().parse().ok();
                }
                // Comments (lines starting with ':') are ignored per SSE spec
            }

            // Rejoin data lines with newlines
            if !data_lines.is_empty() {
                event.data = data_lines.join("\n");
            }

            // Remove consumed bytes from buffer
            self.buffer.drain(..consumed);
            Some(event)
        } else {
            // Cap buffer size to prevent unbounded growth
            if self.buffer.len() > self.max_event_data_bytes {
                // Truncate: drain everything and return an error-like event
                self.buffer.clear();
                return Some(SseEvent {
                    event: "error".to_string(),
                    data: "SSE event data exceeded maximum size".to_string(),
                    id: None,
                    retry: None,
                });
            }
            None
        }
    }
}

impl Stream for SseStream {
    type Item = std::io::Result<SseEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // First, try to parse an event from existing buffer
        if let Some(event) = self.parse_event() {
            return Poll::Ready(Some(Ok(event)));
        }

        // Otherwise, poll the underlying stream for more data
        loop {
            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.buffer.extend_from_slice(&chunk);
                    if let Some(event) = self.parse_event() {
                        return Poll::Ready(Some(Ok(event)));
                    }
                    // Continue polling for more data
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) => {
                    // Underlying stream ended. If there's leftover data, emit it.
                    if !self.buffer.is_empty() {
                        let remaining = String::from_utf8_lossy(&self.buffer).to_string();
                        self.buffer.clear();
                        return Poll::Ready(Some(Ok(SseEvent {
                            event: "message".to_string(),
                            data: remaining,
                            id: None,
                            retry: None,
                        })));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
