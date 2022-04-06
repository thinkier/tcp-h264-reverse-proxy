use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};

use ipnet::{IpAdd, Ipv4Net};
use tokio::net::TcpSocket;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::error::TryRecvError;

pub async fn task_spawner(subnet: Ipv4Net, port: u16) {
	let addr = subnet.addr();
	let suffix_len = 32 - subnet.prefix_len();

	let mut listener_stack = vec![];
	let mut upstream_stack = vec![];

	for i in 1..(1 << suffix_len) {
		let addr = SocketAddrV4::new(addr.clone().saturating_add(i as u32), port);
		let (tx, mut rx) = channel(4);

		let listener = tokio::spawn(async move {
			let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), i));
			let sock = TcpSocket::new_v4().unwrap();
			sock.bind(bind_addr);

			let listener = sock.listen(4)?;
			while let listened = listener.accept().await {
				match listened {
					Ok((sock, _)) => {
						tx.send(sock).await;
					}
					Err(e) => error!("{:?}",e)
				}
			}

			Result::<(), tokio::io::Error>::Ok(())
		});

		listener_stack.push(listener);

		let upstream = tokio::spawn(async move {
			let mut downstreams = vec![];
			loop {
				let mut upstream = TcpSocket::new_v4().unwrap().connect(SocketAddr::V4(addr)).await?;
				'collector: while let recv = rx.try_recv() {
					match recv {
						Ok(downstream) => {
							info!("Proxying {:?} <= {:?}", downstream.peer_addr(), upstream.peer_addr());
							downstreams.push(downstream)
						}
						Err(TryRecvError::Empty) => break 'collector,
						Err(e) => error!("{:?}", e)
					}
				}
			}

			Result::<(), tokio::io::Error>::Ok(())
		});
		upstream_stack.push(upstream);
	}
}
