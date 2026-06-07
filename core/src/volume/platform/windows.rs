//! Windows-specific volume detection using sysinfo plus Win32 drive metadata.
//!
//! `sysinfo` still covers local disks well, but mapped network drives are
//! exposed as logical drives instead of block devices. Those need an explicit
//! Win32 enumeration pass so they become first-class Spacedrive volumes.

use crate::volume::{
	classification::{get_classifier, VolumeDetectionInfo},
	error::{VolumeError, VolumeResult},
	types::{
		DiskType, FileSystem, MountType, Volume, VolumeDetectionConfig, VolumeFingerprint,
		VolumeType,
	},
	utils,
};
use std::{
	collections::{HashMap, HashSet},
	path::{Path, PathBuf},
};
use tokio::task;
use tracing::debug;
use uuid::Uuid;
use windows_sys::Win32::{
	Foundation::{ERROR_MORE_DATA, NO_ERROR},
	NetworkManagement::WNet::WNetGetConnectionW,
	Storage::FileSystem::{
		GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
	},
};

const DRIVE_REMOTE_TYPE: u32 = 4;

#[derive(Debug, Clone)]
struct WindowsRemoteDriveInfo {
	mount_point: PathBuf,
	label: String,
	file_system_name: String,
	total_space: u64,
	available_space: u64,
	remote_target: Option<String>,
}

/// Detect Windows volumes using sysinfo and Win32 logical drive enumeration.
pub async fn detect_volumes(
	device_id: Uuid,
	config: &VolumeDetectionConfig,
) -> VolumeResult<Vec<Volume>> {
	let config = config.clone();
	task::spawn_blocking(move || {
		let network_volumes = detect_network_mapped_volumes(device_id, &config)?;
		let network_mount_keys: HashSet<String> = network_volumes
			.iter()
			.map(|volume| normalize_mount_key(volume.mount_point.as_path()))
			.collect();

		let disks = sysinfo::Disks::new_with_refreshed_list();
		debug!("sysinfo detected {} disks", disks.list().len());

		let mut volumes = network_volumes;
		for disk in disks.list() {
			let mount_point = disk.mount_point().to_path_buf();
			if network_mount_keys.contains(&normalize_mount_key(mount_point.as_path())) {
				debug!(
					"Skipping sysinfo disk because Win32 remote mapping will own it: {}",
					mount_point.display()
				);
				continue;
			}

			let total_space = disk.total_space();
			let available_space = disk.available_space();
			let fs_name = disk.file_system().to_string_lossy().to_string();
			let label = disk.name().to_string_lossy().to_string();
			let is_removable = disk.is_removable();

			if mount_point.as_os_str().is_empty() {
				debug!("Skipping disk with empty mount point: label={:?}", label);
				continue;
			}

			if total_space == 0 {
				debug!("Skipping disk with zero capacity: {:?}", mount_point);
				continue;
			}

			let name = if label.is_empty() {
				format!(
					"Local Disk ({})",
					trim_trailing_separators(&mount_point.to_string_lossy())
				)
			} else {
				label
			};

			let file_system = utils::parse_filesystem_type(&fs_name);
			let mount_type = determine_mount_type_windows(&mount_point);
			let disk_type = match disk.kind() {
				sysinfo::DiskKind::SSD => DiskType::SSD,
				sysinfo::DiskKind::HDD => DiskType::HDD,
				_ => DiskType::Unknown,
			};

			let volume_type =
				classify_volume(&mount_point, &file_system, is_removable, false, total_space);

			let fingerprint = match volume_type {
				VolumeType::External => {
					if let Some(spacedrive_id) =
						utils::read_or_create_dotfile_sync(&mount_point, device_id, None)
					{
						VolumeFingerprint::from_external_volume(spacedrive_id, device_id)
					} else {
						VolumeFingerprint::from_primary_volume(&mount_point, device_id)
					}
				}
				VolumeType::Network => {
					let path_lossy = mount_point.to_string_lossy();
					VolumeFingerprint::from_network_volume(&path_lossy, &path_lossy)
				}
				_ => VolumeFingerprint::from_primary_volume(&mount_point, device_id),
			};

			let mut volume = Volume::new(device_id, fingerprint, name.clone(), mount_point);
			volume.mount_type = mount_type;
			volume.volume_type = volume_type;
			volume.disk_type = disk_type;
			volume.file_system = file_system;
			volume.total_capacity = total_space;
			volume.available_space = available_space;
			volume.is_read_only = false;

			if should_include_volume(&volume, &config) {
				debug!(
					"Detected volume: {} ({}) - {} bytes",
					volume.name,
					volume.mount_point.display(),
					total_space
				);
				volumes.push(volume);
			}
		}

		Ok(deduplicate_volumes(volumes))
	})
	.await
	.map_err(|e| VolumeError::platform(format!("Task join error: {}", e)))?
}

