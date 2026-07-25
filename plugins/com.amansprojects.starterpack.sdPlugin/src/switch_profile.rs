//! Non-spec OpenDeck-specific protocols are used in this file.

use openaction::*;

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct SwitchProfileEvent {
	event: &'static str,
	device: String,
	profile: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct SwitchProfileSettings {
	device: Option<String>,
	profile: Option<String>,
	anticlockwise: Option<String>,
	clockwise: Option<String>,
}

pub struct SwitchProfileAction;
#[async_trait]
impl Action for SwitchProfileAction {
	const UUID: &'static str = "com.amansprojects.starterpack.switchprofile";
	type Settings = SwitchProfileSettings;

	async fn key_up(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
		send_arbitrary_json(SwitchProfileEvent {
			event: "switchProfile",
			device: settings
				.device
				.as_deref()
				.unwrap_or(&instance.device_id)
				.to_owned(),
			profile: settings.profile.as_deref().unwrap_or("Default").to_owned(),
		})
		.await
	}

	async fn dial_up(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
	) -> OpenActionResult<()> {
		self.key_up(instance, settings).await
	}

	async fn dial_rotate(
		&self,
		instance: &Instance,
		settings: &Self::Settings,
		ticks: i16,
		_pressed: bool,
	) -> OpenActionResult<()> {
		let profile = if ticks < 0 {
			&settings.anticlockwise
		} else {
			&settings.clockwise
		};
		send_arbitrary_json(SwitchProfileEvent {
			event: "switchProfile",
			device: settings
				.device
				.as_deref()
				.unwrap_or(&instance.device_id)
				.to_owned(),
			profile: profile.as_deref().unwrap_or("Default").to_owned(),
		})
		.await
	}

	async fn property_inspector_did_appear(
		&self,
		instance: &Instance,
		_settings: &Self::Settings,
	) -> OpenActionResult<()> {
		instance
			.send_to_property_inspector(serde_json::json!({
				"event": "updateDevices",
				"devices": get_connected_devices().await.keys().collect::<Vec<_>>(),
				"profiles": get_profiles_map(),
			}))
			.await
	}
}

pub(crate) async fn update_devices() -> OpenActionResult<()> {
	let message = serde_json::json!({
		"event": "updateDevices",
		"devices": get_connected_devices().await.keys().collect::<Vec<_>>(),
		"profiles": get_profiles_map(),
	});
	for instance in visible_instances(SwitchProfileAction::UUID).await {
		instance.send_to_property_inspector(message.clone()).await?;
	}
	Ok(())
}

/// Resolves the OpenDeck configuration directory path, handling Flatpak sandboxing if active.
fn get_config_dir() -> Option<std::path::PathBuf> {
	if std::path::Path::new("/.flatpak-info").exists() {
		if let Some(home) = dirs::home_dir() {
			let flatpak_config = home.join(".var/app/me.amankhanna.opendeck/config/opendeck");
			if flatpak_config.exists() {
				return Some(flatpak_config);
			}
		}
	}
	dirs::config_dir().map(|p| p.join("opendeck"))
}

/// Helper to extract profile ID from filename by stripping known extensions (.json, .json.bak, .json.temp).
fn extract_profile_id(filename: &str) -> Option<&str> {
	filename
		.strip_suffix(".json")
		.or_else(|| filename.strip_suffix(".json.bak"))
		.or_else(|| filename.strip_suffix(".json.temp"))
}

/// Scans a device directory for profile files and subdirectories.
fn scan_device_profiles(device_dir: &std::path::Path) -> Vec<String> {
	let mut profiles = Vec::new();
	let Ok(entries) = std::fs::read_dir(device_dir) else {
		return vec!["Default".to_owned()];
	};

	for entry in entries.flatten() {
		let path = entry.path();
		if path.is_file() {
			let name = entry.file_name().to_string_lossy().into_owned();
			if let Some(id) = extract_profile_id(&name) {
				if !profiles.contains(&id.to_owned()) {
					profiles.push(id.to_owned());
				}
			}
		} else if path.is_dir() {
			let dir_name = entry.file_name().to_string_lossy().into_owned();
			if let Ok(sub_entries) = std::fs::read_dir(&path) {
				for sub_entry in sub_entries.flatten() {
					if sub_entry.path().is_file() {
						let file_name = sub_entry.file_name().to_string_lossy().into_owned();
						if let Some(stem) = extract_profile_id(&file_name) {
							let id = format!("{dir_name}/{stem}");
							if !profiles.contains(&id) {
								profiles.push(id);
							}
						}
					}
				}
			}
		}
	}

	if profiles.is_empty() {
		profiles.push("Default".to_owned());
	}
	profiles.sort();
	profiles
}

/// Scans the profiles directory and returns a mapping of device IDs to their available profile names.
pub fn get_profiles_map() -> std::collections::HashMap<String, Vec<String>> {
	let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
	let Some(config_dir) = get_config_dir() else {
		return map;
	};
	let Ok(entries) = std::fs::read_dir(config_dir.join("profiles")) else {
		return map;
	};

	for entry in entries.flatten() {
		if entry.path().is_dir() {
			let device_id = entry.file_name().to_string_lossy().into_owned();
			map.insert(device_id, scan_device_profiles(&entry.path()));
		}
	}

	map
}
