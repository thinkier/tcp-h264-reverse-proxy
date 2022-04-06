#[macro_use]
extern crate argh;
#[macro_use]
extern crate log;
extern crate tokio;
extern crate h264_nal_paging;

use std::error::Error;

use crate::listener::task_spawner;
use crate::model::cli::CliArgs;

mod model;
mod listener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let args: CliArgs = argh::from_env();

	task_spawner(args.subnet, args.port).await;

	Ok(())
}
