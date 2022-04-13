extern crate env_logger;
extern crate h264_nal_paging;
#[macro_use]
extern crate log;
extern crate tokio;

use std::env;
use std::error::Error;
use std::net::SocketAddr;

use tokio::signal;

mod downstream;
mod upstream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let target = env::var("TARGET_ADDR")?
        .parse()
        .map(|a| SocketAddr::V4(a))?;
    let bind_port = env::var("PORT")?.parse::<u16>().unwrap_or(1264);

    let (up, ch) = upstream::supervisor(target).await;
    let (cache, down) = downstream::supervisor(bind_port, ch).await;

    // Ctrl+C Handlers
    signal::ctrl_c().await?;
    cache.abort();
    up.abort();
    down.abort();

    Ok(())
}
