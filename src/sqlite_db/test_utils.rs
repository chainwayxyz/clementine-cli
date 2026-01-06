use crate::sqlite_db::sqlite_client::SqliteDb;

/// Create an isolated in-memory SQLite database keyed by the test name.
pub async fn fresh_db_with_test_name() -> SqliteDb {
    let test_name = std::thread::current()
        .name()
        .expect("Failed to get current thread name for test database")
        .split(':')
        .next_back()
        .expect("Failed to get last segment of thread name")
        .to_string();

    SqliteDb::open_in_memory_with_schema(&test_name)
        .await
        .expect("Failed to open in-memory test DB")
}
