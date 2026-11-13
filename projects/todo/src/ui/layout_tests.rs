use super::*;
use crate::storage::TodoStorage;
use runenui_runtime::{
    AppRuntime, FontFamilyName, GenericFontFamily, LogicalSize, SurfaceBuildContext,
};

const FONT: &[u8] = include_bytes!("../../tests/fixtures/Cantarell-Regular.ttf");

#[test]
fn empty_task_field_publishes_visible_clipped_placeholder() {
    let path =
        std::env::temp_dir().join(format!("runen-lab-todo-layout-{}.json", std::process::id(),));
    assert!(
        !path.exists(),
        "layout fixture must start without task data"
    );
    let state = TodoUiState::new(TodoService::at(TodoStorage::at(path)).unwrap());
    let mut runtime = AppRuntime::<TodoUi>::mount(state)
        .expect("initial Todo tree fits the declared producer budget");
    assert!(runtime.register_text_font_bytes(FONT.to_vec()).unwrap() > 0);
    runtime
        .set_text_generic_family_mapping(
            GenericFontFamily::SansSerif,
            &[FontFamilyName::new("Cantarell").unwrap()],
        )
        .unwrap();

    let style = runenui_core::StyleEnvironment::default();
    for surface_width in [800.0, 320.0] {
        let publication = runtime
            .publish_surface(&SurfaceBuildContext::tight(
                &style,
                LogicalSize::try_new(surface_width, 600.0).unwrap(),
            ))
            .unwrap();
        let field = publication
            .frame()
            .nodes()
            .iter()
            .find(|node| {
                node.authored_id()
                    .is_some_and(|id| id.as_str() == "todo.new-task")
            })
            .expect("Todo publishes its authored task field");
        let hints: Vec<_> = publication
            .paint_scene()
            .items()
            .iter()
            .filter(|item| {
                item.primitive().as_shaped_text_run().is_some() && item.opacity().get() == 0.5
            })
            .collect();
        println!(
            "surface width: {surface_width}; initial field bounds: {:?}; placeholder items: {}",
            field.bounds(),
            hints.len()
        );
        for hint in &hints {
            println!("placeholder clips: {:?}", hint.clips());
        }
        assert!(
            field.bounds().width() > 0.0,
            "empty task field must have visible width"
        );
        assert!(
            field.bounds().height() > 0.0,
            "empty task field must have visible height"
        );
        assert_eq!(
            hints.len(),
            1,
            "empty field retains its half-opacity shaped hint"
        );
        assert!(!hints[0].clips().is_empty(), "placeholder remains clipped");
        let run = hints[0].primitive().as_shaped_text_run().unwrap();
        let resource = publication
            .paint_scene()
            .shaped_text_resource(run.resource_ref())
            .unwrap();
        assert!(
            !resource.glyphs().is_empty(),
            "placeholder has shaped glyphs"
        );
        let hint_width = resource
            .glyphs()
            .iter()
            .map(|glyph| glyph.x() + glyph.advance())
            .fold(0.0_f32, f32::max);
        println!(
            "placeholder glyph count: {}; inline extent: {hint_width}",
            resource.glyphs().len()
        );
        assert!(
            field.bounds().width() >= hint_width,
            "field must expose the entire initial placeholder, not clip it to an empty-source sliver"
        );
        let runenui_core::SceneShape::Rect(clip) = hints[0].clips()[0].shape() else {
            panic!("field placeholder must retain its rectangular viewport clip");
        };
        assert_eq!(clip.size(), field.bounds().size());
        let semantic_field = publication
            .semantic_publication()
            .snapshot()
            .nodes()
            .iter()
            .find(|node| node.placeholder() == Some("What needs doing?"))
            .expect("placeholder remains in the field semantics");
        assert_eq!(
            semantic_field
                .editable()
                .and_then(|editable| editable.value()),
            Some("")
        );
        let add = publication
            .frame()
            .nodes()
            .iter()
            .find(|node| {
                node.authored_id()
                    .is_some_and(|id| id.as_str() == "todo.add")
            })
            .unwrap();
        assert!(field.bounds().x() + field.bounds().width() <= add.bounds().x());
        assert!(add.bounds().x() + add.bounds().width() <= surface_width);
        assert!(add.bounds().y() + add.bounds().height() <= 600.0);
        let heading = publication
            .frame()
            .nodes()
            .iter()
            .find(|node| {
                node.authored_id()
                    .is_some_and(|id| id.as_str() == "todo.heading")
            })
            .expect("designed heading is published");
        assert!(heading.bounds().width() > 0.0 && heading.bounds().height() > 0.0);
        assert!(heading.bounds().x() >= 0.0 && heading.bounds().y() >= 0.0);
    }
}

