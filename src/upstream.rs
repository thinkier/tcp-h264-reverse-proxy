use std::net::SocketAddr;
use std::time::Duration;

use h264_nal_paging::{H264NalUnit, H264Stream};
use tokio::io::Result as IoResult;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};

pub async fn supervisor(
    addr: SocketAddr,
) -> (JoinHandle<()>, (Sender<H264NalUnit>, Receiver<H264NalUnit>)) {
    let (tx, rx) = broadcast::channel(64);

    let handle = {
        let (tx, mut rx) = (tx.clone(), tx.subscribe());
        let mut last = Instant::now();

        tokio::spawn(async move {
            let mut join = handler(tx.clone(), addr).await;

            loop {
                if let Ok(_) = rx.try_recv() {
                    while let Ok(_) = rx.try_recv() {
                        // Drain the receiver
                    }

                    last = Instant::now();
                }

                if last.duration_since(Instant::now()).as_secs() > 5 {
                    warn!("Upstream {} timed out, spawning new session.", addr);
                    join.abort();
                    join = handler(tx.clone(), addr).await;
                }

                sleep(Duration::from_millis(1)).await;
            }
        })
    };

    return (handle, (tx, rx));
}

pub async fn handler(tx: Sender<H264NalUnit>, addr: SocketAddr) -> JoinHandle<IoResult<()>> {
    tokio::spawn(async move {
        let mut upstream: H264Stream<TcpStream>;

        match TcpStream::connect(addr).await {
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
        while let Ok(unit) = upstream.next().await {
            trace!(
                "Received new unit id:{}, len:{}",
                unit.unit_code,
                unit.raw_bytes.len()
            );

            // Exit if send fails
            if let Err(_) = tx.send(unit) {
                return Ok(());
            }
        }

        // Supervisor will start another instance...
        Ok(())
    })
}
