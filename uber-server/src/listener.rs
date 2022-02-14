use crate::unixstream::UnixStream;
use futures_core::{ready, Stream};
use std::{path::Path, task::Poll};
use tokio::net::UnixListener;

pub struct Listener {
    inner: UnixListener,
}

impl Listener {
    pub fn new() -> std::io::Result<Self> {
        let path = Path::new("/tmp/uber-driver.sock");
        if path.exists() {
            let _ = std::fs::remove_file(path)?;
        }
        let inner = UnixListener::bind(path)?;

        Ok(Self { inner })
    }
}

impl Stream for Listener {
    type Item = std::io::Result<UnixStream>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Poll::Ready(Some(
            ready!(self.inner.poll_accept(cx)).map(|(stream, _)| UnixStream(stream)),
        ))
    }
}
