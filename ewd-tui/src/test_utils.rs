use std::sync::{Mutex, OnceLock};

static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

pub fn env_mutex() -> &'static Mutex<()> {
	ENV_MUTEX.get_or_init(|| Mutex::new(()))
}
