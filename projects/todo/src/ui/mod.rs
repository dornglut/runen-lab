mod host;

#[cfg(test)]
mod layout_tests;

use crate::model::{MAX_TASKS, TaskId, TodoCommand};
use crate::service::TodoService;
use runenui_core::{
    Color, EditIntent, EditResolution, Effects, Element, IntoUpdateOutput, LayoutDimension,
    LayoutStyle, LogicalLength, NoHostProtocol, ProducerBudget, SemanticEditableMode, TextAffinity,
    TextDocumentId, TextDocumentRevision, TextDocumentSnapshot, TextPosition, TextSelection, UiApp,
    UpdateOutput, View, WorkResidencyBudget, WorkStartBudget, button, column, row, text, text_field,
};

pub use host::install;

const DRAFT_DOCUMENT_ID: TextDocumentId = TextDocumentId::new(1);

// Root, title, input row, editable, Add, and optional empty/error text;
// each task adds one row with Complete/Reopen, title and Remove.
const BASE_UI_NODES: usize = 6;
const TASK_UI_NODES: usize = 4;
const MAX_UI_NODES: usize = BASE_UI_NODES + TASK_UI_NODES * MAX_TASKS;

pub struct TodoUi;

pub struct TodoUiState {
    service: TodoService,
    draft: String,
    draft_revision: u64,
    draft_selection: usize,
    error: Option<String>,
}

