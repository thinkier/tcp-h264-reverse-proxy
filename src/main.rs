#[macro_use]
extern crate argh;
extern crate h264_nal_paging;
#[macro_use]
extern crate log;
extern crate tokio;

use std::error::Error;

use tokio::signal;

use crate::listener::task_spawner;
use crate::model::cli::CliArgs;

mod model;
mod listener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let args: CliArgs = argh::from_env();

	let handles = task_spawner(args.subnet, args.port).await;

	signal::ctrl_c().await?;
	for h in handles {
		h.abort();
	}

	Ok(())
}
