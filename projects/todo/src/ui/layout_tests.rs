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
