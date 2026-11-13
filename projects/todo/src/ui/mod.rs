mod host;

#[cfg(test)]
mod layout_tests;

use crate::model::{Task, TaskId, TodoCommand};
use crate::service::TodoService;
use runenui_core::{
    Color, EdgeInsets, EditIntent, EditResolution, Effects, Element, FlexItemStyle, FontFamily,
    FontWeight, GenericFontFamily, IntoUpdateOutput, LayoutBound, LayoutDimension, LayoutFactor,
    LayoutStyle, LogicalLength, NoHostProtocol, ProducerBudget, Radius, SemanticEditableMode,
    TextAffinity, TextDocumentId, TextDocumentRevision, TextDocumentSnapshot, TextPosition,
    TextSelection, Typography, UiApp, UpdateOutput, View, WorkResidencyBudget, WorkStartBudget,
    button, column, row, text, text_field,
};

pub use host::install;

const DRAFT_DOCUMENT_ID: TextDocumentId = TextDocumentId::new(1);

// Keep the mounted UI independent of the storage model's 10,000-task capacity.
// RunenUI does not yet expose a virtualized list; page the application-owned
// collection instead of reserving unbounded routed focus/output capacity.
const TASKS_PER_PAGE: usize = 8;
// Header, composer, section labels, root, empty/error state.
const BASE_UI_NODES: usize = 14;
const TASK_UI_NODES: usize = 4;
const PAGE_UI_NODES: usize = 4;
const MAX_UI_NODES: usize = BASE_UI_NODES + TASK_UI_NODES * TASKS_PER_PAGE + PAGE_UI_NODES;
// The maximum page exposes a text field, Add, two actions per task and two
// paging actions. Focus transfer can route each event-bearing widget four
// times; ordinary routing and framework defaults require additional headroom.
const MAX_INTERACTION_OUTPUTS: usize = 4 * (4 + 2 * TASKS_PER_PAGE) + 4;

fn ui_font(size: u16, bold: bool) -> Typography {
    let typography = Typography::new(
        FontFamily::generic(GenericFontFamily::SansSerif),
        LogicalLength::from(size),
    );
    if bold {
        typography.with_weight(FontWeight::BOLD)
    } else {
        typography
    }
}

pub struct TodoUi;

pub struct TodoUiState {
    service: TodoService,
    page_index: usize,
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
            page_index: 0,
            draft: String::new(),
            draft_revision: 0,
            draft_selection: 0,
            error: None,
        }
    }

    fn page_count(&self) -> usize {
        self.service.tasks().len().div_ceil(TASKS_PER_PAGE).max(1)
    }

    fn visible_tasks(&self) -> &[Task] {
        let tasks = self.service.tasks();
        let start = self
            .page_index
            .saturating_mul(TASKS_PER_PAGE)
            .min(tasks.len());
        let end = start.saturating_add(TASKS_PER_PAGE).min(tasks.len());
        &tasks[start..end]
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
    PreviousPage,
    NextPage,
    SetCompleted { id: TaskId, completed: bool },
    RemoveTask(TaskId),
}

impl UiApp for TodoUi {
    fn initial_producer_budget(state: &Self::State) -> ProducerBudget {
        // Only the current page mounts; all persisted tasks remain available
        // through the application-owned page state and unchanged TodoService.
        let pager = usize::from(state.page_count() > 1) * PAGE_UI_NODES;
        let nodes = BASE_UI_NODES + TASK_UI_NODES * state.visible_tasks().len() + pager;
        ProducerBudget::new(nodes + 1, 0).with_reconciliation(nodes, 0, nodes)
    }

    fn action_producer_budget(_state: &Self::State, _action: &Self::Action) -> ProducerBudget {
        // Every predecessor state still mounts at most one page. Reserve the
        // bounded lifecycle/reconciliation frontier across page transitions.
        ProducerBudget::new(3 * MAX_UI_NODES + 16, 1).with_reconciliation(
            MAX_UI_NODES,
            MAX_UI_NODES,
            MAX_UI_NODES,
        )
    }