fn detect_network_mapped_volumes(
	device_id: Uuid,
	config: &VolumeDetectionConfig,
) -> VolumeResult<Vec<Volume>> {
	let roots = enumerate_logical_drives()?;
	let mut volumes_by_identity = HashMap::new();

	for root in roots {
		let drive_type = unsafe { GetDriveTypeW(encode_wide_path(root.as_path()).as_ptr()) };
		if drive_type != DRIVE_REMOTE_TYPE {
			continue;
		}

		let metadata = read_remote_drive_info(root.as_path());
		let volume = build_network_volume(device_id, metadata);
		let identity_key =
			network_identity_key(volume.hardware_id.as_deref(), volume.mount_point.as_path());

		if should_include_volume(&volume, config) {
			volumes_by_identity.entry(identity_key).or_insert(volume);
		}
	}

	Ok(volumes_by_identity.into_values().collect())
}

fn build_network_volume(device_id: Uuid, metadata: WindowsRemoteDriveInfo) -> Volume {
	let fingerprint_source = network_fingerprint_source(
		metadata.remote_target.as_deref(),
		metadata.mount_point.as_path(),
	);
	let fingerprint =
		VolumeFingerprint::from_network_volume(&fingerprint_source, &fingerprint_source);
	let name = network_display_name(
		metadata.label.as_str(),
		metadata.remote_target.as_deref(),
		metadata.mount_point.as_path(),
	);
	let file_system = if metadata.file_system_name.is_empty() {
		FileSystem::SMB
	} else {
		utils::parse_filesystem_type(&metadata.file_system_name)
	};

	let mut volume = Volume::new(device_id, fingerprint, name, metadata.mount_point.clone());
	volume.mount_type = MountType::Network;
	volume.volume_type = VolumeType::Network;
	volume.disk_type = DiskType::Network;
	volume.file_system = file_system;
	volume.total_capacity = metadata.total_space;
	volume.available_space = metadata.available_space;
	volume.is_read_only = false;
	volume.hardware_id = metadata.remote_target;
	volume
}

fn enumerate_logical_drives() -> VolumeResult<Vec<PathBuf>> {
	let required_len = unsafe { GetLogicalDriveStringsW(0, std::ptr::null_mut()) };
	if required_len == 0 {
		return Err(VolumeError::platform(
			"GetLogicalDriveStringsW returned an empty drive list".to_string(),
		));
	}

	let mut buffer = vec![0u16; required_len as usize + 1];
	let written = unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) };
	if written == 0 {
		return Err(VolumeError::platform(
			"GetLogicalDriveStringsW failed to write drive roots".to_string(),
		));
	}

	let mut drives = Vec::new();
	let mut start = 0usize;
	for (index, unit) in buffer.iter().enumerate() {
		if *unit != 0 {
			continue;
		}

		if index == start {
			break;
		}

		drives.push(PathBuf::from(decode_wide_string(&buffer[start..index])));
		start = index + 1;
	}

	Ok(drives)
}

