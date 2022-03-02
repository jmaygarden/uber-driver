use std::path::Path;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
use tower::service_fn;
use uber_protos::{driver_client::DriverClient, StartDriverRequest, StopDriverRequest};

const UDS_PATH: &str = "/tmp/uber-driver.sock";
const UDS_URI: &str = "http://tmp/uber-driver.sock";

#[derive(Debug, Error)]
pub enum UberClientError {
    #[error("I/O error")]
    IoError(#[from] std::io::Error),
    #[error("Lua error")]
    LuaError(#[from] mlua::Error),
    #[error("tonic request status")]
    Status(#[from] tonic::Status),
    #[error("tonic transport error")]
    TransportError(#[from] tonic::transport::Error),
}

async fn read_source(path: &Path) -> Result<Vec<u8>, UberClientError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut source = Vec::new();

    file.read_to_end(&mut source).await?;

    Ok(source)
}

async fn write_bytecode(path: &Path, bytecode: &Vec<u8>) -> Result<(), UberClientError> {
    let path = path.with_extension("luac");
    let mut file = tokio::fs::File::create(path).await?;

    file.write_all(bytecode)
        .await
        .map_err(UberClientError::from)
}

async fn load_script(path: &Path) -> Result<Vec<u8>, UberClientError> {
    let source = read_source(path).await?;
    let lua = mlua::Lua::new();
    let function = lua
        .load(&source)
        .set_name(path.as_os_str().to_str().unwrap())?
        .into_function()?;
    let bytecode = function.dump(false);
    log::debug!("{bytecode:X?}");
    write_bytecode(path, &bytecode).await?;

    Ok(bytecode)
}

pub async fn listen() -> Result<(), UberClientError> {
    env_logger::init();

    let channel = tonic::transport::Endpoint::from_static(UDS_URI)
        .connect_with_connector(service_fn(|_| UnixStream::connect(UDS_PATH)))
        .await?;
    let mut client = DriverClient::new(channel);
    let mut stream = client.log_events(()).await?.into_inner();

    while let Some(message) = stream.message().await? {
        let level = log::Level::from(message.level());
        let target = message.target.as_str();

        log::log!(target: target, level, "{}", message.message);
    }

    Ok(())
}

pub async fn start(path: &Path) -> Result<(), UberClientError> {
    env_logger::init();
    log::info!("start script {path:?}");
    let channel = tonic::transport::Endpoint::from_static(UDS_URI)
        .connect_with_connector(service_fn(|_| UnixStream::connect(UDS_PATH)))
        .await?;
    let mut client = DriverClient::new(channel);
    let driver_id = uuid::Uuid::new_v4().to_string();
    let payload = load_script(path).await?;
    let request = tonic::Request::new(StartDriverRequest { driver_id, payload });
    log::info!("request: {request:?}");
    let response = client.start_driver(request).await?;
    log::info!("response: {response:?}");

    Ok(())
}

pub async fn stop(driver_id: String) -> Result<(), UberClientError> {
    env_logger::init();
    log::info!("stop script {driver_id}");
    let channel = tonic::transport::Endpoint::from_static(UDS_URI)
        .connect_with_connector(service_fn(|_| UnixStream::connect(UDS_PATH)))
        .await?;
    let mut client = DriverClient::new(channel);
    let request = tonic::Request::new(StopDriverRequest { driver_id });
    log::info!("request: {request:?}");
    let response = client.stop_driver(request).await?;
    log::info!("response: {response:?}");

    Ok(())
}
