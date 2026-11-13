mod host;
pub(crate) mod theme;

#[cfg(test)]
mod layout_tests;

use crate::model::{Task, TaskId, TodoCommand};
use crate::service::TodoService;
use runenui_core::{
    EditIntent, EditResolution, Effects, Element, FlexContainerStyle, FlexDirection, FlexItemStyle,
    FontFamily, FontWeight, GenericFontFamily, IntoUpdateOutput, ItemAlignment, LayoutBound,
    LayoutContainer, LayoutDimension, LayoutFactor, LayoutStyle, LogicalLength, NoHostProtocol,
    ProducerBudget, SemanticEditableMode, TextAffinity, TextDocumentId, TextDocumentRevision,
    TextDocumentSnapshot, TextPosition, TextSelection, Typography, UiApp, UpdateOutput, View,
    WorkResidencyBudget, WorkStartBudget, button, column, row, text, text_field,
};

pub use host::install;

const DRAFT_DOCUMENT_ID: TextDocumentId = TextDocumentId::new(1);

// Keep the mounted UI independent of the storage model's 10,000-task capacity.
// RunenUI does not yet expose a virtualized list; page the application-owned
// collection instead of reserving unbounded routed focus/output capacity.
// Five rows leave room for section gaps and paging at the default 800x600 size.
const TASKS_PER_PAGE: usize = 5;
// Header, composer, section labels, root, empty/error state.
const BASE_UI_NODES: usize = 22;
const TASK_UI_NODES: usize = 4;
const PAGE_UI_NODES: usize = 4;
const MAX_UI_NODES: usize = BASE_UI_NODES + TASK_UI_NODES * TASKS_PER_PAGE + PAGE_UI_NODES;
// The maximum page exposes a text field, Add, two actions per task and two
// paging actions. Focus transfer can route each event-bearing widget four
// times; ordinary routing and framework defaults require additional headroom.
const MAX_INTERACTION_OUTPUTS: usize = 4 * (7 + 2 * TASKS_PER_PAGE) + 4;

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

fn vertically_centered_row_layout() -> LayoutStyle {
    // with_layout replaces the complete authored style; include both the
    // horizontal Flex container and its gap to retain cross-axis alignment.
    LayoutStyle::default()
        .with_container(LayoutContainer::Flex(
            FlexContainerStyle::default()
                .with_direction(FlexDirection::Row)
                .with_align_items(ItemAlignment::Center),
        ))
        .with_gap(LogicalLength::from(theme::SPACE_3))
}