fn read_remote_drive_info(mount_point: &Path) -> WindowsRemoteDriveInfo {
	let (label, file_system_name) = read_volume_label_and_filesystem(mount_point)
		.unwrap_or_else(|| (String::new(), String::new()));
	let (total_space, available_space) = read_drive_space(mount_point).unwrap_or((0, 0));
	let remote_target = resolve_remote_target(mount_point);

	WindowsRemoteDriveInfo {
		mount_point: mount_point.to_path_buf(),
		label,
		file_system_name,
		total_space,
		available_space,
		remote_target,
	}
}

fn read_volume_label_and_filesystem(mount_point: &Path) -> Option<(String, String)> {
	let mount_point_wide = encode_wide_path(mount_point);
	let mut volume_name = vec![0u16; 256];
	let mut file_system_name = vec![0u16; 128];
	let mut serial_number = 0u32;
	let mut max_component_length = 0u32;
	let mut file_system_flags = 0u32;
	let ok = unsafe {
		GetVolumeInformationW(
			mount_point_wide.as_ptr(),
			volume_name.as_mut_ptr(),
			volume_name.len() as u32,
			&mut serial_number,
			&mut max_component_length,
			&mut file_system_flags,
			file_system_name.as_mut_ptr(),
			file_system_name.len() as u32,
		)
	};
	if ok == 0 {
		debug!(
			"GetVolumeInformationW did not return metadata for {}",
			mount_point.display()
		);
		return None;
	}

	Some((
		decode_wide_buffer(&volume_name),
		decode_wide_buffer(&file_system_name),
	))
}

fn read_drive_space(mount_point: &Path) -> Option<(u64, u64)> {
	let mount_point_wide = encode_wide_path(mount_point);
	let mut available_bytes = 0u64;
	let mut total_bytes = 0u64;
	let mut free_bytes = 0u64;
	let ok = unsafe {
		GetDiskFreeSpaceExW(
			mount_point_wide.as_ptr(),
			&mut available_bytes,
			&mut total_bytes,
			&mut free_bytes,
		)
	};
	if ok == 0 {
		debug!(
			"GetDiskFreeSpaceExW did not return capacity for {}",
			mount_point.display()
		);
		return None;
	}

	Some((total_bytes, available_bytes))
}

fn resolve_remote_target(mount_point: &Path) -> Option<String> {
	let drive_name = trim_trailing_separators(&mount_point.to_string_lossy()).to_string();
	let drive_name_wide = encode_wide_string(&drive_name);
	let mut buffer_len = 512u32;

	loop {
		let mut buffer = vec![0u16; buffer_len as usize];
		let status = unsafe {
			WNetGetConnectionW(
				drive_name_wide.as_ptr(),
				buffer.as_mut_ptr(),
				&mut buffer_len,
			)
		};

		match status {
			NO_ERROR => return Some(decode_wide_buffer(&buffer)),
			ERROR_MORE_DATA => continue,
			_ => {
				debug!(
					"WNetGetConnectionW did not resolve {} as a network mapping (status {})",
					mount_point.display(),
					status
				);
				return None;
			}
		}
	}
}

/// Classify a volume using the platform-specific classifier.
fn classify_volume(
	mount_point: &PathBuf,
	file_system: &FileSystem,
	is_removable: bool,
	is_network_drive: bool,
	total_bytes_capacity: u64,
) -> VolumeType {
	let classifier = get_classifier();
	let detection_info = VolumeDetectionInfo {
		mount_point: mount_point.clone(),
		file_system: file_system.clone(),
		total_bytes_capacity,
		is_removable: Some(is_removable),
		is_network_drive: Some(is_network_drive),
		device_model: None,
	};

	classifier.classify(&detection_info)
}

fn network_fingerprint_source(remote_target: Option<&str>, mount_point: &Path) -> String {
	remote_target
		.map(normalize_network_identity)
		.unwrap_or_else(|| normalize_mount_key(mount_point))
}

fn network_identity_key(remote_target: Option<&str>, mount_point: &Path) -> String {
	network_fingerprint_source(remote_target, mount_point)
}

