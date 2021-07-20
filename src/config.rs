use log_writer::LogWriterConfig;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
	pub unix_socket: Option<String>,
	pub max_incl_len: HashMap<String, usize>,
	pub default_max_incl_len: usize,
	pub interfaces: Vec<String>,
	pub log_writer_config: LogWriterConfig,
}
