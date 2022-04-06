use std::str::FromStr;

use ipnet::Ipv4Net;

#[derive(FromArgs)]
/// proxy and serve multiple h.264 streams over TCP
pub struct CliArgs {
	#[argh(option, short = 's', default = "Ipv4Net::from_str(\"192.168.0.0/24\").unwrap()")]
	/// the subnet to target for this proxy server, up to /24 in size
	pub subnet: Ipv4Net,
	#[argh(option, short = 'p', default = "1264")]
	/// the port to target for upstream servers
	pub port: u16,
}
