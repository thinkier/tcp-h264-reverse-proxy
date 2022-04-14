extern crate env_logger;
extern crate h264_nal_paging;
#[macro_use]
extern crate log;
extern crate tokio;

use std::env;
use std::error::Error;
use std::net::{SocketAddr, SocketAddrV4};

use tokio::signal::unix::*;

use crate::model::message::Message;

mod downstream;
mod model;
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
    let bind_port = env::var("PORT")?.parse::<u16>().unwrap_or(1264);

    let ch = upstream::supervisor(target).await;
    let tx = ch.0.clone();
    downstream::supervisor(bind_port, ch).await;

    // Termination Handlers
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigquit = signal(SignalKind::quit())?;
    tokio::select! {
        _ = sigint.recv() => (),
        _ = sigterm.recv() => (),
        _ = sigquit.recv() => (),
    }
    tx.send(Message::Abort)?;
    debug!("[main] Sent abort signal, exiting...");

    Ok(())
}
