use engine::plugins::render::{RenderFrameProducerId, host::RenderSurfaceId};
use engine::plugins::ui::{
    UiRuntimeFontConfiguration, UiRuntimePresentationAssociationsResource,
    UiRuntimePresentationBinding, UiTypedScreenId,
};
use engine::prelude::*;
use runenui_core::StyleEnvironment;

use super::{TodoUi, TodoUiState};

pub fn install(app: &mut App) -> anyhow::Result<()> {
    // Accepted Runenwerk font policy delegates discovery to RunenUI.
    // No OS font file path or unlicensed binary is embedded in the app.
    let fonts = UiRuntimeFontConfiguration::default().with_system_fonts();

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
