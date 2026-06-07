//! WASM Plugin Manager
//!
//! Manages the lifecycle of WASM extensions: loading, unloading, hot-reload.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use thiserror::Error;
use tokio::sync::RwLock;
use wasmer::{imports, Function, FunctionEnv, Instance, Memory, Module, Store};

use crate::{context::CoreContext, infra::api::ApiDispatcher};

use super::host_functions::{self, host_spacedrive_call, host_spacedrive_log, PluginEnv};
use super::job_registry::ExtensionJobRegistry;
use super::permissions::ExtensionPermissions;
use super::types::{ExtensionManifest, LoadedPlugin};

#[derive(Error, Debug)]
pub enum PluginError {
	#[error("Plugin not found: {0}")]
	NotFound(String),

	#[error("Failed to load manifest: {0}")]
	ManifestLoadFailed(String),

	#[error("Failed to compile WASM module: {0}")]
	CompilationFailed(String),

	#[error("Failed to instantiate WASM module: {0}")]
	InstantiationFailed(String),

	#[error("Plugin already loaded: {0}")]
	AlreadyLoaded(String),

	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
}

/// Manages WASM plugin lifecycle
pub struct PluginManager {
	store: Store,
	plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
	plugin_dir: PathBuf,
	core_context: Arc<CoreContext>,
	api_dispatcher: Arc<ApiDispatcher>,
	job_registry: Arc<ExtensionJobRegistry>,
}

impl PluginManager {
	/// Create new plugin manager
	pub fn new(
		plugin_dir: PathBuf,
		core_context: Arc<CoreContext>,
		api_dispatcher: Arc<ApiDispatcher>,
	) -> Self {
		let store = Store::default();

		Self {
			store,
			plugins: Arc::new(RwLock::new(HashMap::new())),
			plugin_dir,
			core_context,
			api_dispatcher,
			job_registry: Arc::new(ExtensionJobRegistry::new()),
		}
	}

	/// Get the job registry for extension jobs
	pub fn job_registry(&self) -> Arc<ExtensionJobRegistry> {
		self.job_registry.clone()
	}

	/// Read a loaded extension's manifest and compiled WASM bytes from disk.
	///
	/// Returns owned, `Send` data so callers can build a self-contained
	/// [`super::runner::WasmRunner`] on a blocking task without holding the
	/// manager's non-`Send` `wasmer::Store`. Returns `None` if the manifest or
	/// `.wasm` is missing or unreadable.
	pub fn load_extension_wasm(&self, extension_id: &str) -> Option<(ExtensionManifest, Vec<u8>)> {
		let dir = self.plugin_dir.join(extension_id);
		let manifest_str = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
		let manifest: ExtensionManifest = serde_json::from_str(&manifest_str).ok()?;
		let wasm_bytes = std::fs::read(dir.join(&manifest.wasm_file)).ok()?;
		Some((manifest, wasm_bytes))
	}

	/// Load a WASM plugin from directory
	///
	/// Expected structure:
	/// ```
	/// plugins/finance/
	///   ├── manifest.json
	///   └── finance.wasm
	/// ```
	pub async fn load_plugin(&mut self, plugin_id: &str) -> Result<(), PluginError> {
		// Check if already loaded
		if self.plugins.read().await.contains_key(plugin_id) {
			return Err(PluginError::AlreadyLoaded(plugin_id.to_string()));
		}

		tracing::info!("Loading plugin: {}", plugin_id);

		// 1. Load manifest
		let manifest_path = self.plugin_dir.join(plugin_id).join("manifest.json");
		let manifest: ExtensionManifest = {
			let manifest_str = std::fs::read_to_string(&manifest_path).map_err(|e| {
				PluginError::ManifestLoadFailed(format!("Failed to read manifest: {}", e))
			})?;

			serde_json::from_str(&manifest_str).map_err(|e| {
				PluginError::ManifestLoadFailed(format!("Failed to parse manifest: {}", e))
			})?
		};

		tracing::debug!(
			"Loaded manifest for plugin '{}' v{}",
			manifest.name,
			manifest.version
		);

		// 2. Read WASM file
		let wasm_path = self.plugin_dir.join(plugin_id).join(&manifest.wasm_file);
		let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| PluginError::Io(e))?;

