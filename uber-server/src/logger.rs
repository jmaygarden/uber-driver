use crate::service::LogSender;
use log::{Metadata, Record, SetLoggerError};
use uber_protos::LogLevel;
use std::sync::{Arc, Mutex};

pub struct Logger {
    clients: Arc<Mutex<Vec<LogSender>>>,
    inner: env_logger::Logger,
}

pub struct LogSubscriber {
    clients: Arc<Mutex<Vec<LogSender>>>,
}

pub fn init() -> LogSubscriber {
    try_init().expect("logger::init should not be called after logger initialied")
}

pub fn try_init() -> Result<LogSubscriber, SetLoggerError> {
    let clients: Arc<Mutex<Vec<LogSender>>> = Default::default();
    let subscriber = LogSubscriber {
        clients: clients.clone(),
    };
    let inner = env_logger::Logger::from_default_env();
    let logger = Logger { clients, inner };

    log::set_max_level(logger.inner.filter());
    log::set_boxed_logger(Box::new(logger))?;

    Ok(subscriber)
}

impl LogSubscriber {
    pub fn push(&mut self, client: LogSender) {
        log::info!("forwarding log records to {client:?}");

        let mut clients = self.clients.lock().unwrap();

        clients.push(client)
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        let level = record.level();
        let target = record.target().to_string();
        let args = record.args();
        let message = format!("{args}");
        {
            let record = uber_protos::LogEvent {
                level: LogLevel::from(level) as i32,
                target: target.clone(),
                message: message.clone(),
            };
            let clients = self.clients.lock().unwrap();

            for client in clients.iter() {
                let _ = client.send(Ok(record.clone()));
            }
        }

        self.inner.log(record)
    }

    fn flush(&self) {
        self.inner.flush()
    }
}
