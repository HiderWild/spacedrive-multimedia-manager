use clap::Parser;
use std::path::PathBuf;
use tokio::signal;

/// Validate instance name to prevent path traversal attacks
fn validate_instance_name(instance: &str) -> Result<(), String> {
	if instance.is_empty() {
		return Err("Instance name cannot be empty".to_string());
	}
	if instance.len() > 64 {
		return Err("Instance name too long (max 64 characters)".to_string());
	}
	if !instance
		.chars()
		.all(|c| c.is_alphanumeric() || c == '-' || c == '_')
	{
		return Err("Instance name contains invalid characters. Only alphanumeric, dash, and underscore allowed".to_string());
	}
	Ok(())
}

#[derive(Parser, Debug)]
#[command(name = "spacedrive-daemon", about = "Spacedrive daemon")]
struct Args {
	/// Path to spacedrive data directory
	#[arg(long)]
	data_dir: Option<PathBuf>,

	/// Daemon instance name
	#[arg(long)]
	instance: Option<String>,

	/// Internal helper mode for isolated Windows recycle-bin operations.
	#[arg(long, hide = true)]
	internal_trash_helper: bool,

	/// Target path for the internal trash helper mode.
	#[arg(long, hide = true, requires = "internal_trash_helper")]
	internal_trash_path: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
enum RequestedMode {
	Server,
	InternalTrashHelper(PathBuf),
}

impl Args {
	fn requested_mode(&self) -> Result<RequestedMode, String> {
		if self.internal_trash_helper {
			let path = self
				.internal_trash_path
				.clone()
				.ok_or_else(|| "internal trash helper requires --internal-trash-path".to_string())?;
			return Ok(RequestedMode::InternalTrashHelper(path));
		}

		Ok(RequestedMode::Server)
	}
}

#[cfg(target_os = "windows")]
fn run_internal_trash_helper(path: PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	trash::delete(&path).map_err(|error| {
		let message = format!("Failed to move to trash '{}': {}", path.display(), error);
		Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::new(
			std::io::ErrorKind::Other,
			message,
		))
	})
}

#[cfg(not(target_os = "windows"))]
fn run_internal_trash_helper(
	_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	Err(Box::new(std::io::Error::new(
		std::io::ErrorKind::Unsupported,
		"internal trash helper is only supported on Windows",
	)))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let args = Args::parse();
	let configured_socket_addr = std::env::var("SD_SOCKET_ADDR")
		.ok()
		.filter(|addr| !addr.trim().is_empty());

	match args.requested_mode().map_err(|error| {
		Box::<dyn std::error::Error + Send + Sync>::from(std::io::Error::new(
			std::io::ErrorKind::InvalidInput,
			error,
		))
	})? {
		RequestedMode::InternalTrashHelper(path) => {
			run_internal_trash_helper(path)?;
			return Ok(());
		}
		RequestedMode::Server => {}
	}

	// Resolve base data directory
	let base_data_dir = args
		.data_dir
		.unwrap_or(sd_core::config::default_data_dir()?);

	// Calculate instance-specific data directory and socket address
	let (data_dir, socket_addr) = if let Some(instance) = args.instance {
		// Validate instance name for security
		validate_instance_name(&instance).map_err(|e| format!("Invalid instance name: {}", e))?;

		// Each instance gets its own data directory
		let instance_data_dir = base_data_dir.join("instances").join(&instance);

		// Use a simple hash of the instance name to derive a port
		let port = 6970 + (instance.bytes().map(|b| b as u16).sum::<u16>() % 1000);
		let socket_addr = format!("127.0.0.1:{}", port);

		(
			instance_data_dir,
			configured_socket_addr.clone().unwrap_or_else(|| socket_addr),
		)
	} else {
		// Default instance uses the base data directory and port 8488
		let socket_addr = configured_socket_addr.unwrap_or_else(|| "127.0.0.1:8488".to_string());
		(base_data_dir.clone(), socket_addr)
	};

	// Set up signal handling for graceful shutdown
	let ctrl_c = async {
		signal::ctrl_c()
			.await
			.expect("failed to install Ctrl+C handler");
	};

	#[cfg(unix)]
	let terminate = async {
		signal::unix::signal(signal::unix::SignalKind::terminate())
			.expect("failed to install signal handler")
			.recv()
			.await;
	};

	#[cfg(not(unix))]
	let terminate = std::future::pending::<()>();

	// Run the daemon server with signal handling
	tokio::select! {
		result = sd_core::infra::daemon::bootstrap::start_default_server(
			socket_addr,
			data_dir,
			true, // Always enable networking
		) => {
			result
		}
		() = ctrl_c => {
			println!("Received Ctrl+C, shutting down gracefully...");
			Ok(())
		}
		() = terminate => {
			println!("Received SIGTERM, shutting down gracefully...");
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{Args, RequestedMode};
	use clap::Parser;
	use std::path::PathBuf;

	#[test]
	fn parses_internal_trash_helper_mode() {
		let args = Args::try_parse_from([
			"sd-daemon",
			"--internal-trash-helper",
			"--internal-trash-path",
			"D:\\AI",
		])
		.expect("helper args should parse");

		assert_eq!(
			args.requested_mode().expect("helper mode should resolve"),
			RequestedMode::InternalTrashHelper(PathBuf::from("D:\\AI"))
		);
	}

	#[test]
	fn defaults_to_server_mode() {
		let args = Args::try_parse_from(["sd-daemon"]).expect("default args should parse");

		assert_eq!(
			args.requested_mode().expect("default mode should resolve"),
			RequestedMode::Server
		);
	}
}
