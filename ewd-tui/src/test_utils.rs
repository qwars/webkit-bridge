use std::sync::{Mutex, OnceLock};

/// Глобальный мьютекс для синхронизации тестов, изменяющих ENV
/// Используется для предотвращения race condition при параллельном выполнении тестов
static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

/// Получить глобальный мьютекс для ENV
///
/// # Example
/// ```
/// let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
/// // Теперь можно безопасно изменять ENV
/// ```
pub fn env_mutex() -> &'static Mutex<()> {
	ENV_MUTEX.get_or_init(|| Mutex::new(()))
}
