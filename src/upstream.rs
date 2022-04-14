use std::net::SocketAddr;
use std::time::Duration;

use h264_nal_paging::H264Stream;
use tokio::io::Result as IoResult;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};

use crate::model::message::Message;
use crate::utils::abort;

pub async fn supervisor(addr: SocketAddr) -> (Sender<Message>, Receiver<Message>) {
    let (tx, rx) = broadcast::channel(64);

    {
        let (tx, mut rx) = (tx.clone(), tx.subscribe());

        tokio::spawn(async move {
            let mut join = handler(tx.clone(), addr).await;
            let mut last = Instant::now();

            trace!("Upstream supervisor started");
            loop {
                let last_packet_at = Instant::now().duration_since(last);
                match rx.try_recv() {
                    Ok(Message::NalUnit(_)) => {
                        while let Ok(_) = rx.try_recv() {
                            // Drain the receiver
                        }

                        trace!(
                            "New unit received, last upstream unit was {}ms ago",
                            last_packet_at.as_millis()
                        );
                        last = Instant::now();
                    }
                    Ok(Message::Abort) => {
                        debug!("[upstream supervisor] Caught abort, exiting...");
                        return;
                    }
                    _ => {}
                }

                if last_packet_at.as_secs() > 5 {
                    warn!("Upstream {} timed out, spawning new session.", addr);
                    join.abort();
                    join = handler(tx.clone(), addr).await;
                    last = Instant::now();
                }

                sleep(Duration::from_millis(1)).await;
            }
        });
    }

    return (tx, rx);
}

async fn handler(tx: Sender<Message>, addr: SocketAddr) -> JoinHandle<IoResult<()>> {
    tokio::spawn(async move {
        let mut upstream: H264Stream<TcpStream>;

        let conn = tokio::select! {
            c = TcpStream::connect(addr) => c,
            _ = abort(tx.subscribe()) => {
                debug!("[upstream connect] Caught abort, exiting...");
                return Ok(());
            }
        };

        match conn {
            Ok(s) => {
                info!("Connected to upstream {}", addr);

                upstream = H264Stream::new(s)
            }
            Err(e) => {
                warn!("Failed to connect to upstream {}: {}", addr, e);
                return Err(e);
            }
        };

        trace!("Blocking on upstream read task");

        loop {
            let next = tokio::select! {
                n = upstream.next() => n,
                _ = abort(tx.subscribe()) => {
                    debug!("[upstream] Caught abort, exiting...");
                    return Ok(());
                }
            };

            match next {
                Ok(unit) => {
                    trace!(
                        "Received new unit id:{}, len:{}",
                        unit.unit_code,
                        unit.raw_bytes.len()
                    );

                    // Exit if send fails
                    if let Err(_) = tx.send(Message::NalUnit(unit)) {
                        return Ok(());
                    }
                }
                Err(e) => return Err(e),
            }
        }
    })
}
