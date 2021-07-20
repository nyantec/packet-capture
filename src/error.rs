use thiserror::Error as ThisError;
use pcap_file::PcapError;

#[derive(ThisError, Debug)]
pub enum Error {
	#[error(transparent)]
	Io(#[from] std::io::Error),

	#[error(transparent)]
	ParseIntError(#[from] std::num::ParseIntError),

	#[error("Channel send failed")]
	ChannelSend,

	#[error(transparent)]
	ChannelRecv(#[from] std::sync::mpsc::RecvError),
}

pub type Result<T = ()> = std::result::Result<T, Error>;

pub(crate) fn pcap_to_io_error(pcap_error: PcapError) -> std::io::Error {
	match pcap_error {
		PcapError::IoError(e) => e,
		_ => std::io::Error::from(std::io::ErrorKind::Other),
	}
}
