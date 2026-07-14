pub mod instances;
pub mod plugins;
pub mod profiles;
pub mod property_inspector;
pub mod settings;

use crate::shared::{CATEGORIES, Category, DEVICES, DeviceInfo};

use std::collections::HashMap;

use font_loader::system_fonts;
use tauri::{Emitter, Manager, command};

#[derive(Debug, serde_with::SerializeDisplay, serde::Deserialize)]
pub struct Error {
	pub description: String,
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.description)
	}
}
impl std::error::Error for Error {}

impl Error {
	fn new(description: String) -> Self {
		log::error!("{}", description);
		Self { description }
	}
}

impl From<serde_json::Error> for Error {
	fn from(error: serde_json::Error) -> Self {
		Self::new(error.to_string())
	}
}

impl From<std::io::Error> for Error {
	fn from(error: std::io::Error) -> Self {
		Self::new(error.to_string())
	}
}

impl From<anyhow::Error> for Error {
	fn from(error: anyhow::Error) -> Self {
		Self::new(error.to_string())
	}
}

#[command]
pub async fn restart(app: tauri::AppHandle) {
	app.restart();
}

#[command]
pub async fn get_devices() -> dashmap::DashMap<String, DeviceInfo> {
	DEVICES.clone()
}

pub async fn update_devices() {
	let app = crate::APP_HANDLE.get().unwrap();
	let _ = app.get_webview_window("main").unwrap().emit("devices", DEVICES.clone());
}

#[command]
pub async fn get_port_base() -> u16 {
	*crate::plugins::PORT_BASE
}

#[command]
pub async fn get_categories() -> HashMap<String, Category> {
	CATEGORIES.read().await.clone()
}

#[command]
pub async fn get_localisations(locale: &str) -> Result<HashMap<String, serde_json::Value>, Error> {
	let mut localisations: HashMap<String, serde_json::Value> = HashMap::new();

	let mut entries = match tokio::fs::read_dir(&crate::shared::config_dir().join("plugins")).await {
		Ok(entries) => entries,
		Err(error) => return Err(anyhow::Error::from(error).into()),
	};

	while let Ok(Some(entry)) = entries.next_entry().await {
		let path = match entry.metadata().await.unwrap().is_symlink() {
			true => tokio::fs::read_link(entry.path()).await.unwrap(),
			false => entry.path(),
		};
		let metadata = tokio::fs::metadata(&path).await.unwrap();
		if metadata.is_dir() {
			let Ok(locale) = tokio::fs::read(path.join(format!("{locale}.json"))).await else { continue };
			let Ok(locale): Result<serde_json::Value, _> = serde_json::from_slice(&locale) else {
				continue;
			};
			localisations.insert(path.file_name().unwrap().to_str().unwrap().to_owned(), locale);
		}
	}

	Ok(localisations)
}

#[command]
pub async fn get_applications() -> Vec<String> {
	let mut apps = crate::application_watcher::APPLICATIONS.read().await.clone();
	let application_profiles = crate::application_watcher::APPLICATION_PROFILES.read().await;
	for app in application_profiles.value.keys() {
		if app != "opendeck_default" && !apps.contains(app) {
			apps.push(app.clone());
		}
	}
	apps
}

#[command]
pub async fn add_application(app_name: String) {
	let mut applications = crate::application_watcher::APPLICATIONS.write().await;
	if !applications.contains(&app_name) {
		applications.push(app_name);
	}
}

#[command]
pub async fn get_application_profiles() -> crate::application_watcher::ApplicationProfiles {
	crate::application_watcher::APPLICATION_PROFILES.read().await.value.clone()
}

#[command]
pub async fn set_application_profiles(value: crate::application_watcher::ApplicationProfiles) -> Result<(), Error> {
	let mut store = crate::application_watcher::APPLICATION_PROFILES.write().await;
	store.value = value;
	Ok(store.save()?)
}