#[test]
fn actual_todo_add_button_preserves_m13_pointer_settlement() {
    use runenui_core::{
        InputDeviceId, LogicalPoint, PointerButton, PointerButtons, PointerDeviceKind,
        PointerEvent, PointerId, PointerPhase,
    };
    use runenui_runtime::{
        InputArbitrationRecord, PumpBudget, PumpOutcome, RuntimeStatus, UiInputFinality,
    };

    let path = std::env::temp_dir().join(format!(
        "runen-lab-todo-m13-pointer-{}.json",
        std::process::id()
    ));
    assert!(!path.exists(), "M13 fixture cannot reuse a saved task file");
    let state = TodoUiState::new(TodoService::at(TodoStorage::at(path)).unwrap());
    let mut runtime = AppRuntime::<TodoUi>::mount(state).expect("actual Todo initial admission");
    assert!(runtime.register_text_font_bytes(FONT.to_vec()).unwrap() > 0);
    runtime
        .set_text_generic_family_mapping(
            GenericFontFamily::SansSerif,
            &[FontFamilyName::new("Cantarell").unwrap()],
        )
        .unwrap();

    let style = runenui_core::StyleEnvironment::default();
    let publication = runtime
        .publish_surface(&SurfaceBuildContext::tight(
            &style,
            LogicalSize::try_new(800.0, 600.0).unwrap(),
        ))
        .expect("actual mounted Todo publication");
    let add_bounds = publication
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|id| id.as_str() == "todo.add")
        })
        .expect("the real Add button is mounted")
        .bounds();
    assert!(add_bounds.width() > 0.0 && add_bounds.height() > 0.0);
    let point = LogicalPoint::new(
        add_bounds.x() + add_bounds.width() / 2.0,
        add_bounds.y() + add_bounds.height() / 2.0,
    )
    .expect("the displayed Add button center is finite");
    let device = InputDeviceId::new(11).expect("original nonzero device");
    let pointer = PointerId::new(1).expect("original nonzero pointer");

    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        let mut event = PointerEvent::new(
            pointer,
            PointerDeviceKind::Mouse,
            phase,
            point,
            publication.input_context().clone(),
        )
        .with_device_id(device);
        if phase == PointerPhase::Down {
            event = event
                .with_buttons(PointerButtons::new([PointerButton::Primary]))
                .with_changed_button(PointerButton::Primary);
        } else if phase == PointerPhase::Up {
            event = event.with_changed_button(PointerButton::Primary);
        }
        let submission = runtime
            .submit_pointer(event)
            .unwrap_or_else(|error| panic!("Todo {phase:?} admission failed: {error:?}"));
        let batch = runtime
            .pump(PumpBudget::new(128, 128, 128, 128))
            .expect("canonical bounded RunenUI M13 pump must return evidence");
        let report = batch.report();
        assert!(
            batch.pause_reason().is_none()
                && batch.final_ownership().status() == RuntimeStatus::Running
                && matches!(
                    report.outcome(),
                    PumpOutcome::Quiescent | PumpOutcome::BudgetExhausted
                ),
            "Todo {phase:?} M13 failed: outcome={:?}, pause={:?}, final_status={:?}, processed_through={:?}, original_sequence={:?}",
            report.outcome(),
            batch.pause_reason(),
            batch.final_ownership().status(),
            batch.processed_through(),
            submission.sequence(),
        );
        assert!(
            batch
                .processed_through()
                .is_some_and(|sequence| sequence >= submission.sequence()),
            "Todo {phase:?} M13 did not reach the original pointer"
        );
        let receipts = batch
            .ordered_records()
            .iter()
            .filter_map(|record| match record {
                InputArbitrationRecord::InputSettled(settled)
                    if settled.sequence() == submission.sequence() =>
                {
                    Some(settled.finality())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            receipts.len(),
            1,
            "Todo {phase:?} must produce exactly one matching terminal M13 receipt"
        );
        assert!(
            matches!(receipts[0], UiInputFinality::Committed(_)),
            "Todo {phase:?} must preserve the exact successful pointer finality: {:?}",
            receipts[0]
        );
    }
}

