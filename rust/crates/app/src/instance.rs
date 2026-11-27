//! A live process holds this lock beside its game's config until it exits.
//! Leave the file in place so every contender locks the same file.

use std::fs::{File, TryLockError};
use std::io;
use std::path::Path;

pub fn acquire(config: &Path) -> io::Result<Option<File>> {
    let path = config.with_file_name("instance.lock");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::options().read(true).write(true).create(true).truncate(false).open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::config_path_in;
    use std::process::Command;
    use wraex_core::GameId;

    #[test]
    fn games_have_independent_process_locks() {
        const CHILD: &str = "WRAEX_TEST_INSTANCE_PROBE";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = Path::new(&root);
            assert_eq!(
                acquire(&config_path_in(GameId::Poe1, root)).unwrap().is_none(),
                std::env::var_os("WRAEX_TEST_INSTANCE_BUSY").is_some()
            );
            assert!(acquire(&config_path_in(GameId::Poe2, root)).unwrap().is_some());
            return;
        }
        let root = std::env::temp_dir().join(format!("wraex-instance-test-{}", std::process::id()));
        let owner = acquire(&config_path_in(GameId::Poe1, &root)).unwrap().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", "instance::tests::games_have_independent_process_locks"])
            .env(CHILD, &root)
            .env("WRAEX_TEST_INSTANCE_BUSY", "1");
        assert!(child.status().unwrap().success());
        drop(owner);
        assert!(child.env_remove("WRAEX_TEST_INSTANCE_BUSY").status().unwrap().success());
        std::fs::remove_dir_all(root).unwrap();
    }
}