fn network_display_name(label: &str, remote_target: Option<&str>, mount_point: &Path) -> String {
	if !label.trim().is_empty() {
		return label.to_string();
	}

	if let Some(share_name) = remote_target.and_then(network_share_name) {
		return share_name;
	}

	format!(
		"Network Drive ({})",
		trim_trailing_separators(&mount_point.to_string_lossy())
	)
}

fn network_share_name(remote_target: &str) -> Option<String> {
	remote_target
		.trim_matches('\\')
		.rsplit('\\')
		.find(|segment| !segment.is_empty())
		.map(|segment| segment.to_string())
}

fn normalize_mount_key(path: &Path) -> String {
	normalize_network_identity(&path.to_string_lossy())
}

fn normalize_network_identity(value: &str) -> String {
	trim_trailing_separators(&value.replace('/', "\\")).to_lowercase()
}

fn trim_trailing_separators(value: &str) -> &str {
	value.trim_end_matches(['\\', '/'])
}

fn deduplicate_volumes(volumes: Vec<Volume>) -> Vec<Volume> {
	let mut deduped = Vec::new();
	let mut seen = HashSet::new();

	for volume in volumes {
		let key = if matches!(volume.mount_type, MountType::Network) {
			network_identity_key(volume.hardware_id.as_deref(), volume.mount_point.as_path())
		} else {
			normalize_mount_key(volume.mount_point.as_path())
		};

		if seen.insert(key) {
			deduped.push(volume);
		}
	}

	deduped
}

fn encode_wide_path(path: &Path) -> Vec<u16> {
	encode_wide_string(&path.to_string_lossy())
}

fn encode_wide_string(value: &str) -> Vec<u16> {
	value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn decode_wide_buffer(buffer: &[u16]) -> String {
	let end = buffer
		.iter()
		.position(|unit| *unit == 0)
		.unwrap_or(buffer.len());
	decode_wide_string(&buffer[..end])
}

fn decode_wide_string(buffer: &[u16]) -> String {
	String::from_utf16_lossy(buffer)
}

/// Determine mount type for Windows drives by checking if the volume
/// hosts the Windows installation (contains `\Windows\System32`).
fn determine_mount_type_windows(mount_point: &Path) -> MountType {
	if mount_point.join("Windows").join("System32").is_dir() {
		MountType::System
	} else {
		MountType::External
	}
}

/// Check if volume should be included based on config.
pub fn should_include_volume(volume: &Volume, config: &VolumeDetectionConfig) -> bool {
	if !config.include_system && matches!(volume.mount_type, MountType::System) {
		return false;
	}

	if !config.include_virtual && volume.total_bytes_capacity() == 0 {
		return false;
	}

	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn network_fingerprint_source_prefers_unc_target() {
		let first = network_fingerprint_source(Some(r"\\server\share"), Path::new(r"Z:\"));
		let second = network_fingerprint_source(Some(r"\\SERVER\SHARE\\"), Path::new(r"Y:\"));

		assert_eq!(first, second);
	}

	#[test]
	fn network_fingerprint_source_falls_back_to_mount_point() {
		let source = network_fingerprint_source(None, Path::new(r"Z:\"));
		assert_eq!(source, "z:");
	}

	#[test]
	fn network_display_name_falls_back_to_share_name() {
		let name = network_display_name("", Some(r"\\server\archive"), Path::new(r"Z:\"));
		assert_eq!(name, "archive");
	}

	#[test]
	fn windows_network_volume_contains_nested_paths() {
		let mut volume = Volume::new(
			Uuid::new_v4(),
			VolumeFingerprint::from_network_volume(r"\\server\share", r"\\server\share"),
			"share".to_string(),
			PathBuf::from(r"X:\"),
		);
		volume.mount_type = MountType::Network;
		volume.volume_type = VolumeType::Network;
		volume.file_system = FileSystem::NTFS;

		assert!(volume.contains_path(&PathBuf::from(r"X:\nested\file.txt")));
	}
}
