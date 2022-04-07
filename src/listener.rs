use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use h264_nal_paging::{H264NalUnit, H264Stream};
use ipnet::{IpAdd, Ipv4Net};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub async fn task_spawner(subnet: Ipv4Net, port: u16) -> Vec<JoinHandle<tokio::io::Result<()>>> {
	let addr = subnet.addr();
	let suffix_len = 32 - subnet.prefix_len();

	let mut stack = vec![];

	for i in 1..(1 << suffix_len) {
		let addr = SocketAddrV4::new(addr.clone().saturating_add(i as u32), port);
		let (tx, mut rx) = mpsc::channel(4);

		let listener = tokio::spawn(async move {
			let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), i));
			let sock = TcpSocket::new_v4().unwrap();
			sock.bind(bind_addr)?;

			let listener = sock.listen(4)?;
			loop {
				let listened = listener.accept().await;

				match listened {
					Ok((sock, _)) => {
						let _ = tx.send(sock).await;
					}
					Err(e) => error!("{:?}",e)
				}
			}

			Result::<(), tokio::io::Error>::Ok(())
		});

		stack.push(listener);

		let upstream = tokio::spawn(async move {
			let mut upstream: Option<H264Stream<TcpStream>> = None;
			let mut downstreams: Vec<TcpStream> = vec![];

			let mut stream_init_buf: Vec<Option<H264NalUnit>> = vec![None, None];
			let mut frame_buf: Vec<H264NalUnit> = vec![];

			loop {
				let mut reinstantiate = true;

				if downstreams.is_empty() {
					reinstantiate = false;
					sleep(Duration::from_millis(50)).await;
				} else {
					if let Some(ref mut upstream) = &mut upstream {
						if let Ok(unit) = upstream.next().await {
							reinstantiate = false;

							let clients = downstreams.len();
							for j in 1..=clients {
								let i = clients - j;
								let downstream = &mut downstreams[i];
								// Drop connection on write failure
								if let Err(_) = downstream.write_all(&unit.raw_bytes).await {
									info!("Disconnected {:?}", downstream.peer_addr());
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
								_ => frame_buf.push(unit)
							}
						}
					}
				}

				if reinstantiate {
					info!("Connecting to upstream {:?}", addr);
					upstream = TcpSocket::new_v4().unwrap()
						.connect(SocketAddr::V4(addr))
						.await
						.ok()
						.map(|stream| H264Stream::new(stream));
				}

				'collector: loop {
					match rx.try_recv() {
						Ok(mut downstream) => {
							info!("Proxying     {:?} <= {:?}", downstream.peer_addr(), addr);
							let _ = downstream.write_all(&stream_init_buf.iter()
								.filter(|f| f.is_some())
								.flat_map(|f| f.as_ref().unwrap().raw_bytes.clone())
								.collect::<Vec<u8>>()).await;
							let _ = downstream.write_all(&frame_buf.iter()
								.flat_map(|f| f.raw_bytes.clone())
								.collect::<Vec<u8>>()).await;
							downstreams.push(downstream);
						}
						Err(TryRecvError::Empty) => break 'collector,
						Err(e) => error!("{:?}", e)
					}
				}
			}

			Result::<(), tokio::io::Error>::Ok(())
		});
		stack.push(upstream);
	}

	return stack;
}
