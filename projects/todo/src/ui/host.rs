use engine::plugins::ui::{UiRuntimeFontConfiguration, UiTypedScreenId};
use engine::prelude::*;

use super::{TodoUi, TodoUiState};

pub fn install(app: &mut App) -> anyhow::Result<()> {
    // Accepted Runenwerk font policy delegates discovery to RunenUI.
    // No OS font file path or unlicensed binary is embedded in the app.
    let fonts = UiRuntimeFontConfiguration::default().with_system_fonts();

    let state = TodoUiState::load()?;
    let slot = app.ui().mount_app_on_primary::<TodoUi>(
        UiTypedScreenId::new("todo.main"),
        state,
        &fonts,
    )?;

    // Native input selection is independent of UI mounting and presentation.
    app.ui().bind_native_primary_input(slot)?;

    Ok(())
}