// Opt-in, content-free native interaction diagnostics. This does not change
// RunenUI's M13 receipt, document, or event ownership.
fn diagnostics_enabled() -> bool {
    std::env::var_os("RUNEN_LAB_TODO_DIAGNOSTICS").is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskFilter {
    All,
    Active,
    Completed,
}

pub struct TodoUi;

pub struct TodoUiState {
    service: TodoService,
    page_index: usize,
    filter: TaskFilter,
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
            filter: TaskFilter::All,
            draft: String::new(),
            draft_revision: 0,
            draft_selection: 0,
            error: None,
        }
    }

    fn filtered_count(&self) -> usize {
        self.service
            .tasks()
            .iter()
            .filter(|task| match self.filter {
                TaskFilter::All => true,
                TaskFilter::Active => !task.completed(),
                TaskFilter::Completed => task.completed(),
            })
            .count()
    }

    fn page_count(&self) -> usize {
        self.filtered_count().div_ceil(TASKS_PER_PAGE).max(1)
    }

    fn visible_tasks(&self) -> Vec<&Task> {
        let tasks = self.service.tasks();
        // All groups unfinished tasks before completed tasks, without
        // changing original task IDs, order within a group, or storage.
        let ordered: Vec<&Task> = match self.filter {
            TaskFilter::All => tasks
                .iter()
                .filter(|task| !task.completed())
                .chain(tasks.iter().filter(|task| task.completed()))
                .collect(),
            TaskFilter::Active => tasks.iter().filter(|task| !task.completed()).collect(),
            TaskFilter::Completed => tasks.iter().filter(|task| task.completed()).collect(),
        };
        let start = self
            .page_index
            .saturating_mul(TASKS_PER_PAGE)
            .min(ordered.len());
        ordered
            .into_iter()
            .skip(start)
            .take(TASKS_PER_PAGE)
            .collect()
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
    SetFilter(TaskFilter),
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
                text("RUNEN LAB  /  TODO")
                    .foreground(theme::MUTED)
                    .typography(ui_font(11, true))
                    .key("todo.eyebrow")
                    .into_element(),
                text("Your tasks")
                    .id("todo.heading")
                    .foreground(theme::TEXT)
                    .typography(ui_font(27, true))
                    .key("todo.heading")
                    .into_element(),
                text(format!("{open} to do  /  {completed} completed"))
                    .foreground(theme::SECONDARY_TEXT)
                    .typography(ui_font(13, false))
                    .id("todo.summary")
                    .key("todo.summary")
                    .into_element(),
            ])
            .gap(LogicalLength::from(theme::SPACE_2))
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
                .on_submit(|| TodoAction::AddTask)
                .id("todo.new-task")
                .key("todo.new-task")
                .with_layout(
                    LayoutStyle::default()
                        .with_width(LayoutDimension::length(LogicalLength::from(240_u16)))
                        .with_min_width(LayoutBound::length(LogicalLength::from(110_u16)))
                        .with_flex_item(FlexItemStyle::default().with_grow(LayoutFactor::ONE)),
                )
                .placeholder("What needs doing?")
                .background(theme::INPUT)
                .foreground(theme::TEXT)
                .padding(theme::insets(theme::SPACE_3, theme::SPACE_2))
                .radius(theme::radius(theme::SPACE_2))
                .into_element(),
                button("Add task")
                    .id("todo.add")
                    .key("todo.add")
                    .on_activate(|| TodoAction::AddTask)
                    .background(theme::ACCENT)
                    .foreground(theme::ACCENT_TEXT)
                    .typography(ui_font(13, true))
                    .padding(theme::insets(theme::SPACE_4, theme::SPACE_3))
                    .radius(theme::radius(theme::SPACE_2))
                    .into_element(),
            ])
            .with_layout(vertically_centered_row_layout())
            .background(theme::SURFACE)
            .padding(theme::all(theme::SPACE_3))
            .radius(theme::radius(theme::SPACE_3))
            .key("todo.new-task-row")
            .into_element(),
            row([
                button(format!("All  {}", tasks.len()))
                    .id("todo.filter.all")
                    .key("todo.filter.all")
                    .on_activate(|| TodoAction::SetFilter(TaskFilter::All))
                    .background(if state.filter == TaskFilter::All {
                        theme::FILTER_SELECTED
                    } else {
                        theme::SURFACE
                    })
                    .foreground(if state.filter == TaskFilter::All {
                        theme::TEXT
                    } else {
                        theme::SECONDARY_TEXT
                    })
                    .typography(ui_font(12, state.filter == TaskFilter::All))
                    .padding(theme::insets(theme::SPACE_3, theme::SPACE_2))
                    .radius(theme::radius(theme::SPACE_2))
                    .into_element(),
                button(format!("Active  {open}"))
                    .id("todo.filter.active")
                    .key("todo.filter.active")
                    .on_activate(|| TodoAction::SetFilter(TaskFilter::Active))
                    .background(if state.filter == TaskFilter::Active {
                        theme::FILTER_SELECTED
                    } else {
                        theme::SURFACE
                    })
                    .foreground(if state.filter == TaskFilter::Active {
                        theme::TEXT
                    } else {
                        theme::SECONDARY_TEXT
                    })
                    .typography(ui_font(12, state.filter == TaskFilter::Active))
                    .padding(theme::insets(theme::SPACE_3, theme::SPACE_2))
                    .radius(theme::radius(theme::SPACE_2))
                    .into_element(),
                button(format!("Completed  {completed}"))
                    .id("todo.filter.completed")
                    .key("todo.filter.completed")
                    .on_activate(|| TodoAction::SetFilter(TaskFilter::Completed))
                    .background(if state.filter == TaskFilter::Completed {
                        theme::FILTER_SELECTED
                    } else {
                        theme::SURFACE
                    })
                    .foreground(if state.filter == TaskFilter::Completed {
                        theme::TEXT
                    } else {
                        theme::SECONDARY_TEXT
                    })
                    .typography(ui_font(12, state.filter == TaskFilter::Completed))
                    .padding(theme::insets(theme::SPACE_3, theme::SPACE_2))
                    .radius(theme::radius(theme::SPACE_2))
                    .into_element(),
            ])
            .gap(LogicalLength::from(theme::SPACE_2))
            .key("todo.filters")
            .into_element(),
            row([
                text(match state.filter {
                    TaskFilter::All => "ALL TASKS",
                    TaskFilter::Active => "TO DO",
                    TaskFilter::Completed => "COMPLETED",
                })
                .foreground(theme::SECONDARY_TEXT)
                .typography(ui_font(11, true))
                .id("todo.section-title")
                .key("todo.section-title")
                .into_element(),
                text(format!("{} shown", state.filtered_count()))
                    .foreground(theme::MUTED)
                    .typography(ui_font(11, false))
                    .key("todo.total")
                    .into_element(),
            ])
            .gap(LogicalLength::from(theme::SPACE_3))
            .key("todo.section")
            .into_element(),
        ];

        let visible = state.visible_tasks();
        if visible.is_empty() {
            let (heading, description) = match state.filter {
                TaskFilter::All => ("Nothing here yet", "Add your first task above."),
                TaskFilter::Active => ("Nothing to do", "All tasks are complete."),
                TaskFilter::Completed => ("No completed tasks", "Completed tasks appear here."),
            };
            content.push(
                column([
                    text(heading)
                        .foreground(theme::TEXT)
                        .typography(ui_font(16, true))
                        .key("todo.empty-title")
                        .into_element(),
                    text(description)
                        .foreground(theme::SECONDARY_TEXT)
                        .typography(ui_font(13, false))
                        .key("todo.empty-description")
                        .into_element(),
                ])
                .gap(LogicalLength::from(theme::SPACE_2))
                .background(theme::SURFACE)
                .padding(theme::all(theme::SPACE_4))
                .radius(theme::radius(theme::SPACE_3))
                .key("todo.empty")
                .into_element(),
            );
        } else {
            let mut rows: Vec<Element<TodoAction>> = Vec::new();
            let mut group_label_emitted = false;
            for task in visible {
                let id = task.id();
                let done = task.completed();
                if state.filter == TaskFilter::All && done && !group_label_emitted {
                    rows.push(
                        text("COMPLETED")
                            .foreground(theme::SECONDARY_TEXT)
                            .typography(ui_font(11, true))
                            .key("todo.completed-group")
                            .into_element(),
                    );
                    group_label_emitted = true;
                }
                rows.push(
                    row([
                        button(if done { "Reopen" } else { "Complete" })
                            .id(format!("task.{}.toggle", id.get()))
                            .key(format!("task.{}.toggle", id.get()))
                            .on_activate(move || TodoAction::SetCompleted {
                                id,
                                completed: !done,
                            })
                            .foreground(if done {
                                theme::SECONDARY_TEXT
                            } else {
                                theme::ACCENT_TEXT
                            })
                            .background(if done { theme::INPUT } else { theme::ACCENT })
                            .typography(ui_font(12, true))
                            .padding(theme::insets(theme::SPACE_2, theme::SPACE_1))
                            .radius(theme::radius(theme::SPACE_2))
                            .into_element(),
                        text(task.title())
                            .id(format!("task.{}.title", id.get()))
                            .key(format!("task.{}.title", id.get()))
                            .foreground(if done { theme::MUTED } else { theme::TEXT })
                            .typography(ui_font(14, false))
                            .with_layout(LayoutStyle::default().with_flex_item(
                                FlexItemStyle::default().with_grow(LayoutFactor::ONE),
                            ))
                            .into_element(),
                        button("Remove")
                            .id(format!("task.{}.remove", id.get()))
                            .key(format!("task.{}.remove", id.get()))
                            .on_activate(move || TodoAction::RemoveTask(id))
                            .foreground(theme::SECONDARY_TEXT)
                            .background(theme::INPUT)
                            .typography(ui_font(12, false))
                            .padding(theme::insets(theme::SPACE_2, theme::SPACE_1))
                            .radius(theme::radius(theme::SPACE_2))
                            .into_element(),
                    ])
                    .with_layout(vertically_centered_row_layout())
                    .background(if done { theme::SURFACE } else { theme::TASK })
                    .padding(theme::insets(theme::SPACE_3, theme::SPACE_2))
                    .radius(theme::radius(theme::SPACE_3))
                    .id(format!("task.{}.row", id.get()))
                    .key(format!("task.{}.row", id.get()))
                    .into_element(),
                );
            }
            content.push(
                column(rows)
                    .gap(LogicalLength::from(theme::SPACE_3))
                    .key("todo.task-list")
                    .into_element(),
            );
        }

        if state.page_count() > 1 {
            content.push(
                row([
                    button("Previous")
                        .id("todo.page.previous")
                        .key("todo.page.previous")
                        .enabled(state.page_index > 0)
                        .on_activate(|| TodoAction::PreviousPage)
                        .foreground(theme::TEXT)
                        .background(theme::INPUT)
                        .typography(ui_font(12, false))
                        .padding(theme::insets(theme::SPACE_3, theme::SPACE_2))
                        .radius(theme::radius(theme::SPACE_2))
                        .into_element(),
                    text(format!(
                        "Page {} of {}",
                        state.page_index + 1,
                        state.page_count()
                    ))
                    .id("todo.page.label")
                    .key("todo.page.label")
                    .foreground(theme::SECONDARY_TEXT)
                    .typography(ui_font(12, false))
                    .into_element(),
                    button("Next")
                        .id("todo.page.next")
                        .key("todo.page.next")
                        .enabled(state.page_index + 1 < state.page_count())
                        .on_activate(|| TodoAction::NextPage)
                        .foreground(theme::TEXT)
                        .background(theme::INPUT)
                        .typography(ui_font(12, false))
                        .padding(theme::insets(theme::SPACE_3, theme::SPACE_2))
                        .radius(theme::radius(theme::SPACE_2))
                        .into_element(),
                ])
                .with_layout(vertically_centered_row_layout())
                .key("todo.page.row")
                .into_element(),
            );
        }

        if let Some(error) = &state.error {
            content.push(
                text(format!("Action failed: {error}"))
                    .foreground(theme::DANGER)
                    .background(theme::DANGER_SURFACE)
                    .padding(theme::all(theme::SPACE_3))
                    .radius(theme::radius(theme::SPACE_2))
                    .key("todo.error")
                    .into_element(),
            );
        }

        // with_layout replaces the whole authored LayoutStyle. Preserve the
        // vertical Flex container and its gap alongside width constraints.
        // Otherwise a default Block container silently discards column spacing.
        column(content)
            .key("todo.root")
            .padding(theme::all(theme::SPACE_4))
            .background(theme::CANVAS)
            .with_layout(
                LayoutStyle::default()
                    .with_container(LayoutContainer::Flex(
                        FlexContainerStyle::default().with_direction(FlexDirection::Column),
                    ))
                    .with_gap(LogicalLength::from(theme::SPACE_3))
                    .with_width(LayoutDimension::Fill)
                    .with_max_width(LayoutBound::length(LogicalLength::from(780_u16))),
            )
            .foreground(theme::TEXT)
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
                if diagnostics_enabled() {
                    eprintln!(
                        "todo.editor: accepted revision={} draft_bytes={}",
                        state.draft_revision,
                        state.draft.len()
                    );
                }
                UpdateOutput::edit(EditResolution::accepted(request, state.draft_snapshot()))
            }
            TodoAction::AddTask => {
                if diagnostics_enabled() {
                    eprintln!(
                        "todo.action: add-start revision={} draft_bytes={}",
                        state.draft_revision,
                        state.draft.len()
                    );
                }
                let created = state.execute(TodoCommand::Add {
                    title: state.draft.clone(),
                });
                if diagnostics_enabled() {
                    eprintln!(
                        "todo.action: add-result persisted={} task_count={} error_present={}",
                        created,
                        state.service.tasks().len(),
                        state.error.is_some()
                    );
                }
                if created {
                    if state.filter == TaskFilter::Completed {
                        state.filter = TaskFilter::Active;
                    }
                    // New titles are active, so they precede completed tasks
                    // in All; navigate to the active group, not the last page.
                    let active = state
                        .service
                        .tasks()
                        .iter()
                        .filter(|task| !task.completed())
                        .count();
                    state.page_index = active.saturating_sub(1) / TASKS_PER_PAGE;
                    state.draft.clear();
                    state.draft_selection = 0;
                    state.draft_revision += 1;
                }
                UpdateOutput::effects(Effects::none())
            }
            TodoAction::SetFilter(filter) => {
                state.filter = filter;
                state.page_index = 0;
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
                let persisted = state.execute(TodoCommand::SetCompleted { id, completed });
                if persisted {
                    state.page_index = state.page_index.min(state.page_count() - 1);
                }
                if diagnostics_enabled() {
                    eprintln!(
                        "todo.action: completed-result id={} completed={} persisted={} task_count={} page_index={} error_present={}",
                        id.get(),
                        completed,
                        persisted,
                        state.service.tasks().len(),
                        state.page_index,
                        state.error.is_some()
                    );
                }
                UpdateOutput::effects(Effects::none())
            }
            TodoAction::RemoveTask(id) => {
                let persisted = state.execute(TodoCommand::Remove { id });
                if persisted {
                    state.page_index = state.page_index.min(state.page_count() - 1);
                }
                if diagnostics_enabled() {
                    eprintln!(
                        "todo.action: remove-result id={} persisted={} task_count={} page_index={} error_present={}",
                        id.get(),
                        persisted,
                        state.service.tasks().len(),
                        state.page_index,
                        state.error.is_some()
                    );
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
    fn status_filters_group_visible_tasks_and_never_mutate_storage() {
        let path = std::env::temp_dir().join(format!(
            "runen-lab-todo-filter-{}-{}.json",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
        ));
        assert!(!path.exists(), "filter fixture must be isolated");
        let storage = TodoStorage::at(path.clone());
        let mut service = TodoService::at(storage.clone()).unwrap();
        for number in 1..=9 {
            service
                .execute(TodoCommand::Add {
                    title: format!("Task {number}"),
                })
                .unwrap();
        }
        for index in [1_usize, 8] {
            let id = service.tasks()[index].id();
            service
                .execute(TodoCommand::SetCompleted {
                    id,
                    completed: true,
                })
                .unwrap();
        }
        let before_filter = std::fs::read(&path).unwrap();
        let mut state = TodoUiState::new(service);
        assert_eq!(state.filter, TaskFilter::All);
        let ids = state
            .visible_tasks()
            .iter()
            .map(|task| task.id().get())
            .collect::<Vec<_>>();
        let expected = [1, 3, 4, 5, 6, 7, 8]
            .into_iter()
            .take(TASKS_PER_PAGE)
            .collect::<Vec<_>>();
        assert_eq!(ids, expected);

        let _ = TodoUi::update(&mut state, TodoAction::NextPage);
        assert_eq!(state.page_index, 1);
        let _ = TodoUi::update(&mut state, TodoAction::SetFilter(TaskFilter::Completed));
        assert_eq!(state.page_index, 0);
        assert_eq!(state.filtered_count(), 2);
        assert_eq!(
            state
                .visible_tasks()
                .iter()
                .map(|task| task.id().get())
                .collect::<Vec<_>>(),
            vec![2, 9]
        );
        assert_eq!(std::fs::read(&path).unwrap(), before_filter);
        let _ = TodoUi::update(&mut state, TodoAction::SetFilter(TaskFilter::Active));
        assert_eq!(state.filtered_count(), 7);
        assert_eq!(state.page_count(), 2);
        assert_eq!(std::fs::read(&path).unwrap(), before_filter);

        // Completing an active task drops it from Active while persisting the
        // state change; selecting Completed then allows Reopen.
        let active_id = state.visible_tasks()[0].id();
        let _ = TodoUi::update(
            &mut state,
            TodoAction::SetCompleted {
                id: active_id,
                completed: true,
            },
        );
        assert_eq!(state.filtered_count(), 6);
        assert!(
            !state
                .visible_tasks()
                .iter()
                .any(|task| task.id() == active_id)
        );
        assert!(
            storage
                .load()
                .unwrap()
                .tasks()
                .iter()
                .any(|task| task.id() == active_id && task.completed())
        );
        let _ = TodoUi::update(&mut state, TodoAction::SetFilter(TaskFilter::Completed));
        assert_eq!(state.filtered_count(), 3);
        let _ = TodoUi::update(
            &mut state,
            TodoAction::SetCompleted {
                id: active_id,
                completed: false,
            },
        );
        assert_eq!(state.filtered_count(), 2);

        // The composer always surfaces its new active task, even if the
        // user was previously inspecting Completed.
        state.draft = "Fresh task".to_owned();
        state.draft_selection = state.draft.len();
        let _ = TodoUi::update(&mut state, TodoAction::AddTask);
        assert_eq!(state.filter, TaskFilter::Active);
        assert!(
            state
                .visible_tasks()
                .iter()
                .any(|task| task.title() == "Fresh task")
        );
        assert_eq!(storage.load().unwrap().tasks().len(), 10);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn adding_to_all_selects_new_active_task_not_last_completed_page() {
        let path = std::env::temp_dir().join(format!(
            "runen-lab-todo-active-in-all-{}-{}.json",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
        ));
        assert!(!path.exists(), "All filter fixture must start empty");
        let storage = TodoStorage::at(path.clone());
        let mut service = TodoService::at(storage.clone()).unwrap();
        for index in 0..10 {
            service
                .execute(TodoCommand::Add {
                    title: format!("Existing {index}"),
                })
                .unwrap();
        }
        for index in 1..10 {
            let id = service.tasks()[index].id();
            service
                .execute(TodoCommand::SetCompleted {
                    id,
                    completed: true,
                })
                .unwrap();
        }
        let mut state = TodoUiState::new(service);
        assert_eq!(state.filter, TaskFilter::All);
        assert_eq!(state.page_count(), 2);
        state.page_index = 1;
        state.draft = "Newest active".to_owned();
        state.draft_selection = state.draft.len();

        let _ = TodoUi::update(&mut state, TodoAction::AddTask);
        assert_eq!(state.page_index, 0);
        assert!(
            state
                .visible_tasks()
                .iter()
                .any(|task| task.title() == "Newest active")
        );
        let all = storage.load().unwrap();
        assert_eq!(all.tasks().len(), 11);
        assert_eq!(all.tasks()[10].title(), "Newest active");
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
