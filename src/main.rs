extern crate env_logger;
extern crate h264_nal_paging;
#[macro_use]
extern crate log;
extern crate tokio;

use std::env;
use std::error::Error;
use std::net::{SocketAddr, SocketAddrV4};
use tokio::signal;

use crate::model::message::Message;

mod downstream;
mod model;
mod still;
mod upstream;
mod utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let target = {
        let target_str = env::var("TARGET")?;
        let (target_addr, target_port) = target_str
            .split_once(':')
            .expect("failed to parse TARGET, not a host:port pair");

        SocketAddr::V4(SocketAddrV4::new(
            target_addr.parse()?,
            target_port.parse()?,
        ))
    };
    let video_port = env::var("VIDEO_PORT")
        .unwrap_or(String::new())
        .parse::<u16>()
        .unwrap_or(1264);

    let ch = upstream::supervisor(target).await;
    let frame_buf = downstream::supervisor(video_port, ch).await;
    still::serve(frame_buf).await;

    signal::ctrl_c().await?;

    Ok(())
}
