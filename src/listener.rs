use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use h264_nal_paging::{H264NalUnit, H264Stream};
use ipnet::{IpAdd, Ipv4Net};
use tokio::io::AsyncWriteExt;
use tokio::io::Result as IoResult;
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};

pub async fn task_spawner(subnet: Ipv4Net, port: u16) -> Vec<JoinHandle<IoResult<()>>> {
    let addr = subnet.addr();
    let suffix_len = 32 - subnet.prefix_len();

    let mut stack = vec![];

    for i in 1..(1 << suffix_len) {
        let addr = SocketAddrV4::new(addr.clone().saturating_add(i as u32), port);
        let (tx, rx) = mpsc::channel(4);

        let listener = tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), i));
            let sock = TcpSocket::new_v4().unwrap();
            sock.bind(bind_addr)?;

            let listener = sock.listen(4)?;
            loop {
                let listened = listener.accept().await;

                match listened {
                    Ok((sock, addr)) => {
                        let _ = tx.send((sock, addr)).await;
                    }
                    Err(e) => error!("{:?}", e),
                }
            }
        });

        stack.push(listener);

        let upstream = socket_server(addr, rx).await;
        stack.push(upstream);
    }

    return stack;
}

async fn socket_server(
    addr: SocketAddrV4,
    mut input_channel: Receiver<(TcpStream, SocketAddr)>,
) -> JoinHandle<IoResult<()>> {
    tokio::spawn(async move {
        let mut upstream: Option<H264Stream<TcpStream>> = None;
        let mut downstreams: Vec<(TcpStream, SocketAddr)> = vec![];

        let mut stream_init_buf: Vec<Option<H264NalUnit>> = vec![None, None];
        let mut frame_buf: Vec<H264NalUnit> = vec![];

        let mut last_unit = Instant::now();

        loop {
            trace!("Outer Loop");
            let mut reinstantiate = true;

            if upstream.is_some() && Instant::now().duration_since(last_unit).as_secs() > 5 {
                // Reboot the socket if no data was received in 5 secs
                info!("Dropping inactive upstream {:?}", addr);
            } else if downstreams.is_empty() {
                reinstantiate = false;
                sleep(Duration::from_millis(50)).await;
            } else if let Some(ref mut upstream) = &mut upstream {
                trace!("Upstream read");
                if let Ok(unit) = upstream.try_next().await {
                    reinstantiate = false;
                    trace!("Attempting to read from upstream");

                    if let Some(unit) = unit {
                        last_unit = Instant::now();
                        trace!(
                            "Received new unit id:{}, len:{}",
                            unit.unit_code,
                            unit.raw_bytes.len()
                        );

                        let clients = downstreams.len();
                        for j in 1..=clients {
                            let i = clients - j;
                            let (downstream, ds_addr) = &mut downstreams[i];
                            // Drop connection on write failure
                            if let Err(_) = downstream.write_all(&unit.raw_bytes).await {
                                info!("Disconnected {:?}", ds_addr);
                                let _ = downstreams.remove(i);
                            }
                        }

                        match unit.unit_code {
                            7 => {
                                stream_init_buf[0] = Some(unit);
                            }
                            8 => {
                                stream_init_buf[1] = Some(unit);
                            }
                            5 => {
                                frame_buf.clear();
                                frame_buf.push(unit);
                            }
                            _ => frame_buf.push(unit),
                        }
                    } else {
                        // Sleep on it for 100us while waiting for new data
                        sleep(Duration::from_micros(100)).await;
                    }
                } else {
                    trace!("h264 nal paging reported an error that wasn't handled");
                }
            }

            if reinstantiate {
                trace!("Reconnecting to upstream {}", addr);
                upstream = None;

                match TcpStream::connect(SocketAddr::V4(addr)).await {
                    Ok(s) => {
                        info!("Connected to upstream {}", addr);

                        last_unit = Instant::now();
                        upstream = Some(H264Stream::new(s))
                    }
                    Err(e) => {
                        warn!("Failed to connect to upstream {}: {}", addr, e);
                        sleep(Duration::from_secs(5)).await;
                    }
                };
            }

            trace!("New connection collector");
            'collector: loop {
                match input_channel.try_recv() {
                    Ok((mut downstream, ds_addr)) => {
                        info!("Proxying     {:?} <= [cached] <= {:?}", ds_addr, addr);
                        let _ = downstream
                            .write_all(
                                &stream_init_buf
                                    .iter()
                                    .filter(|f| f.is_some())
                                    .flat_map(|f| f.as_ref().unwrap().raw_bytes.clone())
                                    .collect::<Vec<u8>>(),
                            )
                            .await;
                        let _ = downstream
                            .write_all(
                                &frame_buf
                                    .iter()
                                    .flat_map(|f| f.raw_bytes.clone())
                                    .collect::<Vec<u8>>(),
                            )
                            .await;
                        downstreams.push((downstream, ds_addr));
                    }
                    Err(TryRecvError::Empty) => break 'collector,
                    Err(e) => error!("{:?}", e),
                }
            }
        }
    })
}
