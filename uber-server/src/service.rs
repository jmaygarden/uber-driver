use crate::{executor::Executor, logger::LogSubscriber, UberServerError};
use futures_core::Stream;
use std::pin::Pin;
use tokio::sync::{mpsc, Mutex};
use uber_protos::{
    driver_server::Driver, DriverResponse, LogEvent, StartDriverRequest, StopDriverRequest,
};

pub type LogSender = mpsc::UnboundedSender<Result<LogEvent, tonic::Status>>;

pub struct Service {
    request_tx: mpsc::Sender<ExecutorRequest>,
    response_rx: Mutex<mpsc::Receiver<DriverResponse>>,
}

#[derive(Debug)]
enum ExecutorRequest {
    Log(LogSender),
    Start(StartDriverRequest),
    Stop(StopDriverRequest),
}

impl Service {
    pub fn new(mut log_subscriber: LogSubscriber) -> Result<Self, UberServerError> {
        let (request_tx, mut request_rx) = mpsc::channel(1);
        let (response_tx, response_rx) = mpsc::channel(1);
        let response_rx = Mutex::new(response_rx);
        let mut executor = Executor::new()?;

        tokio::task::spawn_local(async move {
            while let Some(request) = request_rx.recv().await {
                let response_tx = response_tx.clone();
                let response = match request {
                    ExecutorRequest::Log(log_tx) => {
                        log_subscriber.push(log_tx);

                        None
                    }
                    ExecutorRequest::Start(StartDriverRequest { driver_id, payload }) => {
                        let error = match executor.create_coroutine(driver_id.clone(), payload) {
                            Ok(()) => None,
                            Err(error) => Some(error.to_string()),
                        };

                        Some(DriverResponse { driver_id, error })
                    }
                    ExecutorRequest::Stop(StopDriverRequest { driver_id }) => {
                        Some(executor.kill_coroutine(driver_id))
                    }
                };

                if let Some(response) = response {
                    response_tx
                        .send(response)
                        .await
                        .expect("service channel dropped");
                }
            }
        });

        Ok(Self {
            request_tx,
            response_rx,
        })
    }

    async fn send(&self, request: ExecutorRequest) -> Result<(), tonic::Status> {
        self.request_tx
            .send(request)
            .await
            .map_err(|error| tonic::Status::internal(error.to_string()))
    }

    async fn execute(
        &self,
        request: ExecutorRequest,
    ) -> Result<tonic::Response<DriverResponse>, tonic::Status> {
        self.send(request).await?;

        let response = self
            .response_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| tonic::Status::internal("connection dropped"))?;

        Ok(tonic::Response::new(response))
    }
}

#[tonic::async_trait]
impl Driver for Service {
    type LogEventsStream = Pin<Box<dyn Stream<Item = Result<LogEvent, tonic::Status>> + Send>>;

    async fn start_driver(
        &self,
        request: tonic::Request<StartDriverRequest>,
    ) -> Result<tonic::Response<DriverResponse>, tonic::Status> {
        let request = request.into_inner();

        log::info!("start_driver {request:?}");

        self.execute(ExecutorRequest::Start(request)).await
    }

    async fn stop_driver(
        &self,
        request: tonic::Request<StopDriverRequest>,
    ) -> Result<tonic::Response<DriverResponse>, tonic::Status> {
        let request = request.into_inner();

        log::info!("stop_driver {request:?}");

        self.execute(ExecutorRequest::Stop(request)).await
    }

    async fn log_events(
        &self,
        _request: tonic::Request<()>,
    ) -> Result<tonic::Response<Self::LogEventsStream>, tonic::Status> {
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

        log::info!("log events stream");

        self.send(ExecutorRequest::Log(tx)).await?;

        Ok(tonic::Response::new(Box::pin(rx)))
    }
}
