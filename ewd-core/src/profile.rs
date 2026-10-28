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
