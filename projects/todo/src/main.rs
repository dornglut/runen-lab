mod application;
mod model;
mod storage;
mod rendering;
mod ui;
mod window;

fn main() -> anyhow::Result<()> {
    application::run()
}
