use engine::prelude::*;

pub fn handle_close(mut windows: ResMut<WindowStateRegistryResource>) {
    if let Some(id) = windows.primary_window_id()
        && let Some(window) = windows.record_mut(id)
        && window.close_intent_pending
    {
        window.approve_close();
    }
}
