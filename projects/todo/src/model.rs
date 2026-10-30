//! Todo-owned task operations. No Engine, UI, terminal, or transport dependency.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

pub const MAX_TASKS: usize = 10_000;
pub const MAX_TITLE_CHARS: usize = 200;
const STORAGE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TaskId(u64);

impl TaskId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    id: TaskId,
    title: String,
    completed: bool,
}

impl Task {
    pub const fn id(&self) -> TaskId {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub const fn completed(&self) -> bool {
        self.completed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TodoCommand {
    Add { title: String },
    SetCompleted { id: TaskId, completed: bool },
    Remove { id: TaskId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TodoOutcome {
    Added(TaskId),
    Updated(TaskId),
    Unchanged(TaskId),
    Removed(TaskId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TodoError {
    InvalidTitle,
    TooManyTasks,
    IdExhausted,
    UnknownTask(TaskId),
    InvalidStoredState(&'static str),
}

impl fmt::Display for TodoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTitle => write!(
                f,
                "task title must contain 1–{MAX_TITLE_CHARS} printable characters"
            ),
            Self::TooManyTasks => write!(f, "task limit of {MAX_TASKS} reached"),
            Self::IdExhausted => f.write_str("task identity space exhausted"),
            Self::UnknownTask(id) => write!(f, "task {} does not exist", id.get()),
            Self::InvalidStoredState(reason) => write!(f, "invalid stored task state: {reason}"),
        }
    }
}

impl std::error::Error for TodoError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TodoList {
    version: u32,
    next_id: u64,
    tasks: Vec<Task>,
}

impl Default for TodoList {
    fn default() -> Self {
        Self {
            version: STORAGE_VERSION,
            next_id: 1,
            tasks: Vec::new(),
        }
    }
}

impl TodoList {
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// One canonical semantic mutation boundary for UI and programmatic callers.
    pub fn execute(&mut self, command: TodoCommand) -> Result<TodoOutcome, TodoError> {
        match command {
            TodoCommand::Add { title } => {
                let title = normalize_title(&title)?;
                if self.tasks.len() >= MAX_TASKS {
                    return Err(TodoError::TooManyTasks);
                }
                let next = self.next_id.checked_add(1).ok_or(TodoError::IdExhausted)?;
                let id = TaskId(self.next_id);
                self.tasks.push(Task {
                    id,
                    title: title.to_owned(),
                    completed: false,
                });
                self.next_id = next;
                Ok(TodoOutcome::Added(id))
            }
            TodoCommand::SetCompleted { id, completed } => {
                let task = self
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == id)
                    .ok_or(TodoError::UnknownTask(id))?;
                if task.completed == completed {
                    return Ok(TodoOutcome::Unchanged(id));
                }
                task.completed = completed;
                Ok(TodoOutcome::Updated(id))
            }
            TodoCommand::Remove { id } => {
                let position = self
                    .tasks
                    .iter()
                    .position(|task| task.id == id)
                    .ok_or(TodoError::UnknownTask(id))?;
                self.tasks.remove(position);
                Ok(TodoOutcome::Removed(id))
            }
        }
    }

    /// Run after deserialization, before exposing untrusted persisted state.
    pub fn validate(&self) -> Result<(), TodoError> {
        if self.version != STORAGE_VERSION {
            return Err(TodoError::InvalidStoredState("unsupported storage version"));
        }
        if self.next_id == 0 {
            return Err(TodoError::InvalidStoredState(
                "next task ID must be positive",
            ));
        }
        if self.tasks.len() > MAX_TASKS {
            return Err(TodoError::InvalidStoredState("too many tasks"));
        }
        let mut ids = HashSet::with_capacity(self.tasks.len());
        for task in &self.tasks {
            if task.id.get() == 0 || task.id.get() >= self.next_id || !ids.insert(task.id) {
                return Err(TodoError::InvalidStoredState(
                    "invalid or duplicate task ID",
                ));
            }
            if normalize_title(&task.title).is_err() || task.title.trim() != task.title {
                return Err(TodoError::InvalidStoredState("invalid task title"));
            }
        }
        Ok(())
    }
}

fn normalize_title(value: &str) -> Result<&str, TodoError> {
    let title = value.trim();
    if title.is_empty()
        || title.chars().count() > MAX_TITLE_CHARS
        || title.chars().any(char::is_control)
    {
        return Err(TodoError::InvalidTitle);
    }
    Ok(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_list_complete_reopen_and_remove_share_one_owner() {
        let mut tasks = TodoList::default();
        let id = match tasks
            .execute(TodoCommand::Add {
                title: "  Read book  ".into(),
            })
            .unwrap()
        {
            TodoOutcome::Added(id) => id,
            other => panic!("unexpected outcome: {other:?}"),
        };
        assert_eq!(tasks.tasks()[0].title(), "Read book");
        assert_eq!(
            tasks
                .execute(TodoCommand::SetCompleted {
                    id,
                    completed: true
                })
                .unwrap(),
            TodoOutcome::Updated(id)
        );
        assert!(tasks.tasks()[0].completed());
        assert_eq!(
            tasks
                .execute(TodoCommand::SetCompleted {
                    id,
                    completed: true
                })
                .unwrap(),
            TodoOutcome::Unchanged(id)
        );
        assert_eq!(
            tasks
                .execute(TodoCommand::SetCompleted {
                    id,
                    completed: false
                })
                .unwrap(),
            TodoOutcome::Updated(id)
        );
        assert!(!tasks.tasks()[0].completed());
        assert_eq!(
            tasks.execute(TodoCommand::Remove { id }).unwrap(),
            TodoOutcome::Removed(id)
        );
        assert!(tasks.tasks().is_empty());
        let next = tasks
            .execute(TodoCommand::Add {
                title: "Next".into(),
            })
            .unwrap();
        assert_eq!(next, TodoOutcome::Added(TaskId(2)));
    }

    #[test]
    fn rejected_operations_never_change_state() {
        let mut tasks = TodoList::default();
        let before = tasks.clone();
        assert_eq!(
            tasks.execute(TodoCommand::Add {
                title: " \n ".into()
            }),
            Err(TodoError::InvalidTitle)
        );
        assert_eq!(
            tasks.execute(TodoCommand::Add {
                title: "x".repeat(MAX_TITLE_CHARS + 1)
            }),
            Err(TodoError::InvalidTitle)
        );
        assert_eq!(
            tasks.execute(TodoCommand::SetCompleted {
                id: TaskId(9),
                completed: true
            }),
            Err(TodoError::UnknownTask(TaskId(9)))
        );
        assert_eq!(
            tasks.execute(TodoCommand::Remove { id: TaskId(9) }),
            Err(TodoError::UnknownTask(TaskId(9)))
        );
        assert_eq!(tasks, before);
    }

    #[test]
    fn persisted_ids_and_unknown_fields_are_validated() {
        let valid = TodoList::default();
        let json = serde_json::to_vec(&valid).unwrap();
        assert_eq!(serde_json::from_slice::<TodoList>(&json).unwrap(), valid);
        let invalid: TodoList = serde_json::from_str(r#"{"version":1,"next_id":2,"tasks":[{"id":1,"title":"A","completed":false},{"id":1,"title":"B","completed":false}]}"#).unwrap();
        assert!(invalid.validate().is_err());
        let unknown = r#"{"version":1,"next_id":1,"tasks":[],"surprise":true}"#;
        assert!(serde_json::from_str::<TodoList>(unknown).is_err());
    }
}
