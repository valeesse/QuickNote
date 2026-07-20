use super::*;

fn database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().to_path_buf()).unwrap();
    (dir, db)
}

mod clipboard_cases;
mod note_cases;
