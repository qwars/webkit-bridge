use std::collections::HashMap;
use std::sync::RwLock;
use webkit6::WebsiteDataManager;

pub struct ProfileManager {
	managers: RwLock<HashMap<String, WebsiteDataManager>>,
}

impl ProfileManager {
	pub fn new() -> Self {
		Self {
			managers: RwLock::new(HashMap::new()),
		}
	}

	pub fn get_or_create(&self, name: &str) -> WebsiteDataManager {
		let mut managers = self.managers.write().unwrap();

		if let Some(manager) = managers.get(name) {
			return manager.clone();
		}

		// Создаем директорию для данных профиля: ~/.cache/ewd/profiles/<name>
		let cache_dir = glib::user_cache_dir().join("ewd").join("profiles").join(name);
		let _ = std::fs::create_dir_all(&cache_dir);

		let data_manager = webkit6::WebsiteDataManager::builder()
			.base_data_directory(cache_dir.to_str().unwrap_or("/tmp/ewd"))
			.build();

		managers.insert(name.to_string(), data_manager.clone());
		data_manager
	}
}

impl Default for ProfileManager {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use uuid::Uuid;

	fn unique_profile_name(prefix: &str) -> String {
		format!("{}_{}", prefix, Uuid::new_v4())
	}

	#[test]
	fn test_profile_creation() {
		let manager = ProfileManager::new();
		let profile_name = unique_profile_name("test_creation");

		let data_manager = manager.get_or_create(&profile_name);

		// Проверяем что директория создана
		let base_dir = data_manager
			.base_data_directory()
			.expect("base_data_directory should return Some");
		let path = std::path::Path::new(base_dir.as_str());

		assert!(path.exists(), "Profile directory should exist: {:?}", path);
		assert!(path.is_dir(), "Profile path should be a directory");

		// Cleanup
		let _ = fs::remove_dir_all(path);
	}

	#[test]
	fn test_profile_caching() {
		let manager = ProfileManager::new();
		let profile_name = unique_profile_name("test_cache");

		let dm1 = manager.get_or_create(&profile_name);
		let dm2 = manager.get_or_create(&profile_name);

		// Проверяем что base_data_directory одинаковый (кэширование работает)
		let dir1 = dm1.base_data_directory().expect("should have base dir");
		let dir2 = dm2.base_data_directory().expect("should have base dir");
		assert_eq!(dir1.as_str(), dir2.as_str());

		// Cleanup
		let path = std::path::Path::new(dir1.as_str());
		let _ = fs::remove_dir_all(path);
	}

	#[test]
	fn test_different_profiles() {
		let manager = ProfileManager::new();
		let name1 = unique_profile_name("test_diff1");
		let name2 = unique_profile_name("test_diff2");

		let dm1 = manager.get_or_create(&name1);
		let dm2 = manager.get_or_create(&name2);

		let dir1 = dm1.base_data_directory().expect("should have base dir");
		let dir2 = dm2.base_data_directory().expect("should have base dir");

		assert_ne!(
			dir1.as_str(),
			dir2.as_str(),
			"Different profiles should have different directories"
		);

		// Cleanup
		let _ = fs::remove_dir_all(std::path::Path::new(dir1.as_str()));
		let _ = fs::remove_dir_all(std::path::Path::new(dir2.as_str()));
	}
}
