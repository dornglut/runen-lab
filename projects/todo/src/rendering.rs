use engine::plugins::render::RenderFlow;
use engine::prelude::*;

const BACKGROUND_COLOR: [f32; 4] = [0.06, 0.07, 0.09, 1.0];

pub fn configure(app: &mut App) -> anyhow::Result<()> {
    app.add_render_flow(build_flow()?);
    Ok(())
}

fn build_flow() -> anyhow::Result<RenderFlow> {
    let flow = RenderFlow::new("todo.main").with_surface_color()?;

    let flow = add_background(flow)?;
    let flow = add_ui_composite(flow)?;
    let flow = add_presentation(flow)?;

    flow.validate()
}

fn add_background(flow: RenderFlow) -> anyhow::Result<RenderFlow> {
    Ok(flow
        .fullscreen_pass("todo.clear")
        .main_surface_only()
        .clear_color(BACKGROUND_COLOR)
        .write_surface_color()?
        .finish())
}

fn add_ui_composite(flow: RenderFlow) -> anyhow::Result<RenderFlow> {
    Ok(flow
        .builtin_ui_composite_pass("todo.ui")?
        .main_surface_only()
        .finish())
}

fn add_presentation(flow: RenderFlow) -> anyhow::Result<RenderFlow> {
    Ok(flow
        .present_pass("todo.present")?
        .main_surface_only()
        .surface_color()?
        .finish())
}