#[command]
pub fn get_fonts() -> Vec<String> {
	system_fonts::query_all()
}

#[derive(serde::Serialize)]
pub struct WindowInfo {
	pub title: String,
	pub class: String,
}

enum DesktopEnvironment {
	Hyprland,
	Kde,
	X11,
	Unknown,
}

impl DesktopEnvironment {
	fn detect() -> Self {
		if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
			return Self::Hyprland;
		}
		let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
		if desktop.contains("kde") {
			return Self::Kde;
		}
		let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default().to_lowercase();
		if session_type == "x11" || std::env::var("DISPLAY").is_ok() {
			return Self::X11;
		}
		Self::Unknown
	}
}

fn get_windows_hyprland() -> Vec<WindowInfo> {
	let mut windows = Vec::new();
	if let Ok(output) = std::process::Command::new("hyprctl").args(["clients", "-j"]).output() {
		if let Ok(stdout) = String::from_utf8(output.stdout) {
			if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
				if let Some(arr) = json.as_array() {
					for client in arr {
						let title = client.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_owned();
						let class = client.get("class").and_then(|v| v.as_str()).unwrap_or_default().to_owned();
						if !class.is_empty() {
							windows.push(WindowInfo { title, class });
						}
					}
				}
			}
		}
	}
	windows
}

fn get_windows_kde() -> Vec<WindowInfo> {
	let mut windows = Vec::new();
	if let Ok(output) = std::process::Command::new("kdotool").arg("search").arg("").output() {
		if let Ok(stdout) = String::from_utf8(output.stdout) {
			for id in stdout.lines() {
				let id = id.trim();
				if id.is_empty() {
					continue;
				}
				let class_output = std::process::Command::new("kdotool").args(["getwindowclassname", id]).output();
				let title_output = std::process::Command::new("kdotool").args(["getwindowname", id]).output();
				let class = class_output.ok().and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_owned()).unwrap_or_default();
				let title = title_output.ok().and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_owned()).unwrap_or_default();
				if !class.is_empty() && class != "plasmashell" {
					windows.push(WindowInfo { title, class });
				}
			}
		}
	}
	windows
}

fn get_windows_x11() -> Vec<WindowInfo> {
	let mut windows = Vec::new();
	if let Ok(output) = std::process::Command::new("wmctrl").args(["-l", "-x"]).output() {
		if let Ok(stdout) = String::from_utf8(output.stdout) {
			for line in stdout.lines() {
				let parts: Vec<&str> = line.split_whitespace().collect();
				if parts.len() >= 4 {
					let class_part = parts[2];
					let class = class_part.split('.').last().unwrap_or(class_part).to_owned();
					let title = parts[3..].join(" ");
					if !class.is_empty() {
						windows.push(WindowInfo { title, class });
					}
				}
			}
		}
	} else if let Ok(output) = std::process::Command::new("xdotool").args(["search", "--onlyvisible", ""]).output() {
		if let Ok(stdout) = String::from_utf8(output.stdout) {
			for id in stdout.lines() {
				let id = id.trim();
				if id.is_empty() {
					continue;
				}
				let class_output = std::process::Command::new("xdotool").args(["getwindowclassname", id]).output();
				let title_output = std::process::Command::new("xdotool").args(["getwindowname", id]).output();
				let class = class_output.ok().and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_owned()).unwrap_or_default();
				let title = title_output.ok().and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_owned()).unwrap_or_default();
				if !class.is_empty() {
					windows.push(WindowInfo { title, class });
				}
			}
		}
	}
	windows
}

#[command]
pub async fn get_open_windows() -> Vec<WindowInfo> {
	match DesktopEnvironment::detect() {
		DesktopEnvironment::Hyprland => get_windows_hyprland(),
		DesktopEnvironment::Kde => get_windows_kde(),
		DesktopEnvironment::X11 => get_windows_x11(),
		DesktopEnvironment::Unknown => Vec::new(),
	}
}
