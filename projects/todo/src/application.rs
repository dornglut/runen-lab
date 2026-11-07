use engine::plugins::{RenderPlugin, UiPlugin};
use engine::prelude::*;

use crate::{rendering, ui, window};

pub fn run() -> anyhow::Result<()> {
    let mut app = App::new();

    app.set_title("Runen Lab - Todo");
    app.add_plugins(default_plugins());
    app.add_plugin(RenderPlugin);
    app.add_plugin(UiPlugin);

    app.add_systems(Update, window::handle_close);

    rendering::configure(&mut app)?;
    ui::install(&mut app)?;

    app.run()
}