    fn interaction_producer_budget(_state: &Self::State) -> ProducerBudget {
        // Focus may traverse all mounted actionable rows after earlier
        // application actions. Reserve a fixed upper bound for every page,
        // including the U9-C native text/IME framework-service frontier.
        ProducerBudget::new(3 * MAX_INTERACTION_OUTPUTS + 32, MAX_INTERACTION_OUTPUTS)
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

        let tasks = state.service.tasks();
        let completed = tasks.iter().filter(|task| task.completed()).count();
        let open = tasks.len() - completed;
        let mut content: Vec<Element<TodoAction>> = vec![
            column([
                text("RUNEN LAB  /  DESKTOP")
                    .foreground(Color::rgb(108, 133, 157))
                    .typography(ui_font(11, false))
                    .key("todo.eyebrow")
                    .into_element(),
                text("Your tasks")
                    .id("todo.heading")
                    .foreground(Color::rgb(242, 246, 251))
                    .typography(ui_font(30, true))
                    .key("todo.heading")
                    .into_element(),
                text(format!("{open} open  /  {completed} completed"))
                    .foreground(Color::rgb(158, 176, 192))
                    .typography(ui_font(13, false))
                    .key("todo.summary")
                    .into_element(),
            ])
            .gap(LogicalLength::from(3_u16))
            .key("todo.header")
            .into_element(),
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
                .with_layout(
                    LayoutStyle::default()
                        .with_width(LayoutDimension::length(LogicalLength::from(240_u16)))
                        .with_min_width(LayoutBound::length(LogicalLength::from(110_u16)))
                        .with_flex_item(FlexItemStyle::default().with_grow(LayoutFactor::ONE)),
                )
                .placeholder("What needs doing?")
                .background(Color::rgb(34, 46, 64))
                .foreground(Color::rgb(242, 246, 251))
                .padding(EdgeInsets::symmetric(
                    LogicalLength::from(11_u16),
                    LogicalLength::from(9_u16),
                ))
                .radius(Radius::all(LogicalLength::from(8_u16)))
                .into_element(),
                button("Add task")
                    .id("todo.add")
                    .key("todo.add")
                    .on_activate(|| TodoAction::AddTask)
                    .background(Color::rgb(107, 222, 188))
                    .foreground(Color::rgb(12, 33, 34))
                    .typography(ui_font(13, true))
                    .padding(EdgeInsets::symmetric(
                        LogicalLength::from(15_u16),
                        LogicalLength::from(11_u16),
                    ))
                    .radius(Radius::all(LogicalLength::from(8_u16)))
                    .into_element(),
            ])
            .gap(LogicalLength::from(10_u16))
            .background(Color::rgb(24, 33, 47))
            .padding(EdgeInsets::all(LogicalLength::from(10_u16)))
            .radius(Radius::all(LogicalLength::from(12_u16)))
            .key("todo.new-task-row")
            .into_element(),
            row([
                text("TASKS")
                    .foreground(Color::rgb(158, 176, 192))
                    .typography(ui_font(12, true))
                    .key("todo.section-title")
                    .into_element(),
                text(format!("{} total", tasks.len()))
                    .foreground(Color::rgb(108, 133, 157))
                    .typography(ui_font(12, false))
                    .key("todo.total")
                    .into_element(),
            ])
            .gap(LogicalLength::from(14_u16))
            .key("todo.section")
            .into_element(),
        ];

        if tasks.is_empty() {
            content.push(
                column([
                    text("Nothing here yet")
                        .foreground(Color::rgb(226, 236, 244))
                        .typography(ui_font(17, true))
                        .key("todo.empty-title")
                        .into_element(),
                    text("Add your first task above to get started.")
                        .foreground(Color::rgb(155, 174, 192))
                        .typography(ui_font(13, false))
                        .key("todo.empty-description")
                        .into_element(),
                ])
                .gap(LogicalLength::from(6_u16))
                .background(Color::rgb(24, 33, 47))
                .padding(EdgeInsets::all(LogicalLength::from(18_u16)))
                .radius(Radius::all(LogicalLength::from(12_u16)))
                .key("todo.empty")
                .into_element(),
            );
        } else {
            for task in state.visible_tasks() {
                let id = task.id();
                let completed = task.completed();
                content.push(
                    row([
                        button(if completed { "Reopen" } else { "Complete" })
                            .id(format!("task.{}.toggle", id.get()))
                            .key(format!("task.{}.toggle", id.get()))
                            .on_activate(move || TodoAction::SetCompleted {
                                id,
                                completed: !completed,
                            })
                            .foreground(if completed {
                                Color::rgb(160, 185, 197)
                            } else {
                                Color::rgb(12, 33, 34)
                            })
                            .background(if completed {
                                Color::rgb(45, 58, 72)
                            } else {
                                Color::rgb(107, 222, 188)
                            })
                            .typography(ui_font(12, true))
                            .padding(EdgeInsets::symmetric(
                                LogicalLength::from(9_u16),
                                LogicalLength::from(6_u16),
                            ))
                            .radius(Radius::all(LogicalLength::from(7_u16)))
                            .into_element(),
                        text(task.title())
                            .key(format!("task.{}.title", id.get()))
                            .foreground(if completed {
                                Color::rgb(120, 141, 158)
                            } else {
                                Color::rgb(233, 241, 248)
                            })
                            .typography(ui_font(14, false))
                            .with_layout(LayoutStyle::default().with_flex_item(
                                FlexItemStyle::default().with_grow(LayoutFactor::ONE),
                            ))
                            .into_element(),
                        button("Remove")
                            .id(format!("task.{}.remove", id.get()))
                            .key(format!("task.{}.remove", id.get()))
                            .on_activate(move || TodoAction::RemoveTask(id))
                            .foreground(Color::rgb(149, 165, 181))
                            .background(Color::rgb(37, 48, 63))
                            .typography(ui_font(12, false))
                            .padding(EdgeInsets::symmetric(
                                LogicalLength::from(8_u16),
                                LogicalLength::from(6_u16),
                            ))
                            .radius(Radius::all(LogicalLength::from(7_u16)))
                            .into_element(),
                    ])
                    .gap(LogicalLength::from(10_u16))
                    .background(if completed {
                        Color::rgb(22, 29, 40)
                    } else {
                        Color::rgb(28, 38, 53)
                    })
                    .padding(EdgeInsets::symmetric(
                        LogicalLength::from(10_u16),
                        LogicalLength::from(6_u16),
                    ))
                    .radius(Radius::all(LogicalLength::from(9_u16)))
                    .id(format!("task.{}.row", id.get()))
                    .key(format!("task.{}.row", id.get()))
                    .into_element(),
                );
            }
        }

        if state.page_count() > 1 {
            content.push(
                row([
                    button("Previous")
                        .id("todo.page.previous")
                        .key("todo.page.previous")
                        .enabled(state.page_index > 0)
                        .on_activate(|| TodoAction::PreviousPage)
                        .foreground(Color::rgb(207, 222, 234))
                        .background(Color::rgb(40, 53, 69))
                        .padding(EdgeInsets::symmetric(
                            LogicalLength::from(10_u16),
                            LogicalLength::from(6_u16),
                        ))
                        .radius(Radius::all(LogicalLength::from(7_u16)))
                        .into_element(),
                    text(format!(
                        "Page {} of {}",
                        state.page_index + 1,
                        state.page_count()
                    ))
                    .id("todo.page.label")
                    .key("todo.page.label")
                    .foreground(Color::rgb(150, 169, 186))
                    .typography(ui_font(12, false))
                    .into_element(),
                    button("Next")
                        .id("todo.page.next")
                        .key("todo.page.next")
                        .enabled(state.page_index + 1 < state.page_count())
                        .on_activate(|| TodoAction::NextPage)
                        .foreground(Color::rgb(207, 222, 234))
                        .background(Color::rgb(40, 53, 69))
                        .padding(EdgeInsets::symmetric(
                            LogicalLength::from(10_u16),
                            LogicalLength::from(6_u16),
                        ))
                        .radius(Radius::all(LogicalLength::from(7_u16)))
                        .into_element(),
                ])
                .gap(LogicalLength::from(12_u16))
                .key("todo.page.row")
                .into_element(),
            );
        }

        if let Some(error) = &state.error {
            content.push(
                text(format!("Action failed: {error}"))
                    .foreground(Color::rgb(255, 174, 178))
                    .background(Color::rgb(68, 35, 47))
                    .padding(EdgeInsets::all(LogicalLength::from(10_u16)))
                    .radius(Radius::all(LogicalLength::from(8_u16)))
                    .key("todo.error")
                    .into_element(),
            );
        }

        column(content)
            .key("todo.root")
            .gap(LogicalLength::from(8_u16))
            .padding(EdgeInsets::all(LogicalLength::from(14_u16)))
            .with_layout(
                LayoutStyle::default()
                    .with_width(LayoutDimension::Fill)
                    .with_max_width(LayoutBound::length(LogicalLength::from(780_u16))),
            )
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
                // An input proposal may arrive after Enter has committed and
                // cleared the draft. Reject it against the authoritative document,
                // never turn an editor conflict into a persisted-task error.
                let mut next_draft = state.draft.clone();
                if next_draft.get(replacement.clone()).is_none() {
                    eprintln!(
                        "todo.editor: rejected out-of-range edit; revision={}, bytes={}, range={}..{}, inserted_bytes={}",
                        state.draft_revision,
                        state.draft.len(),
                        range.start(),
                        range.end(),
                        intent.replacement_text().len()
                    );
                    return UpdateOutput::edit(EditResolution::rejected(
                        request,
                        state.draft_snapshot(),
                    ));
                }
                next_draft.replace_range(replacement, intent.replacement_text());
                let next_selection = intent.proposed_selection().active();
                let next_snapshot = TextDocumentSnapshot::new(
                    DRAFT_DOCUMENT_ID,
                    TextDocumentRevision::new(state.draft_revision + 1),
                );
                if TextPosition::new(
                    next_snapshot,
                    &next_draft,
                    next_selection,
                    TextAffinity::Downstream,
                )
                .is_err()
                    || next_draft.chars().any(char::is_control)
                {
                    eprintln!(
                        "todo.editor: rejected invalid resulting text/selection; revision={}, bytes={}",
                        state.draft_revision,
                        next_draft.len()
                    );
                    return UpdateOutput::edit(EditResolution::rejected(
                        request,
                        state.draft_snapshot(),
                    ));
                }
                state.draft = next_draft;
                state.draft_selection = next_selection;
                state.draft_revision += 1;
                state.error = None;
                UpdateOutput::edit(EditResolution::accepted(request, state.draft_snapshot()))
            }
            TodoAction::AddTask => {
                let created = state.execute(TodoCommand::Add {
                    title: state.draft.clone(),
                });
                if created {
                    state.page_index = (state.service.tasks().len() - 1) / TASKS_PER_PAGE;
                    state.draft.clear();
                    state.draft_selection = 0;
                    state.draft_revision += 1;
                }
                UpdateOutput::effects(Effects::none())
            }
            TodoAction::PreviousPage => {
                state.page_index = state.page_index.saturating_sub(1);
                UpdateOutput::effects(Effects::none())
            }
            TodoAction::NextPage => {
                state.page_index = (state.page_index + 1).min(state.page_count() - 1);
                UpdateOutput::effects(Effects::none())
            }
            TodoAction::SetCompleted { id, completed } => {
                state.execute(TodoCommand::SetCompleted { id, completed });
                UpdateOutput::effects(Effects::none())
            }
            TodoAction::RemoveTask(id) => {
                if state.execute(TodoCommand::Remove { id }) {
                    state.page_index = state.page_index.min(state.page_count() - 1);
                }
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
    fn paged_tasks_preserve_full_storage_and_clamp_after_last_removal() {
        let path = std::env::temp_dir().join(format!(
            "runen-lab-todo-pages-{}-{}.json",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
        ));
        let storage = TodoStorage::at(path.clone());
        let mut service = TodoService::at(storage.clone()).unwrap();
        for number in 0..=TASKS_PER_PAGE {
            service
                .execute(TodoCommand::Add {
                    title: format!("Task {number}"),
                })
                .unwrap();
        }
        let mut state = TodoUiState::new(service);
        assert_eq!(state.page_count(), 2);
        assert_eq!(state.visible_tasks().len(), TASKS_PER_PAGE);
        let last = state.service.tasks()[TASKS_PER_PAGE].id();

        let _ = TodoUi::update(&mut state, TodoAction::NextPage);
        assert_eq!(state.page_index, 1);
        assert_eq!(state.visible_tasks().len(), 1);
        assert_eq!(state.visible_tasks()[0].id(), last);

        let _ = TodoUi::update(
            &mut state,
            TodoAction::SetCompleted {
                id: last,
                completed: true,
            },
        );
        assert!(storage.load().unwrap().tasks()[TASKS_PER_PAGE].completed());
        let _ = TodoUi::update(&mut state, TodoAction::RemoveTask(last));
        assert_eq!(state.page_index, 0);
        assert_eq!(state.page_count(), 1);
        assert_eq!(state.visible_tasks().len(), TASKS_PER_PAGE);
        assert_eq!(storage.load().unwrap().tasks().len(), TASKS_PER_PAGE);
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
