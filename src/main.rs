use argh::FromArgs;
use std::path::PathBuf;

#[derive(Debug, FromArgs)]
#[argh(description = "Prototype for running multiple Lua coroutines")]
struct Args {
    #[argh(subcommand)]
    command: Command,
}

#[derive(Debug, FromArgs)]
#[argh(subcommand)]
enum Command {
    Log(LogCommand),
    Serve(ServeCommand),
    Start(StartCommand),
    Stop(StopCommand),
}

#[derive(Debug, FromArgs)]
#[argh(
    subcommand,
    name = "log",
    description = "listen for log messages from a server"
)]
struct LogCommand {}

#[derive(Debug, FromArgs)]
#[argh(
    subcommand,
    name = "serve",
    description = "start a server that runs Lua coroutines"
)]
struct ServeCommand {}

#[derive(Debug, FromArgs)]
#[argh(
    subcommand,
    name = "start",
    description = "start a Lua script as a coroutine on a server"
)]
struct StartCommand {
    #[argh(positional)]
    path: PathBuf,
}

#[derive(Debug, FromArgs)]
#[argh(
    subcommand,
    name = "stop",
    description = "stop a Lua script with the given identifier"
)]
struct StopCommand {
    #[argh(positional)]
    driver_id: String,
}

#[tokio::main]
async fn main() {
    let args: Args = argh::from_env();
    log::debug!("{args:?}");

    match args.command {
        Command::Log(_arg) => uber_client::listen().await.unwrap(),
        Command::Serve(_arg) => uber_server::serve().await.unwrap(),
        Command::Start(arg) => uber_client::start(arg.path.as_path()).await.unwrap(),
        Command::Stop(arg) => uber_client::stop(arg.driver_id).await.unwrap(),
    }
}
