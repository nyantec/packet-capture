mod error;
mod config;

use error::{Error, Result, pcap_to_io_error};
use config::Config;

use std::io::{Read, Write};
use std::sync::mpsc;
use std::{fs, thread};
use std::time::{SystemTime, UNIX_EPOCH};
use std::borrow::Cow;
use std::collections::HashMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use log_writer::{LogWriterCallbacks, LogWriter};
use pcap_file::{TsResolution, PcapReader};
use pcap_file::pcap::{Packet, PacketHeader, PcapHeader};
use afpacket::sync::RawPacketStream;
use byteorder::BigEndian;
use log::*;
use getopts::Options;

#[derive(Clone, Debug)]
struct PcapCallbacks;
impl LogWriterCallbacks for PcapCallbacks {
	fn start_file(&mut self, writer: &mut LogWriter<Self>) -> std::result::Result<(), std::io::Error> {
		let header: PcapHeader = Default::default();
		header.write_to::<_, BigEndian>(writer).map_err(pcap_to_io_error)?;
		Ok(())
	}
	fn end_file(&mut self, _writer: &mut LogWriter<Self>) -> std::result::Result<(), std::io::Error> {
		Ok(())
	}
}

fn write_packet<W: Write>(packet: Packet, writer: &mut W) -> Result {
	let mut buf = Vec::<u8>::new();
	packet.header.write_to::<_, BigEndian>(&mut buf, TsResolution::MicroSecond).map_err(pcap_to_io_error)?;
	buf.write_all(&packet.data)?;
	writer.write_all(&buf)?;
	Ok(())
}

fn iface_thread(interface: String, tx: mpsc::Sender<(String, Packet)>) -> Result<()> {
	let mut reader = RawPacketStream::new()?;
	reader.bind(&interface)?;

	// We just assume nothing will be bigger than 2^16 bytes
	let mut buf = [0u8; 65536];

	loop {
		if let Ok(bytes_read) = reader.read(&mut buf) {
			let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
			let packet = Packet {
				header: PacketHeader {
					ts_sec: timestamp.as_secs() as u32,
					ts_nsec: timestamp.subsec_nanos() as u32,
					incl_len: bytes_read as u32,
					orig_len: bytes_read as u32,
				},
				data: Cow::Owned(buf[0..bytes_read].to_vec())
			};
			tx.send((interface.clone(), packet)).map_err(|_| Error::ChannelSend)?;
		} else {
			warn!("read error");
		}
	}
}

fn unix_listener_thread(path: String, tx: mpsc::Sender<(String, Packet<'static>)>) -> Result<()> {
	let _result = fs::remove_file(Path::new(&path));
	fs::create_dir_all(Path::new(&path).parent().unwrap())?;
	let listener = UnixListener::bind(&path)?;
	for stream in listener.incoming() {
		if let Ok(stream) = stream {
			let tx = tx.clone();
			thread::spawn(move || -> Result<()> {
				unix_stream_thread(stream, tx)
			});
		}
	}
	Ok(())
}

fn unix_stream_thread(stream: UnixStream, tx: mpsc::Sender<(String, Packet)>) -> Result<()> {
	let reader = PcapReader::new(stream).map_err(pcap_to_io_error)?;
	for pcap in reader {
		let mut pcap = pcap.map_err(pcap_to_io_error)?;
		let tag_len = pcap.data[0];
		let tag_end = 1 + (tag_len as usize);
		let tag = String::from_utf8_lossy(&pcap.data[1..tag_end].to_vec()).to_string();
		pcap.data = Cow::Owned(pcap.data[tag_end..pcap.header.incl_len as usize].to_vec());
		pcap.header.orig_len -= (tag_len as u32) + 1;
		pcap.header.incl_len -= (tag_len as u32) + 1;
		tx.send((tag, pcap)).map_err(|_| Error::ChannelSend)?;
	}
	Ok(())
}

impl Config {
	pub fn run(&self) -> Result {
		let (tx, rx) = mpsc::channel();

		for interface in self.interfaces.iter() {
			let interface = interface.clone();
			let tx = tx.clone();
			thread::spawn(move || {
				iface_thread(interface, tx)
			});
		}
		if let Some(path) = &self.unix_socket {
			let path = path.clone();
			let tx = tx.clone();
			thread::spawn(move || {
				unix_listener_thread(path, tx)
			});
		}

		let mut writers = HashMap::new();
		loop {
			let (stream_name, mut packet) = rx.recv()?;
			let max_incl_len = self.max_incl_len.get(&stream_name).unwrap_or(&self.default_max_incl_len);
			let incl_len = std::cmp::min(packet.header.incl_len as usize, *max_incl_len);
			packet.header.incl_len = incl_len as u32;
			let truncated_data = Cow::Owned(packet.data[0..incl_len].to_vec());
			packet.data = truncated_data;

			if !writers.contains_key(&stream_name) {
				// first packet, create writer
				let mut log_writer_config = self.log_writer_config.clone();
				log_writer_config.suffix = format!("-{}.pcap", stream_name);
				let new_writer = LogWriter::new_with_callbacks(log_writer_config, PcapCallbacks)?;
				writers.insert(stream_name.clone(), new_writer);
			}
			let writer = writers.get_mut(&stream_name).unwrap();
			write_packet(packet, writer)?;
			// flush ?
			//for (_stream_name, writer) in &mut writers {
			//	writer.flush()?;
			//}
		}
	}
}

fn print_usage(program: &str, opts: Options) {
	let brief = format!("Usage: {} CONFIG [options]", program);
	print!("{}", opts.usage(&brief));
}

fn main() {
	let args: Vec<String> = std::env::args().collect();
	let program = args[0].clone();

	let mut opts = Options::new();
	opts.optflag("h", "help", "Display this help text and exit");

	let matches = match opts.parse(&args[1..]) {
		Ok(m) => m,
		Err(f) => {
			panic!("{}", f)
		}
	};

	if matches.opt_present("h") {
		print_usage(&program, opts);
		return;
	}

	let config_path = if !matches.free.is_empty() {
		matches.free[0].clone()
	} else {
		print_usage(&program, opts);
		return;
	};

	env_logger::init();

	let config_str = fs::read_to_string(&config_path).unwrap();
	let config: Config = serde_yaml::from_str(&config_str).unwrap();
	config.run().unwrap();
}