		tracing::debug!("Read {} bytes of WASM", wasm_bytes.len());

		// 3. Compile WASM module
		let module = Module::new(&self.store, wasm_bytes).map_err(|e| {
			PluginError::CompilationFailed(format!("Failed to compile WASM: {}", e))
		})?;

		tracing::debug!("Compiled WASM module");

		// 4. Create plugin environment with temporary memory
		let permissions =
			ExtensionPermissions::from_manifest(manifest.id.clone(), &manifest.permissions);

		// Create temporary memory (will be replaced with instance's memory)
		let temp_memory = Memory::new(&mut self.store, wasmer::MemoryType::new(1, None, false))
			.map_err(|e| {
				PluginError::InstantiationFailed(format!("Failed to create temp memory: {}", e))
			})?;

		let plugin_env = PluginEnv {
			extension_id: manifest.id.clone(),
			core_context: self.core_context.clone(),
			api_dispatcher: self.api_dispatcher.clone(),
			permissions,
			memory: temp_memory,
			job_registry: self.job_registry.clone(),
		};

		let env = FunctionEnv::new(&mut self.store, plugin_env);

		// 5. Create imports (host functions exposed to WASM)
		let import_object = imports! {
			"spacedrive" => {
				// Core functions
				"spacedrive_call" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_spacedrive_call
				),
				"spacedrive_log" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_spacedrive_log
				),

