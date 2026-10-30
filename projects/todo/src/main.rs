mod application;
mod model;
mod rendering;
mod service;
mod storage;
mod ui;
mod window;

fn main() -> anyhow::Result<()> {
    application::run()
}
