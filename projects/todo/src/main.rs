mod application;
mod rendering;
mod ui;
mod window;

fn main() -> anyhow::Result<()> {
    application::run()
}
