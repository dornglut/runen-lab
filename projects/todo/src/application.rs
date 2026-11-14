use engine::plugins::{RenderPlugin, UiPlugin};
use engine::prelude::*;

use crate::{rendering, ui, window};

const FRAME_PACING_ENV: &str = "RUNEN_LAB_TODO_FRAME_PACING";

// This is an opt-in downstream experiment; the ordinary Todo launch retains
// Runenwerk's accepted default policy without changing engine scheduler semantics.
fn selected_pacing(value: Option<&str>) -> anyhow::Result<Option<FramePacingPolicyResource>> {
    match value {
        None | Some("continuous_capped_60") => Ok(None),
        Some("on_demand") => Ok(Some(FramePacingPolicyResource::on_demand())),
        Some(other) => anyhow::bail!(
            "{FRAME_PACING_ENV}={other:?} is unsupported; use continuous_capped_60 or on_demand"
        ),
    }
}

fn pacing_from_environment() -> anyhow::Result<Option<FramePacingPolicyResource>> {
    let Some(value) = std::env::var_os(FRAME_PACING_ENV) else {
        return selected_pacing(None);
    };
    let value = value
        .into_string()
        .map_err(|_| anyhow::anyhow!("{FRAME_PACING_ENV} must be valid Unicode"))?;
    selected_pacing(Some(&value))
}

pub fn run() -> anyhow::Result<()> {
    let pacing = pacing_from_environment()?;
    let mut app = App::new();

    app.set_title("Runen Lab - Todo");
    if let Some(policy) = pacing {
        app.with_frame_pacing(policy);
    }
    app.add_plugins(default_plugins());
    app.add_plugin(RenderPlugin);
    app.add_plugin(UiPlugin);

    app.add_systems(Update, window::handle_close);

    rendering::configure(&mut app)?;
    ui::install(&mut app)?;

    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_explicit_control_keep_the_accepted_host_policy() {
        assert_eq!(selected_pacing(None).unwrap(), None);
        assert_eq!(selected_pacing(Some("continuous_capped_60")).unwrap(), None);
    }

    #[test]
    fn experiment_selects_only_the_accepted_on_demand_policy() {
        assert_eq!(
            selected_pacing(Some("on_demand")).unwrap(),
            Some(FramePacingPolicyResource::on_demand())
        );
    }

    #[test]
    fn invalid_pacing_configuration_fails_instead_of_silently_changing_policy() {
        for invalid in ["", "ON_DEMAND", "continuous_uncapped", "999"] {
            assert!(selected_pacing(Some(invalid)).is_err());
        }
    }
}