impl TodoUiState {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self::new(TodoService::load()?))
    }

    fn new(service: TodoService) -> Self {
        Self {
            service,
            draft: String::new(),
            draft_revision: 0,
            draft_selection: 0,
            error: None,
        }
    }

    fn draft_snapshot(&self) -> TextDocumentSnapshot {
        TextDocumentSnapshot::new(
            DRAFT_DOCUMENT_ID,
            TextDocumentRevision::new(self.draft_revision),
        )
    }

    fn execute(&mut self, command: TodoCommand) -> bool {
        match self.service.execute(command) {
            Ok(_) => {
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }
}

pub enum TodoAction {
    EditDraft(Box<EditIntent>),
    AddTask,
    SetCompleted { id: TaskId, completed: bool },
    RemoveTask(TaskId),
}

impl UiApp for TodoUi {
    fn initial_producer_budget(state: &Self::State) -> ProducerBudget {
        // Initial mounting is state-dependent; bound the actual persisted task tree.
        let nodes = BASE_UI_NODES + TASK_UI_NODES * state.service.tasks().len();
        ProducerBudget::new(nodes + 1, 0).with_reconciliation(nodes, 0, nodes)
    }

    fn action_producer_budget(
        _state: &Self::State,
        _action: &Self::Action,
    ) -> ProducerBudget {
        // Queued actions may change the task count before this action executes.
        // The model's MAX_TASKS is the closed bound for every reachable tree.
        ProducerBudget::new(28, 1)
            .with_reconciliation(MAX_UI_NODES, MAX_UI_NODES, MAX_UI_NODES)
    }

    fn interaction_producer_budget(_state: &Self::State) -> ProducerBudget {
        // Retain the accepted U9-C text-field/IME service frontier, while
        // reconciling the Todo-owned maximum number of authored nodes.
        ProducerBudget::new(28, 13)
            .with_reconciliation(MAX_UI_NODES, MAX_UI_NODES, MAX_UI_NODES)
            .with_work_starts(WorkStartBudget::new(0, 0, 0, 0, 2))
            .with_work_residency(WorkResidencyBudget::new(0, 0, 0, 0, 0, 2))
    }

    type State = TodoUiState;
    type Action = TodoAction;
    type HostProtocol = NoHostProtocol;

    fn root(state: &Self::State) -> impl View<Self::Action> {
        let snapshot = state.draft_snapshot();
        let selection = TextPosition::new(
            snapshot,
            &state.draft,
            state.draft_selection,
            if state.draft_selection == state.draft.len() && !state.draft.is_empty() {
                TextAffinity::Upstream
            } else {
                TextAffinity::Downstream
            },
        )
        .expect("Todo owns a valid UTF-8 edit selection");

        let mut content: Vec<Element<TodoAction>> = vec![
            text("Todo").key("todo.heading").into_element(),
            row([
                text_field(
                    snapshot,
                    state.draft.clone(),
                    TextSelection::collapsed(selection),
                    SemanticEditableMode::SingleLine,
                    |intent| TodoAction::EditDraft(Box::new(intent)),
                )
                .expect("Todo draft is always normalized single-line text")
                .id("todo.new-task")
                .key("todo.new-task")
                // Reserve a usable entry width even when the document is empty;
                // the visual placeholder does not supply its intrinsic width.
                .with_layout(
                    LayoutStyle::default()
                        .with_width(LayoutDimension::length(LogicalLength::from(240_u16))),
                )
                .placeholder("New task")
                .on_submit(|| TodoAction::AddTask)
                .into_element(),
                button("Add")
                    .id("todo.add")
                    .key("todo.add")
                    .on_activate(|| TodoAction::AddTask)
                    .into_element(),
            ])
            .key("todo.new-task-row")
            .into_element(),
        ];

        if state.service.tasks().is_empty() {
            content.push(text("No tasks yet.").key("todo.empty").into_element());
        } else {
            for task in state.service.tasks() {
                let id = task.id();
                let completed = task.completed();
                let label = if completed {
                    format!("Done: {}", task.title())
                } else {
                    task.title().to_owned()
                };
                content.push(
                    row([
                        button(if completed { "Reopen" } else { "Complete" })
                            .key(format!("task.{}.toggle", id.get()))
                            .on_activate(move || TodoAction::SetCompleted {
                                id,
                                completed: !completed,
                            })
                            .into_element(),
                        text(label)
                            .key(format!("task.{}.title", id.get()))
                            .into_element(),
                        button("Remove")
                            .key(format!("task.{}.remove", id.get()))
                            .on_activate(move || TodoAction::RemoveTask(id))
                            .into_element(),
                    ])
                    .key(format!("task.{}.row", id.get()))
                    .into_element(),
                );
            }
        }

        if let Some(error) = &state.error {
            content.push(
                text(format!("Error: {error}"))
                    .key("todo.error")
                    .into_element(),
            );
        }

        column(content)
            .key("todo.root")
            .foreground(Color::rgb(238, 240, 246))
    }

    fn update(
        state: &mut Self::State,
        action: Self::Action,
    ) -> impl IntoUpdateOutput<Self::Action, Self::HostProtocol> {
        match action {
            TodoAction::EditDraft(intent) => {
                let range = intent.replacement();
                let request = intent.request().clone();
                let replacement = range.start()..range.end();
                if state.draft.get(replacement.clone()).is_none() {
                    state.error = Some("Rejected a stale or invalid text edit".to_owned());
                    return UpdateOutput::edit(EditResolution::rejected(
                        request,
                        state.draft_snapshot(),
                    ));
                }
                state
                    .draft
                    .replace_range(replacement, intent.replacement_text());
                state.draft_selection = intent.proposed_selection().active();
                state.draft_revision += 1;
                state.error = None;
                UpdateOutput::edit(EditResolution::accepted(request, state.draft_snapshot()))
            }
            TodoAction::AddTask => {
                let created = state.execute(TodoCommand::Add {
                    title: state.draft.clone(),
                });
                if created {
                    state.draft.clear();
                    state.draft_selection = 0;
                    state.draft_revision += 1;
                }
                UpdateOutput::effects(Effects::none())
            }
            TodoAction::SetCompleted { id, completed } => {
                state.execute(TodoCommand::SetCompleted { id, completed });
                UpdateOutput::effects(Effects::none())
            }
            TodoAction::RemoveTask(id) => {
                state.execute(TodoCommand::Remove { id });
                UpdateOutput::effects(Effects::none())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::TodoStorage;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn ui_and_programmatic_calls_use_same_task_owner_and_persist() {
        let path = std::env::temp_dir().join(format!(
            "runen-lab-todo-ui-{}-{}.json",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
        ));
        let storage = TodoStorage::at(path.clone());
        let mut state = TodoUiState::new(TodoService::at(storage.clone()).unwrap());

        state.draft = "Created from UI".into();
        state.draft_selection = state.draft.len();
        let _ = TodoUi::update(&mut state, TodoAction::AddTask);
        assert_eq!(state.service.tasks().len(), 1);
        assert_eq!(state.service.tasks()[0].title(), "Created from UI");

        let id = state.service.tasks()[0].id();
        let _ = TodoUi::update(
            &mut state,
            TodoAction::SetCompleted {
                id,
                completed: true,
            },
        );
        assert!(storage.load().unwrap().tasks()[0].completed());

        let _ = TodoUi::update(
            &mut state,
            TodoAction::SetCompleted {
                id,
                completed: false,
            },
        );
        assert!(!storage.load().unwrap().tasks()[0].completed());

        let _ = TodoUi::update(&mut state, TodoAction::RemoveTask(id));
        assert!(state.service.tasks().is_empty());
        assert!(storage.load().unwrap().tasks().is_empty());

        // A programmatic caller invokes the same live application service.
        state
            .service
            .execute(TodoCommand::Add {
                title: "Programmatic".into(),
            })
            .unwrap();
        assert_eq!(state.service.tasks()[0].title(), "Programmatic");
        assert_eq!(storage.load().unwrap().tasks()[0].title(), "Programmatic");

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_ui_add_keeps_draft_and_does_not_persist() {
        let path = std::env::temp_dir().join(format!(
            "runen-lab-todo-ui-{}-{}.json",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
        ));
        let storage = TodoStorage::at(path.clone());
        let mut state = TodoUiState::new(TodoService::at(storage).unwrap());
        state.draft = " ".into();
        state.draft_selection = state.draft.len();

        let _ = TodoUi::update(&mut state, TodoAction::AddTask);
        assert_eq!(state.draft, " ");
        assert!(state.service.tasks().is_empty());
        assert!(state.error.is_some());
        assert!(!path.exists());
    }
}
