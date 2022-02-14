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
    Serve(ServeCommand),
    Run(RunCommand),
}

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
    name = "run",
    description = "run a Lua script as a coroutine on a server"
)]
struct RunCommand {
    #[argh(positional)]
    path: PathBuf,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args: Args = argh::from_env();
    log::debug!("{args:?}");

    match args.command {
        Command::Serve(_arg) => uber_server::serve().await.unwrap(),
        Command::Run(arg) => uber_client::run(arg.path.as_path()).await.unwrap(),
    }
}
