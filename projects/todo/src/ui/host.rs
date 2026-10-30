use engine::plugins::render::{RenderFrameProducerId, host::RenderSurfaceId};
use engine::plugins::ui::{
    UiRuntimeFontConfiguration, UiRuntimePresentationAssociationsResource,
    UiRuntimePresentationBinding, UiTypedScreenId,
};
use engine::prelude::*;
use runenui_core::{FontFamilyName, GenericFontFamily, StyleEnvironment};

use super::{TodoUi, TodoUiState};

pub fn install(app: &mut App) -> anyhow::Result<()> {
    let font_bytes = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?;

    let fonts = UiRuntimeFontConfiguration::new(vec![font_bytes]).with_generic_mapping(
        GenericFontFamily::SansSerif,
        vec![FontFamilyName::new("Segoe UI")?],
    );

    let state = TodoUiState::load()?;
    let mount = app
        .ui()
        .mount_app::<TodoUi>(UiTypedScreenId::new("todo.main"), state, &fonts);

    let slot = mount
        .slot_id()
        .ok_or_else(|| anyhow::anyhow!("Todo UI mount failed: {mount:?}"))?;

    let binding = UiRuntimePresentationBinding::new(
        slot,
        RenderFrameProducerId::try_from_raw(1).expect("producer ID must be nonzero"),
        RenderSurfaceId::primary(),
        StyleEnvironment::default(),
    );

    app.world_mut()
        .resource_mut::<UiRuntimePresentationAssociationsResource>()?
        .bind(binding)?;

    Ok(())
}
