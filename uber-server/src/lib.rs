pub use crate::{executor::Executor, listener::Listener, service::Service};
use thiserror::Error;
use tokio::task::LocalSet;
use uber_protos::driver_server::DriverServer;

#[derive(Debug, Error)]
pub enum UberServerError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Lua error: {0}")]
    LuaError(#[from] mlua::Error),
    #[error("tonic transport error: {0}")]
    TransportError(#[from] tonic::transport::Error),
}

pub async fn serve() -> Result<(), UberServerError> {
    let local_set = LocalSet::new();

    local_set
        .run_until(async move {
            let incoming = Listener::new()?;
            let service = Service::new()?;

            log::info!("starting service");

            tonic::transport::Server::builder()
                .add_service(DriverServer::new(service))
                .serve_with_incoming(incoming)
                .await?;

            Ok(())
        })
        .await
}

mod executor;
mod listener;
mod service;
mod unixstream;
