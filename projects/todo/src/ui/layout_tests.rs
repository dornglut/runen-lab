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
            .find(|node| node.placeholder() == Some("New task"))
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
fn actual_todo_text_focus_edit_and_add_preserve_m13_and_persistence() {
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
        .find(|node| node.placeholder() == Some("New task"))
        .expect("semantic editable survives committed text");
    assert_eq!(
        semantic_field
            .editable()
            .and_then(|editable| editable.value()),
        Some("x"),
        "paired committed text updates Todo exactly once"
    );

    let add = edited
        .frame()
        .nodes()
        .iter()
        .find(|node| {
            node.authored_id()
                .is_some_and(|id| id.as_str() == "todo.add")
        })
        .expect("real Add button after edited text")
        .bounds();
    let add_point =
        LogicalPoint::new(add.x() + add.width() / 2.0, add.y() + add.height() / 2.0).unwrap();
    click(&mut runtime, edited.input_context(), add_point, device);

    let saved = storage
        .load()
        .expect("Todo persisted after original typed input");
    assert_eq!(saved.tasks().len(), 1, "exactly one task was added");
    assert_eq!(saved.tasks()[0].title(), "x");
    assert!(!saved.tasks()[0].completed());
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
    let mut runtime = AppRuntime::<TodoUi>::mount(restored)
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
