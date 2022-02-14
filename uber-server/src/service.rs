use tokio::sync::{mpsc, Mutex};
use uber_protos::{driver_server::Driver, DriverResponse, StartDriverRequest, StopDriverRequest};

use crate::{executor::Executor, UberServerError};

pub struct Service {
    request_tx: mpsc::Sender<ExecutorRequest>,
    response_rx: Mutex<mpsc::Receiver<DriverResponse>>,
}

#[derive(Debug)]
enum ExecutorRequest {
    Start(StartDriverRequest),
    Stop(StopDriverRequest),
}

impl Service {
    pub fn new() -> Result<Self, UberServerError> {
        let (request_tx, mut request_rx) = mpsc::channel(1);
        let (response_tx, response_rx) = mpsc::channel(1);
        let response_rx = Mutex::new(response_rx);
        let mut executor = Executor::new()?;

        tokio::task::spawn_local(async move {
            while let Some(request) = request_rx.recv().await {
                let response_tx = response_tx.clone();
                let response = match request {
                    ExecutorRequest::Start(StartDriverRequest { driver_id, payload }) => {
                        let error = match executor.create_coroutine(driver_id.clone(), payload) {
                            Ok(()) => None,
                            Err(error) => Some(error.to_string()),
                        };

                        DriverResponse { driver_id, error }
                    }
                    ExecutorRequest::Stop(StopDriverRequest { driver_id }) => {
                        executor.kill_coroutine(driver_id)
                    }
                };

                response_tx
                    .send(response)
                    .await
                    .expect("service channel dropped");
            }
        });

        Ok(Self {
            request_tx,
            response_rx,
        })
    }
}

#[tonic::async_trait]
impl Driver for Service {
    async fn start_driver(
        &self,
        request: tonic::Request<StartDriverRequest>,
    ) -> Result<tonic::Response<DriverResponse>, tonic::Status> {
        let request = request.into_inner();

        log::info!("start_driver {request:?}");

        self.request_tx
            .send(ExecutorRequest::Start(request))
            .await
            .expect("executor channel dropped");

        let response = self
            .response_rx
            .lock()
            .await
            .recv()
            .await
            .expect("executor channel dropped");

        Ok(tonic::Response::new(response))
    }

    async fn stop_driver(
        &self,
        request: tonic::Request<StopDriverRequest>,
    ) -> Result<tonic::Response<DriverResponse>, tonic::Status> {
        let request = request.into_inner();

        log::info!("stop_driver {request:?}");

        self.request_tx
            .send(ExecutorRequest::Stop(request))
            .await
            .expect("executor channel dropped");

        let response = self
            .response_rx
            .lock()
            .await
            .recv()
            .await
            .expect("executor channel dropped");

        Ok(tonic::Response::new(response))
    }
}
