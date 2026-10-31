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
    let mut runtime = AppRuntime::<TodoUi>::mount(state);
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
