use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncWriteExt, Result as IoResult};
use tokio::net::TcpListener;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::RwLock;

use crate::model::message::Message;
use crate::utils::abort;

pub async fn supervisor(bind_port: u16, (tx, mut rx): (Sender<Message>, Receiver<Message>)) {
    let cached_bytes = Arc::new(RwLock::new(vec![]));

    {
        let cached_bytes = Arc::clone(&cached_bytes);
        tokio::spawn(async move {
            let mut seq_param = None;
            let mut pic_param = None;

            while let Ok(Message::NalUnit(unit)) = rx.recv().await {
                match unit.unit_code {
                    7 => {
                        seq_param = Some(unit);
                    }
                    8 => {
                        pic_param = Some(unit);
                    }
                    5 => {
                        let mut byte_buf = cached_bytes.write().await;
                        byte_buf.clear();
                        if let Some(ref cached_unit) = &seq_param {
                            byte_buf.extend(&cached_unit.raw_bytes);
                        }
                        if let Some(ref cached_unit) = &pic_param {
                            byte_buf.extend(&cached_unit.raw_bytes);
                        }
                        byte_buf.extend(unit.raw_bytes);
                    }
                    _ => cached_bytes.write().await.extend(unit.raw_bytes),
                }
            }

            debug!("[cacher] Caught error or abort, exiting...");
        });
    }

    tokio::spawn(async move {
        let listener = TcpListener::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            bind_port,
        ))
        .await?;

        loop {
            let cached_bytes = Arc::clone(&cached_bytes);
            let mut rx = tx.subscribe();

            let next = tokio::select! {
                next = listener.accept() => next,
                _ = abort(tx.subscribe()) => {
                    debug!("[listener] Caught abort, exiting...");
                    return Ok(());
                }
            };

            match next {
                Ok((mut sock, addr)) => tokio::spawn(async move {
                    let bytes = { (cached_bytes.read().await.as_ref() as &Vec<u8>).clone() };
                    if let Err(e) = sock.write_all(&bytes).await {
                        error!("{} was disconnected due to {}", addr, e);
                        return;
                    }

                    while let Ok(Message::NalUnit(unit)) = rx.recv().await {
                        if let Err(e) = sock.write_all(&unit.raw_bytes).await {
                            error!("{} was disconnected due to {}", addr, e);
                            return;
                        }
                    }

                    debug!("[socket task {}] Caught error or abort, exiting...", addr);
                }),
                Err(e) => return Err(e) as IoResult<()>,
            };
        }
    });
}
