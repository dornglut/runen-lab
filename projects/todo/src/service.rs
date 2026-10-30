//! Todo application service shared by UI and future typed callers.
//! Only the Todo owner decides whether and when a command changes saved state.

use crate::model::{Task, TodoCommand, TodoError, TodoList, TodoOutcome};
use crate::storage::TodoStorage;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum TodoApplyError {
    Command(TodoError),
    Persistence(anyhow::Error),
}

impl fmt::Display for TodoApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => error.fmt(f),
            Self::Persistence(error) => write!(f, "could not save tasks: {error}"),
        }
    }
}

impl Error for TodoApplyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Persistence(error) => Some(error.as_ref()),
        }
    }
}

pub struct TodoService {
    tasks: TodoList,
    storage: TodoStorage,
}

impl TodoService {
    pub fn load() -> anyhow::Result<Self> {
        Self::at(TodoStorage::from_environment()?)
    }

    pub fn at(storage: TodoStorage) -> anyhow::Result<Self> {
        let tasks = storage.load()?;
        Ok(Self { tasks, storage })
    }

    pub fn tasks(&self) -> &[Task] {
        self.tasks.tasks()
    }

    /// Mutates the Todo owner, not a copy held by a particular interface.
    /// State is published only after a successful save. No-op outcomes do not write.
    pub fn execute(&mut self, command: TodoCommand) -> Result<TodoOutcome, TodoApplyError> {
        let mut candidate = self.tasks.clone();
        let outcome = candidate
            .execute(command)
            .map_err(TodoApplyError::Command)?;
        if !matches!(outcome, TodoOutcome::Unchanged(_)) {
            self.storage
                .save(&candidate)
                .map_err(TodoApplyError::Persistence)?;
            self.tasks = candidate;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn programmatic_caller_preserves_owner_and_durable_result() {
        let path = std::env::temp_dir().join(format!(
            "runen-lab-todo-service-{}-{}.json",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
        ));
        let storage = TodoStorage::at(path.clone());
        let mut service = TodoService::at(storage.clone()).unwrap();
        let id = match service
            .execute(TodoCommand::Add {
                title: "Programmatic".into(),
            })
            .unwrap()
        {
            TodoOutcome::Added(id) => id,
            other => panic!("unexpected result: {other:?}"),
        };
        assert_eq!(service.tasks().len(), 1);
        assert_eq!(storage.load().unwrap().tasks()[0].id(), id);

        let error = service.execute(TodoCommand::Add { title: " ".into() });
        assert!(matches!(
            error,
            Err(TodoApplyError::Command(TodoError::InvalidTitle))
        ));
        assert_eq!(service.tasks().len(), 1);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejected_persistence_does_not_publish_candidate_mutation() {
        let path = std::env::temp_dir().join(format!(
            "runen-lab-todo-service-{}-{}.json",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
        ));
        let mut service = TodoService::at(TodoStorage::at(path.clone())).unwrap();
        // A directory at the output path prevents replacing it with task data.
        std::fs::create_dir(&path).unwrap();
        let error = service.execute(TodoCommand::Add {
            title: "Must not appear".into(),
        });
        assert!(matches!(error, Err(TodoApplyError::Persistence(_))));
        assert!(service.tasks().is_empty());
        assert!(path.is_dir());
        std::fs::remove_dir(path).unwrap();
    }
}
