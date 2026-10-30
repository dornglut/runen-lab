//! Project-local persistence. The Todo app has one writer; no cross-process editing is implied.

use crate::model::TodoList;
use anyhow::{Context, Result, bail};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct TodoStorage {
    path: PathBuf,
}

impl TodoStorage {
    pub fn from_environment() -> Result<Self> {
        if let Some(path) = std::env::var_os("RUNEN_LAB_TODO_DATA_FILE") {
            if path.is_empty() {
                bail!("RUNEN_LAB_TODO_DATA_FILE is empty");
            }
            return Ok(Self::at(PathBuf::from(path)));
        }
        Ok(Self::at(default_data_dir()?.join("tasks.json")))
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<TodoList> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(TodoList::default());
            }
            Err(error) => return Err(error).with_context(|| format!("open {}", self.path.display())),
        };
        // Bound allocations even if the file grows while reading.
        let mut data = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut data)
            .with_context(|| format!("read {}", self.path.display()))?;
        if data.len() as u64 > MAX_FILE_BYTES {
            bail!("task data exceeds the {} byte limit", MAX_FILE_BYTES);
        }
        let tasks: TodoList = serde_json::from_slice(&data)
            .with_context(|| format!("decode {}", self.path.display()))?;
        tasks.validate().context("validate saved tasks")?;
        Ok(tasks)
    }

    pub fn save(&self, tasks: &TodoList) -> Result<()> {
        tasks.validate().context("validate tasks before writing")?;
        let data = serde_json::to_vec_pretty(tasks).context("encode tasks")?;
        if data.len() as u64 > MAX_FILE_BYTES {
            bail!("task data exceeds the {} byte limit", MAX_FILE_BYTES);
        }
        let parent = self.path.parent().filter(|p| !p.as_os_str().is_empty())
            .context("task data path has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        // One writer per running Todo instance; process-ID suffix avoids reusing another
        // process's temporary file, but does not implement multi-process locking.
        let mut temp_name = self.path.as_os_str().to_os_string();
        temp_name.push(format!(".{}.tmp", std::process::id()));
        let temp = PathBuf::from(temp_name);
        let result = (|| -> Result<()> {
            let mut file = OpenOptions::new().write(true).create(true).truncate(true)
                .open(&temp).with_context(|| format!("create {}", temp.display()))?;
            file.write_all(&data).context("write task data")?;
            file.sync_all().context("sync task data")?;
            fs::rename(&temp, &self.path)
                .with_context(|| format!("replace {}", self.path.display()))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

#[cfg(windows)]
fn default_data_dir() -> Result<PathBuf> {
    let root = std::env::var_os("APPDATA").context("APPDATA is unavailable")?;
    Ok(PathBuf::from(root).join("RunenLab").join("Todo"))
}

#[cfg(target_os = "macos")]
fn default_data_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is unavailable")?;
    Ok(PathBuf::from(home).join("Library/Application Support/RunenLab/Todo"))
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn default_data_dir() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(root).join("runen-lab").join("todo"));
    }
    let home = std::env::var_os("HOME").context("HOME is unavailable")?;
    Ok(PathBuf::from(home).join(".local/share/runen-lab/todo"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TodoCommand, TodoOutcome};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "runen-lab-todo-{}-{}.json",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
        ))
    }

    #[test]
    fn restart_loads_completed_tasks_and_preserves_ids() {
        let path = test_path();
        let store = TodoStorage::at(path.clone());
        let mut tasks = store.load().unwrap();
        let id = match tasks.execute(TodoCommand::Add { title: "Persistent".into() }).unwrap() {
            TodoOutcome::Added(id) => id,
            other => panic!("unexpected outcome: {other:?}"),
        };
        tasks.execute(TodoCommand::SetCompleted { id, completed: true }).unwrap();
        store.save(&tasks).unwrap();
        let reloaded = store.load().unwrap();
        assert_eq!(reloaded, tasks);
        assert_eq!(reloaded.tasks()[0].id(), id);
        assert!(reloaded.tasks()[0].completed());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_data_fails_closed_instead_of_erasing_tasks() {
        let path = test_path();
        fs::write(&path, b"{bad json").unwrap();
        let store = TodoStorage::at(path.clone());
        assert!(store.load().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{bad json");
        fs::remove_file(path).unwrap();
    }
}