#[test]
fn actual_todo_text_focus_edit_and_enter_submits_once_without_whitespace() {
    use runenui_core::{
        CommittedTextEvent, InputDeviceId, KeyLocation, KeyModifiers, KeyboardCompositionState,
        KeyboardEvent, KeyboardPhase, LogicalKey, LogicalPoint, PhysicalKey, PointerButton,
        PointerButtons, PointerDeviceKind, PointerEvent, PointerId, PointerPhase,
        SurfaceInputContext,
    };
    use runenui_runtime::{
        InputArbitrationRecord, InputPumpBatch, PumpBudget, PumpOutcome, RuntimeStatus,
        UiInputFinality,
    };

    fn click(
        runtime: &mut AppRuntime<TodoUi>,
        context: &SurfaceInputContext,
        point: LogicalPoint,
        device: InputDeviceId,
    ) {
        let pointer = PointerId::new(1).expect("same original pointer stream");
        for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
            let mut event = PointerEvent::new(
                pointer,
                PointerDeviceKind::Mouse,
                phase,
                point,
                context.clone(),
            )
            .with_device_id(device);
            if phase == PointerPhase::Down {
                event = event
                    .with_buttons(PointerButtons::new([PointerButton::Primary]))
                    .with_changed_button(PointerButton::Primary);
            } else if phase == PointerPhase::Up {
                event = event.with_changed_button(PointerButton::Primary);
            }
            let submitted = runtime
                .submit_pointer(event)
                .unwrap_or_else(|error| panic!("Todo {phase:?} input rejected: {error:?}"));
            let batch = runtime
                .pump(PumpBudget::new(128, 128, 128, 128))
                .expect("canonical M13 mouse pump");
            assert!(
                batch.pause_reason().is_none()
                    && batch.final_ownership().status() == RuntimeStatus::Running
                    && matches!(
                        batch.report().outcome(),
                        PumpOutcome::Quiescent | PumpOutcome::BudgetExhausted
                    ),
                "Todo {phase:?} unsettled after original input: outcome={:?}, pause={:?}, status={:?}",
                batch.report().outcome(),
                batch.pause_reason(),
                batch.final_ownership().status(),
            );
            assert!(
                batch
                    .processed_through()
                    .is_some_and(|through| through >= submitted.sequence()),
                "Todo {phase:?} original pointer was not processed"
            );
            let exact = batch
                .ordered_records()
                .iter()
                .filter_map(|record| match record {
                    InputArbitrationRecord::InputSettled(settled)
                        if settled.sequence() == submitted.sequence() =>
                    {
                        Some(settled.finality())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(exact.len(), 1, "Todo {phase:?} needs one M13 receipt");
            assert!(
                matches!(exact[0], UiInputFinality::Committed(_)),
                "Todo {phase:?} finality: {:?}",
                exact[0]
            );
        }
    }

    fn prove_text(batch: &InputPumpBatch, sequence: runenui_core::WorkSequence) {
        assert!(batch.pause_reason().is_none(), "text M13 cannot pause");
        assert_eq!(batch.final_ownership().status(), RuntimeStatus::Running);
        assert!(
            batch
                .processed_through()
                .is_some_and(|through| through >= sequence),
            "paired keyboard/text input must reach its original sequence"
        );
        let exact = batch
            .ordered_records()
            .iter()
            .filter_map(|record| match record {
                InputArbitrationRecord::InputSettled(settled) if settled.sequence() == sequence => {
                    Some(settled.finality())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(exact.len(), 1, "keyboard/text needs one M13 receipt");
        assert!(
            matches!(exact[0], UiInputFinality::Committed(_)),
            "keyboard/text finality: {:?}",
            exact[0]
        );
    }

    let path = std::env::temp_dir().join(format!(
        "runen-lab-todo-edit-click-{}.json",
        std::process::id()
    ));
    assert!(!path.exists(), "edit fixture must not reuse persisted data");
    let storage = TodoStorage::at(path.clone());
    let state = TodoUiState::new(TodoService::at(storage.clone()).unwrap());
    let mut runtime = AppRuntime::<TodoUi>::mount(state).expect("real Todo initial mount");
    assert!(runtime.register_text_font_bytes(FONT.to_vec()).unwrap() > 0);
    runtime
        .set_text_generic_family_mapping(
            GenericFontFamily::SansSerif,
            &[FontFamilyName::new("Cantarell").unwrap()],
        )
        .unwrap();
    let style = runenui_core::StyleEnvironment::default();
    let build = SurfaceBuildContext::tight(&style, LogicalSize::try_new(800.0, 600.0).unwrap());
    let displayed = runtime.publish_surface(&build).unwrap();
    let field = displayed
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|id| id.as_str() == "todo.new-task")
        })
        .expect("real published editable target")
        .bounds();
    let device = InputDeviceId::new(11).expect("real source device");
    let field_point = LogicalPoint::new(
        field.x() + field.width() / 2.0,
        field.y() + field.height() / 2.0,
    )
    .unwrap();
    click(&mut runtime, displayed.input_context(), field_point, device);
    assert!(
        runtime
            .input_ownership()
            .unwrap()
            .keyboard()
            .focused_node()
            .is_some(),
        "clicking the actual text field must establish M13 keyboard focus"
    );

    let keyboard = KeyboardEvent::new(
        KeyboardPhase::Down,
        PhysicalKey::Code(String::from("KeyX")),
        LogicalKey::Character(String::from("x")),
        KeyModifiers::NONE,
        false,
        KeyLocation::Standard,
        KeyboardCompositionState::Inactive,
        Some(device),
    );
    let text = CommittedTextEvent::new("x", Some(device)).unwrap();
    let (keyboard_receipt, text_receipt) = runtime
        .submit_keyboard_with_text(keyboard, text)
        .expect("same-callback keyboard and committed text must be accepted");
    let batch = runtime
        .pump(PumpBudget::new(128, 128, 128, 128))
        .expect("canonical bounded paired keyboard/text pump");
    prove_text(&batch, keyboard_receipt.sequence());
    prove_text(&batch, text_receipt.sequence());

    let edited = runtime.publish_surface(&build).unwrap();
    let semantic_field = edited
        .semantic_publication()
        .snapshot()
        .nodes()
        .iter()
        .find(|node| node.placeholder() == Some("What needs doing?"))
        .expect("semantic editable survives committed text");
    assert_eq!(
        semantic_field
            .editable()
            .and_then(|editable| editable.value()),
        Some("x"),
        "paired committed text updates Todo exactly once"
    );

    // An unmodified Enter on the focused single-line field must submit
    // precisely once instead of inserting whitespace or a newline.
    let enter = KeyboardEvent::new(
        KeyboardPhase::Down,
        PhysicalKey::Code(String::from("Enter")),
        LogicalKey::Enter,
        KeyModifiers::NONE,
        false,
        KeyLocation::Standard,
        KeyboardCompositionState::Inactive,
        Some(device),
    );
    let entered = runtime
        .submit_keyboard(enter)
        .expect("original Enter reaches the focused single-line Todo field");
    let enter_batch = runtime
        .pump(PumpBudget::new(128, 128, 128, 128))
        .expect("original Enter must settle through M13");
    prove_text(&enter_batch, entered.sequence());

    let saved = storage
        .load()
        .expect("Todo persisted after original typed input");
    assert_eq!(saved.tasks().len(), 1, "exactly one task was added");
    assert_eq!(saved.tasks()[0].title(), "x");
    assert!(!saved.tasks()[0].completed());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn separated_task_cards_have_real_vertical_gaps() {
    let path = std::env::temp_dir().join(format!(
        "runen-lab-todo-card-gaps-{}.json",
        std::process::id()
    ));
    assert!(!path.exists(), "card-gap fixture must be isolated");
    let storage = TodoStorage::at(path.clone());
    let mut service = TodoService::at(storage).unwrap();
    for n in 1..=3 {
        service
            .execute(TodoCommand::Add {
                title: format!("Card {n}"),
            })
            .unwrap();
    }
    service
        .execute(TodoCommand::SetCompleted {
            id: service.tasks()[1].id(),
            completed: true,
        })
        .unwrap();
    let mut runtime = AppRuntime::<TodoUi>::mount(TodoUiState::new(service))
        .expect("initial mounted theme layout budget");
    assert!(runtime.register_text_font_bytes(FONT.to_vec()).unwrap() > 0);
    runtime
        .set_text_generic_family_mapping(
            GenericFontFamily::SansSerif,
            &[FontFamilyName::new("Cantarell").unwrap()],
        )
        .unwrap();
    let style = runenui_core::StyleEnvironment::default();
    let publication = runtime
        .publish_surface(&SurfaceBuildContext::tight(
            &style,
            LogicalSize::try_new(800.0, 600.0).unwrap(),
        ))
        .unwrap();
    let bounds = |id: &str| {
        publication
            .frame()
            .nodes()
            .iter()
            .find(|node| {
                node.authored_id()
                    .is_some_and(|authored| authored.as_str() == id)
            })
            .unwrap_or_else(|| panic!("missing rendered task card: {id}"))
            .bounds()
    };
    // All's visible order is Active 1, Active 3, then Completed 2.
    let first = bounds("task.1.row");
    let second = bounds("task.3.row");
    let completed = bounds("task.2.row");
    assert!(
        second.y() - (first.y() + first.height()) >= 8.0,
        "active task cards must have meaningful vertical separation"
    );
    assert!(
        completed.y() - (second.y() + second.height()) >= 8.0,
        "completed section and task card must remain separated"
    );
    assert!(
        completed.y() + completed.height() <= 600.0,
        "last task card must remain inside default viewport"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn persisted_task_initial_mount_and_original_pointer_remain_running() {
    use runenui_core::{
        InputDeviceId, LogicalPoint, PointerButton, PointerButtons, PointerDeviceKind,
        PointerEvent, PointerId, PointerPhase,
    };
    use runenui_runtime::{
        InputArbitrationRecord, PumpBudget, PumpOutcome, RuntimeStatus, UiInputFinality,
    };

    let path = std::env::temp_dir().join(format!(
        "runen-lab-todo-restored-m13-{}.json",
        std::process::id()
    ));
    assert!(!path.exists(), "restored fixture must start empty");
    let storage = TodoStorage::at(path.clone());
    let mut service = TodoService::at(storage.clone()).unwrap();
    service
        .execute(TodoCommand::Add {
            title: "Persisted native task".into(),
        })
        .expect("persist a real task before recreating TodoUi");
    drop(service);

    let restored = TodoUiState::new(TodoService::at(storage.clone()).unwrap());
    assert_eq!(restored.service.tasks().len(), 1);
    let config = runenui_runtime::RuntimeConfig::default()
        .with_trace_config(runenui_runtime::TraceConfig::new(1024));
    let mut runtime = AppRuntime::<TodoUi>::mount_with_config(restored, config)
        .expect("persisted one-task Todo must fit initial producer budget");
    assert!(runtime.register_text_font_bytes(FONT.to_vec()).unwrap() > 0);
    runtime
        .set_text_generic_family_mapping(
            GenericFontFamily::SansSerif,
            &[FontFamilyName::new("Cantarell").unwrap()],
        )
        .unwrap();
    let style = runenui_core::StyleEnvironment::default();
    let publication = runtime
        .publish_surface(&SurfaceBuildContext::tight(
            &style,
            LogicalSize::try_new(800.0, 600.0).unwrap(),
        ))
        .expect("restored nonempty Todo must publish without leaving Running");
    assert_eq!(
        runtime.input_ownership().unwrap().status(),
        RuntimeStatus::Running,
        "restored one-task Todo is not running after initial Present preparation"
    );
    let field = publication
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|id| id.as_str() == "todo.new-task")
        })
        .expect("restored Todo text input exists")
        .bounds();
    let point = LogicalPoint::new(
        field.x() + field.width() / 2.0,
        field.y() + field.height() / 2.0,
    )
    .unwrap();
    let device = InputDeviceId::new(11).unwrap();
    let pointer = PointerId::new(1).unwrap();
    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        let mut event = PointerEvent::new(
            pointer,
            PointerDeviceKind::Mouse,
            phase,
            point,
            publication.input_context().clone(),
        )
        .with_device_id(device);
        if phase == PointerPhase::Down {
            event = event
                .with_buttons(PointerButtons::new([PointerButton::Primary]))
                .with_changed_button(PointerButton::Primary);
        } else if phase == PointerPhase::Up {
            event = event.with_changed_button(PointerButton::Primary);
        }
        let submitted = runtime
            .submit_pointer(event)
            .unwrap_or_else(|error| panic!("restored Todo {phase:?} admission: {error:?}"));
        let batch = runtime
            .pump(PumpBudget::new(128, 128, 128, 128))
            .expect("restored Todo canonical M13 pointer pump");
        if batch.final_ownership().status() != RuntimeStatus::Running {
            let trace_kinds = runtime.trace().kinds().collect::<Vec<_>>();
            let tail = &trace_kinds[trace_kinds.len().saturating_sub(64)..];
            eprintln!("restored_pointer_phase={phase:?} trace_kinds={tail:?}");
        }
        assert!(
            batch.pause_reason().is_none()
                && batch.final_ownership().status() == RuntimeStatus::Running
                && matches!(
                    batch.report().outcome(),
                    PumpOutcome::Quiescent | PumpOutcome::BudgetExhausted
                ),
            "restored Todo {phase:?} failed Running M13 finality: {:?}",
            batch.report()
        );
        assert!(
            batch
                .processed_through()
                .is_some_and(|through| through >= submitted.sequence()),
            "original restored Todo {phase:?} sequence was not reached"
        );
        let finals = batch
            .ordered_records()
            .iter()
            .filter_map(|record| match record {
                InputArbitrationRecord::InputSettled(receipt)
                    if receipt.sequence() == submitted.sequence() =>
                {
                    Some(receipt.finality())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(finals.len(), 1, "restored Todo {phase:?} terminal receipt");
        assert!(
            matches!(finals[0], UiInputFinality::Committed(_)),
            "restored Todo {phase:?} did not commit: {:?}",
            finals[0]
        );
    }
    assert_eq!(storage.load().unwrap().tasks().len(), 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn persisted_full_page_mounts_bounded_ui_and_pagination_without_truncating_storage() {
    let path = std::env::temp_dir().join(format!(
        "runen-lab-todo-full-page-{}.json",
        std::process::id()
    ));
    assert!(!path.exists(), "isolated full-page fixture is required");
    let storage = TodoStorage::at(path.clone());
    let mut service = TodoService::at(storage.clone()).unwrap();
    for index in 0..=TASKS_PER_PAGE {
        service
            .execute(TodoCommand::Add {
                title: format!("Task {index}"),
            })
            .unwrap();
    }
    let state = TodoUiState::new(TodoService::at(storage.clone()).unwrap());
    assert_eq!(state.service.tasks().len(), TASKS_PER_PAGE + 1);
    assert_eq!(state.visible_tasks().len(), TASKS_PER_PAGE);
    let mut runtime = AppRuntime::<TodoUi>::mount(state)
        .expect("bounded initial page must mount without reducing persisted capacity");
    assert!(runtime.register_text_font_bytes(FONT.to_vec()).unwrap() > 0);
    runtime
        .set_text_generic_family_mapping(
            GenericFontFamily::SansSerif,
            &[FontFamilyName::new("Cantarell").unwrap()],
        )
        .unwrap();
    let style = runenui_core::StyleEnvironment::default();
    let publication = runtime
        .publish_surface(&SurfaceBuildContext::tight(
            &style,
            LogicalSize::try_new(800.0, 600.0).unwrap(),
        ))
        .unwrap();
    assert!(publication.frame().nodes().len() <= MAX_UI_NODES);
    for task in storage.load().unwrap().tasks().iter().take(TASKS_PER_PAGE) {
        let row_id = format!("task.{}.row", task.id().get());
        let task_row = publication
            .frame()
            .nodes()
            .iter()
            .find(|node| node.authored_id().is_some_and(|id| id.as_str() == row_id))
            .expect("styled task row must remain published");
        let rect = task_row.bounds();
        assert!(rect.width() > 0.0 && rect.height() > 0.0);
        assert!(rect.x() >= 0.0 && rect.y() >= 0.0);
        assert!(
            rect.x() + rect.width() <= 800.0 && rect.y() + rect.height() <= 600.0,
            "task row outside default surface: {row_id} {rect:?}"
        );
    }
    let next = publication
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|id| id.as_str() == "todo.page.next")
        })
        .expect("page navigation remains mounted and addressable");
    let bounds = next.bounds();
    assert!(
        bounds.width() > 0.0
            && bounds.height() > 0.0
            && bounds.x() >= 0.0
            && bounds.y() >= 0.0
            && bounds.x() + bounds.width() <= 800.0
            && bounds.y() + bounds.height() <= 600.0,
        "Next control must be visible inside the default Todo window: {bounds:?}"
    );
    assert_eq!(storage.load().unwrap().tasks().len(), TASKS_PER_PAGE + 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn full_page_next_pointer_activates_real_pagination_without_changing_storage() {
    use runenui_core::{
        InputDeviceId, LogicalPoint, PointerButton, PointerButtons, PointerDeviceKind,
        PointerEvent, PointerId, PointerPhase,
    };
    use runenui_runtime::{InputArbitrationRecord, PumpBudget, RuntimeStatus, UiInputFinality};

    let path = std::env::temp_dir().join(format!(
        "runen-lab-todo-page-pointer-{}.json",
        std::process::id()
    ));
    assert!(!path.exists(), "page interaction fixture must start empty");
    let storage = TodoStorage::at(path.clone());
    let mut service = TodoService::at(storage.clone()).unwrap();
    for index in 0..=TASKS_PER_PAGE {
        service
            .execute(TodoCommand::Add {
                title: format!("Task {index}"),
            })
            .unwrap();
    }
    let state = TodoUiState::new(TodoService::at(storage.clone()).unwrap());
    let mut runtime = AppRuntime::<TodoUi>::mount(state).unwrap();
    assert!(runtime.register_text_font_bytes(FONT.to_vec()).unwrap() > 0);
    runtime
        .set_text_generic_family_mapping(
            GenericFontFamily::SansSerif,
            &[FontFamilyName::new("Cantarell").unwrap()],
        )
        .unwrap();
    let style = runenui_core::StyleEnvironment::default();
    let context = SurfaceBuildContext::tight(&style, LogicalSize::try_new(800.0, 600.0).unwrap());
    let publication = runtime.publish_surface(&context).unwrap();
    let next = publication
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|id| id.as_str() == "todo.page.next")
        })
        .expect("the actual Next control is mounted");
    let bounds = next.bounds();
    let point = LogicalPoint::new(
        bounds.x() + bounds.width() / 2.0,
        bounds.y() + bounds.height() / 2.0,
    )
    .unwrap();
    let surface = publication.input_context().clone();
    let pointer = PointerId::new(2).unwrap();
    let device = InputDeviceId::new(11).unwrap();

    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        let mut event = PointerEvent::new(
            pointer,
            PointerDeviceKind::Mouse,
            phase,
            point,
            surface.clone(),
        )
        .with_device_id(device);
        if phase == PointerPhase::Down {
            event = event
                .with_buttons(PointerButtons::new([PointerButton::Primary]))
                .with_changed_button(PointerButton::Primary);
        } else if phase == PointerPhase::Up {
            event = event.with_changed_button(PointerButton::Primary);
        }
        let submitted = runtime
            .submit_pointer(event)
            .expect("ordinary Next mouse input is admitted");
        let batch = runtime
            .pump(PumpBudget::new(128, 128, 128, 128))
            .expect("ordinary Next mouse input is observed");
        assert_eq!(
            batch.final_ownership().status(),
            RuntimeStatus::Running,
            "Next {phase:?} must not poison the running runtime"
        );
        assert!(
            batch
                .processed_through()
                .is_some_and(|through| through >= submitted.sequence()),
            "Next {phase:?} must settle the original input sequence"
        );
        let finalities = batch
            .ordered_records()
            .iter()
            .filter_map(|record| match record {
                InputArbitrationRecord::InputSettled(receipt)
                    if receipt.sequence() == submitted.sequence() =>
                {
                    Some(receipt.finality())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(finalities.len(), 1, "Next {phase:?} terminal receipt");
        assert!(
            matches!(finalities[0], UiInputFinality::Committed(_)),
            "Next {phase:?} was not committed: {:?}",
            finalities[0]
        );
    }

    assert_eq!(runtime.state().page_index, 1);
    assert_eq!(runtime.state().visible_tasks().len(), 1);
    assert_eq!(
        runtime.state().visible_tasks()[0].title(),
        format!("Task {TASKS_PER_PAGE}")
    );
    let next_page = runtime.publish_surface(&context).unwrap();
    assert!(next_page.frame().nodes().iter().any(|node| {
        node.authored_id()
            .is_some_and(|id| id.as_str() == "todo.page.previous")
    }));
    assert_eq!(storage.load().unwrap().tasks().len(), TASKS_PER_PAGE + 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn native_smoke_click_targets() {
    // Test-only click coordinates come from the real public RunenUI surface
    // publication at the native app's default 800x600 viewport.
    // The native smoke must still dispatch original XTEST OS pointer input.
    let path = std::env::temp_dir().join(format!(
        "runen-lab-todo-native-targets-{}.json",
        std::process::id()
    ));
    assert!(!path.exists(), "native target fixture must be isolated");
    let storage = TodoStorage::at(path.clone());
    let mut service = TodoService::at(storage).unwrap();
    service
        .execute(TodoCommand::Add {
            title: "x11".into(),
        })
        .unwrap();
    let state = TodoUiState::new(service);
    // Mirror the native host's SystemAndBundled policy exactly. In contrast,
    // ordinary deterministic layout tests deliberately use an isolated font.
    // Do not manufacture click positions with a font profile the native
    // Release application does not use.
    let config = runenui_runtime::RuntimeConfig::default()
        .with_text_font_source_policy(runenui_runtime::FontSourcePolicy::SystemAndBundled);
    let mut runtime = AppRuntime::<TodoUi>::mount_with_config(state, config).unwrap();
    let style = runenui_core::StyleEnvironment::default();
    let publication = runtime
        .publish_surface(&SurfaceBuildContext::tight(
            &style,
            LogicalSize::try_new(800.0, 600.0).unwrap(),
        ))
        .unwrap();
    for (name, id) in [
        ("field", "todo.new-task"),
        ("add", "todo.add"),
        ("toggle", "task.1.toggle"),
        ("remove", "task.1.remove"),
    ] {
        let rect = publication
            .frame()
            .nodes()
            .iter()
            .find(|node| {
                node.authored_id()
                    .is_some_and(|authored| authored.as_str() == id)
            })
            .unwrap_or_else(|| panic!("native click target missing: {id}"))
            .bounds();
        assert!(
            rect.width() >= 12.0
                && rect.height() >= 12.0
                && rect.x() >= 0.0
                && rect.y() >= 0.0
                && rect.x() + rect.width() <= 800.0
                && rect.y() + rect.height() <= 600.0,
            "native click target outside viewport: {id} {rect:?}"
        );
        let x = (rect.x() + rect.width() / 2.0).round() as u32;
        let y = (rect.y() + rect.height() / 2.0).round() as u32;
        println!("TODO_NATIVE_TARGET {name} {x} {y}");
    }
    // Prove that the advertised native target also activates the real
    // task action through RunenUI's ordinary original pointer sequence.
    use runenui_core::{
        InputDeviceId, LogicalPoint, PointerButton, PointerButtons, PointerDeviceKind,
        PointerEvent, PointerId, PointerPhase,
    };
    use runenui_runtime::{InputArbitrationRecord, PumpBudget, RuntimeStatus, UiInputFinality};
    let toggle_bounds = publication
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|authored| authored.as_str() == "task.1.toggle")
        })
        .expect("real toggle remains mounted")
        .bounds();
    let point = LogicalPoint::new(
        toggle_bounds.x() + toggle_bounds.width() / 2.0,
        toggle_bounds.y() + toggle_bounds.height() / 2.0,
    )
    .unwrap();
    let context = publication.input_context().clone();
    let pointer = PointerId::new(9).unwrap();
    let device = InputDeviceId::new(11).unwrap();
    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        let mut event = PointerEvent::new(
            pointer,
            PointerDeviceKind::Mouse,
            phase,
            point,
            context.clone(),
        )
        .with_device_id(device);
        if phase == PointerPhase::Down {
            event = event
                .with_buttons(PointerButtons::new([PointerButton::Primary]))
                .with_changed_button(PointerButton::Primary);
        } else if phase == PointerPhase::Up {
            event = event.with_changed_button(PointerButton::Primary);
        }
        let submitted = runtime.submit_pointer(event).unwrap();
        let batch = runtime.pump(PumpBudget::new(128, 128, 128, 128)).unwrap();
        assert_eq!(batch.final_ownership().status(), RuntimeStatus::Running);
        assert!(
            batch
                .processed_through()
                .is_some_and(|through| through >= submitted.sequence())
        );
        let exact = batch
            .ordered_records()
            .iter()
            .filter_map(|record| match record {
                InputArbitrationRecord::InputSettled(receipt)
                    if receipt.sequence() == submitted.sequence() =>
                {
                    Some(receipt.finality())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            exact.len(),
            1,
            "native-target test original {phase:?} receipt"
        );
        assert!(matches!(exact[0], UiInputFinality::Committed(_)));
    }
    assert!(
        runtime.state().service.tasks()[0].completed(),
        "published toggle center must activate Complete"
    );
    assert!(TodoStorage::at(path.clone()).load().unwrap().tasks()[0].completed());
    // The Reopen label has a different width from Complete. Resolve its
    // actual control center from the completed-state native-font surface.
    let completed_publication = runtime
        .publish_surface(&SurfaceBuildContext::tight(
            &style,
            LogicalSize::try_new(800.0, 600.0).unwrap(),
        ))
        .unwrap();
    let reopen = completed_publication
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|authored| authored.as_str() == "task.1.toggle")
        })
        .expect("completed task publishes Reopen")
        .bounds();
    assert!(
        reopen.width() >= 12.0
            && reopen.height() >= 12.0
            && reopen.x() >= 0.0
            && reopen.y() >= 0.0
            && reopen.x() + reopen.width() <= 800.0
            && reopen.y() + reopen.height() <= 600.0,
        "Reopen target outside native viewport: {reopen:?}"
    );
    let reopen_x = (reopen.x() + reopen.width() / 2.0).round() as u32;
    let reopen_y = (reopen.y() + reopen.height() / 2.0).round() as u32;
    println!("TODO_NATIVE_TARGET reopen {reopen_x} {reopen_y}");
    // A persisted Reopen commit is not evidence that the UI's displayed
    // geometry has switched back from the completed group. Publish the
    // actual reopened state, and derive the next native Remove target there.
    // First prove the transition through ordinary M13 pointer finality.
    let reopen_point = LogicalPoint::new(
        reopen.x() + reopen.width() / 2.0,
        reopen.y() + reopen.height() / 2.0,
    )
    .unwrap();
    let reopened_context = completed_publication.input_context().clone();
    let reopened_pointer = PointerId::new(10).unwrap();
    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        let mut event = PointerEvent::new(
            reopened_pointer,
            PointerDeviceKind::Mouse,
            phase,
            reopen_point,
            reopened_context.clone(),
        )
        .with_device_id(device);
        if phase == PointerPhase::Down {
            event = event
                .with_buttons(PointerButtons::new([PointerButton::Primary]))
                .with_changed_button(PointerButton::Primary);
        } else if phase == PointerPhase::Up {
            event = event.with_changed_button(PointerButton::Primary);
        }
        let submitted = runtime.submit_pointer(event).unwrap();
        let batch = runtime.pump(PumpBudget::new(128, 128, 128, 128)).unwrap();
        assert_eq!(batch.final_ownership().status(), RuntimeStatus::Running);
        assert!(
            batch
                .processed_through()
                .is_some_and(|through| through >= submitted.sequence())
        );
        let exact = batch
            .ordered_records()
            .iter()
            .filter_map(|record| match record {
                InputArbitrationRecord::InputSettled(receipt)
                    if receipt.sequence() == submitted.sequence() =>
                {
                    Some(receipt.finality())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(exact.len(), 1, "native fixture Reopen {phase:?} receipt");
        assert!(matches!(exact[0], UiInputFinality::Committed(_)));
    }
    assert!(!runtime.state().service.tasks()[0].completed());
    let reopened_publication = runtime
        .publish_surface(&SurfaceBuildContext::tight(
            &style,
            LogicalSize::try_new(800.0, 600.0).unwrap(),
        ))
        .unwrap();
    let reopened_remove = reopened_publication
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|authored| authored.as_str() == "task.1.remove")
        })
        .expect("reopened task publishes Remove")
        .bounds();
    assert!(
        reopened_remove.width() >= 12.0
            && reopened_remove.height() >= 12.0
            && reopened_remove.x() >= 0.0
            && reopened_remove.y() >= 0.0
            && reopened_remove.x() + reopened_remove.width() <= 800.0
            && reopened_remove.y() + reopened_remove.height() <= 600.0,
        "reopened Remove target outside native viewport: {reopened_remove:?}"
    );
    let remove_x = (reopened_remove.x() + reopened_remove.width() / 2.0).round() as u32;
    let remove_y = (reopened_remove.y() + reopened_remove.height() / 2.0).round() as u32;
    println!("TODO_NATIVE_TARGET remove-reopened {remove_x} {remove_y}");

    // This is headless API-route proof. The smoke script still must deliver
    // its own *original* XTEST button click; never substitute this ingress.
    let remove_point = LogicalPoint::new(
        reopened_remove.x() + reopened_remove.width() / 2.0,
        reopened_remove.y() + reopened_remove.height() / 2.0,
    )
    .unwrap();
    let remove_context = reopened_publication.input_context().clone();
    let remove_pointer = PointerId::new(11).unwrap();
    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        let mut event = PointerEvent::new(
            remove_pointer,
            PointerDeviceKind::Mouse,
            phase,
            remove_point,
            remove_context.clone(),
        )
        .with_device_id(device);
        if phase == PointerPhase::Down {
            event = event
                .with_buttons(PointerButtons::new([PointerButton::Primary]))
                .with_changed_button(PointerButton::Primary);
        } else if phase == PointerPhase::Up {
            event = event.with_changed_button(PointerButton::Primary);
        }
        let submitted = runtime.submit_pointer(event).unwrap();
        let batch = runtime.pump(PumpBudget::new(128, 128, 128, 128)).unwrap();
        assert_eq!(batch.final_ownership().status(), RuntimeStatus::Running);
        assert!(
            batch
                .processed_through()
                .is_some_and(|through| through >= submitted.sequence())
        );
        let exact = batch
            .ordered_records()
            .iter()
            .filter_map(|record| match record {
                InputArbitrationRecord::InputSettled(receipt)
                    if receipt.sequence() == submitted.sequence() =>
                {
                    Some(receipt.finality())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(exact.len(), 1, "native fixture Remove {phase:?} receipt");
        assert!(matches!(exact[0], UiInputFinality::Committed(_)));
    }
    assert!(runtime.state().service.tasks().is_empty());
    assert!(
        TodoStorage::at(path.clone())
            .load()
            .unwrap()
            .tasks()
            .is_empty()
    );
    // Separate native-font publications for both pages prove that original
    // OS clicks have reachable targets after paging without relying on a
    // fixed pixel offset or an artificial title width.
    let paging_path = std::env::temp_dir().join(format!(
        "runen-lab-todo-native-pages-{}.json",
        std::process::id()
    ));
    assert!(
        !paging_path.exists(),
        "native paging fixture must be isolated"
    );
    let paging_storage = TodoStorage::at(paging_path.clone());
    let mut paging_service = TodoService::at(paging_storage.clone()).unwrap();
    for index in 1..=9 {
        paging_service
            .execute(TodoCommand::Add {
                title: format!("Bulk {index}"),
            })
            .unwrap();
    }
    for page_index in [0_usize, 1] {
        let mut page_state = TodoUiState::new(TodoService::at(paging_storage.clone()).unwrap());
        page_state.page_index = page_index;
        let config = runenui_runtime::RuntimeConfig::default()
            .with_text_font_source_policy(runenui_runtime::FontSourcePolicy::SystemAndBundled);
        let mut page_runtime = AppRuntime::<TodoUi>::mount_with_config(page_state, config).unwrap();
        let page_publication = page_runtime
            .publish_surface(&SurfaceBuildContext::tight(
                &style,
                LogicalSize::try_new(800.0, 600.0).unwrap(),
            ))
            .unwrap();
        let last_toggle = String::from("task.9.toggle");
        let targets = if page_index == 0 {
            [
                ("page-next", "todo.page.next".to_owned()),
                ("page-first", "task.1.toggle".to_owned()),
            ]
        } else {
            [
                ("page-previous", "todo.page.previous".to_owned()),
                ("page-last", last_toggle),
            ]
        };
        for (name, id) in targets {
            let rect = page_publication
                .frame()
                .nodes()
                .iter()
                .find(|node| {
                    node.authored_id()
                        .is_some_and(|authored| authored.as_str() == id.as_str())
                })
                .unwrap_or_else(|| panic!("paged native target missing: {id}"))
                .bounds();
            assert!(
                rect.width() >= 12.0
                    && rect.height() >= 12.0
                    && rect.x() >= 0.0
                    && rect.y() >= 0.0
                    && rect.x() + rect.width() <= 800.0
                    && rect.y() + rect.height() <= 600.0,
                "paged native target outside viewport: {id} {rect:?}"
            );
            let x = (rect.x() + rect.width() / 2.0).round() as u32;
            let y = (rect.y() + rect.height() / 2.0).round() as u32;
            println!("TODO_NATIVE_TARGET {name} {x} {y}");
        }
    }
    std::fs::remove_file(paging_path).unwrap();
    std::fs::remove_file(path).unwrap();
}