				// Job-specific functions
				"job_report_progress" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_functions::host_job_report_progress
				),
				"job_checkpoint" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_functions::host_job_checkpoint
				),
				"job_check_interrupt" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_functions::host_job_check_interrupt
				),
				"job_add_warning" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_functions::host_job_add_warning
				),
				"job_increment_bytes" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_functions::host_job_increment_bytes
				),
				"job_increment_items" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_functions::host_job_increment_items
				),

				// Extension registration functions
				"register_job" => Function::new_typed_with_env(
					&mut self.store,
					&env,
					host_functions::host_register_job
				),
			}
		};

		// 6. Instantiate WASM module
		let instance = Instance::new(&mut self.store, &module, &import_object).map_err(|e| {
			PluginError::InstantiationFailed(format!("Failed to instantiate WASM: {}", e))
		})?;

		tracing::debug!("Instantiated WASM module");

		// 7. Get actual memory from instance and update environment
		let memory = instance.exports.get_memory("memory").map_err(|e| {
			PluginError::InstantiationFailed(format!("Plugin missing memory export: {}", e))
		})?;

		env.as_mut(&mut self.store).memory = memory.clone();

		// 8. Call plugin initialization function
		if let Ok(init_fn) = instance.exports.get_function("plugin_init") {
			match init_fn.call(&mut self.store, &[]) {
				Ok(_) => tracing::info!("Plugin {} initialized successfully", plugin_id),
				Err(e) => {
					tracing::error!("Plugin init failed: {}", e);
					return Err(PluginError::InstantiationFailed(format!(
						"plugin_init() failed: {}",
						e
					)));
				}
			}
		} else {
			tracing::warn!("Plugin {} has no plugin_init() function", plugin_id);
		}

		// 9. Store loaded plugin
		self.plugins.write().await.insert(
			plugin_id.to_string(),
			LoadedPlugin {
				id: plugin_id.to_string(),
				manifest,
				loaded_at: Utc::now(),
			},
		);

		tracing::info!("✓ Plugin {} loaded successfully", plugin_id);

		Ok(())
	}

	/// Unload a plugin
	pub async fn unload_plugin(&mut self, plugin_id: &str) -> Result<(), PluginError> {
		tracing::info!("Unloading plugin: {}", plugin_id);

		// Call plugin_cleanup() if exported, before removing the plugin entry.
		// We re-instantiate the WASM module briefly so the cleanup export can run.
		// This is necessary because the original Instance is not stored and the
		// Store is borrowed during the plugin's lifetime.
		let cleanup_result = {
			let manifest_path = self.plugin_dir.join(plugin_id).join("manifest.json");
			match std::fs::read_to_string(&manifest_path) {
				Ok(manifest_str) => {
					match serde_json::from_str::<ExtensionManifest>(&manifest_str) {
						Ok(manifest) => {
							let wasm_path =
								self.plugin_dir.join(plugin_id).join(&manifest.wasm_file);
							match std::fs::read(&wasm_path) {
								Ok(wasm_bytes) => Self::call_plugin_cleanup(
									&mut self.store,
									&manifest,
									&wasm_bytes,
									&self.core_context,
									&self.api_dispatcher,
									&self.job_registry,
								),
								Err(e) => {
									tracing::warn!(
										"Could not read WASM for cleanup of {}: {}",
										plugin_id,
										e
									);
									Ok(())
								}
							}
						}
						Err(e) => {
							tracing::warn!(
								"Could not parse manifest for cleanup of {}: {}",
								plugin_id,
								e
							);
							Ok(())
						}
					}
				}
				Err(e) => {
					tracing::warn!(
						"Could not read manifest for cleanup of {}: {}",
						plugin_id,
						e
					);
					Ok(())
				}
			}
		};

		if let Err(e) = cleanup_result {
			tracing::warn!("Plugin {} cleanup returned error: {}", plugin_id, e);
		}

		// Unregister extension jobs before removing the plugin entry
		self.job_registry.unregister_extension_jobs(plugin_id);

		// Remove plugin from the loaded map
		let _plugin = self
			.plugins
			.write()
			.await
			.remove(plugin_id)
			.ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

		tracing::info!("Plugin {} unloaded", plugin_id);

		Ok(())
	}

	/// Hot-reload a plugin (for development)
	pub async fn reload_plugin(&mut self, plugin_id: &str) -> Result<(), PluginError> {
		tracing::info!("Reloading plugin: {}", plugin_id);

		self.unload_plugin(plugin_id).await?;
		self.load_plugin(plugin_id).await?;

		tracing::info!("✓ Plugin {} reloaded", plugin_id);

		Ok(())
	}

	/// List all loaded plugins
	pub async fn list_plugins(&self) -> Vec<String> {
		self.plugins.read().await.keys().cloned().collect()
	}

	/// Get plugin manifest
	pub async fn get_manifest(&self, plugin_id: &str) -> Option<ExtensionManifest> {
		self.plugins
			.read()
			.await
			.get(plugin_id)
			.map(|p| p.manifest.clone())
	}

	/// Re-instantiate a WASM module and call its `plugin_cleanup()` export if present.
	///
	/// This is called during `unload_plugin` so extensions can release global
	/// resources.  Because the original `Instance` is not kept alive, we
	/// recompile and re-instantiate a fresh module; the cleanup function will
	/// therefore **not** have access to per-invocation state, only to any
	/// globals the WASM module declared at the module level.
	fn call_plugin_cleanup(
		store: &mut Store,
		manifest: &ExtensionManifest,
		wasm_bytes: &[u8],
		core_context: &Arc<crate::context::CoreContext>,
		api_dispatcher: &Arc<crate::infra::api::ApiDispatcher>,
		job_registry: &Arc<super::job_registry::ExtensionJobRegistry>,
	) -> Result<(), PluginError> {
		let module = Module::new(store, wasm_bytes).map_err(|e| {
			PluginError::CompilationFailed(format!("Failed to compile WASM for cleanup: {}", e))
		})?;

		let permissions =
			ExtensionPermissions::from_manifest(manifest.id.clone(), &manifest.permissions);

		let temp_memory =
			Memory::new(store, wasmer::MemoryType::new(1, None, false)).map_err(|e| {
				PluginError::InstantiationFailed(format!(
					"Failed to create temp memory for cleanup: {}",
					e
				))
			})?;

		let plugin_env = PluginEnv {
			extension_id: manifest.id.clone(),
			core_context: core_context.clone(),
			api_dispatcher: api_dispatcher.clone(),
			permissions,
			memory: temp_memory,
			job_registry: job_registry.clone(),
		};

		let env = FunctionEnv::new(store, plugin_env);

		// Build a minimal import object -- only the imports the cleanup function
		// is likely to need.
		let import_object = imports! {
			"spacedrive" => {
				"spacedrive_call" => Function::new_typed_with_env(
					store, &env, host_spacedrive_call,
				),
				"spacedrive_log" => Function::new_typed_with_env(
					store, &env, host_spacedrive_log,
				),
				"job_report_progress" => Function::new_typed_with_env(
					store, &env, host_functions::host_job_report_progress,
				),
				"job_checkpoint" => Function::new_typed_with_env(
					store, &env, host_functions::host_job_checkpoint,
				),
				"job_check_interrupt" => Function::new_typed_with_env(
					store, &env, host_functions::host_job_check_interrupt,
				),
				"job_add_warning" => Function::new_typed_with_env(
					store, &env, host_functions::host_job_add_warning,
				),
				"job_increment_bytes" => Function::new_typed_with_env(
					store, &env, host_functions::host_job_increment_bytes,
				),
				"job_increment_items" => Function::new_typed_with_env(
					store, &env, host_functions::host_job_increment_items,
				),
				"register_job" => Function::new_typed_with_env(
					store, &env, host_functions::host_register_job,
				),
			}
		};

		let instance = Instance::new(store, &module, &import_object).map_err(|e| {
			PluginError::InstantiationFailed(format!(
				"Failed to instantiate WASM for cleanup: {}",
				e
			))
		})?;

		// Replace the temporary memory with the instance's real memory
		if let Ok(inst_memory) = instance.exports.get_memory("memory") {
			env.as_mut(store).memory = inst_memory.clone();
		}

		// Call plugin_cleanup() if the module exports it
		match instance.exports.get_function("plugin_cleanup") {
			Ok(cleanup_fn) => match cleanup_fn.call(store, &[]) {
				Ok(_) => {
					tracing::info!("Plugin '{}' cleanup completed successfully", manifest.id);
					Ok(())
				}
				Err(e) => Err(PluginError::InstantiationFailed(format!(
					"plugin_cleanup() failed: {}",
					e
				))),
			},
			Err(_) => {
				tracing::debug!("Plugin '{}' has no plugin_cleanup() export", manifest.id);
				Ok(())
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	/// Helper to create a minimal PluginManager pointing at a temporary directory.
	fn temp_manager() -> (PluginManager, tempfile::TempDir) {
		let tmp = tempfile::tempdir().expect("Failed to create temp dir");
		let context = Arc::new(crate::context::CoreContext::new(
			Arc::new(crate::infra::event::EventBus::new()),
			Arc::new(crate::device::DeviceManager::new(tmp.path().join("device"))),
			None,
			Arc::new(crate::volume::VolumeManager::new()),
			Arc::new(crate::crypto::key_manager::KeyManager::new(
				tmp.path().join("keys"),
			)),
			tmp.path().to_path_buf(),
		));
		let api = Arc::new(crate::infra::api::ApiDispatcher::new(context.clone()));
		let manager = PluginManager::new(tmp.path().join("plugins"), context, api);
		(manager, tmp)
	}

	#[tokio::test]
	async fn create_plugin_manager() {
		let (manager, _tmp) = temp_manager();
		let plugins = manager.list_plugins().await;
		assert!(plugins.is_empty(), "New manager should have no plugins");
	}

	#[tokio::test]
	async fn unload_nonexistent_plugin_returns_not_found() {
		let (mut manager, _tmp) = temp_manager();
		let result = manager.unload_plugin("does-not-exist").await;
		assert!(
			matches!(result, Err(PluginError::NotFound(_))),
			"Expected NotFound, got {:?}",
			result
		);
	}

	#[tokio::test]
	async fn get_manifest_returns_none_for_unknown() {
		let (manager, _tmp) = temp_manager();
		assert!(
			manager.get_manifest("nonexistent").await.is_none(),
			"Should return None for unknown plugin"
		);
	}

	#[tokio::test]
	async fn load_plugin_fails_for_missing_directory() {
		let (mut manager, _tmp) = temp_manager();
		let result = manager.load_plugin("missing-plugin").await;
		assert!(
			result.is_err(),
			"Loading a plugin from a non-existent directory should fail"
		);
	}
}
