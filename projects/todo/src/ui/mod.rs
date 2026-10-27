mod host;

use runenui_core::{Effects, IntoUpdateOutput, NoHostProtocol, UiApp, View, column, text};

pub use host::install;

pub struct TodoUi;

impl UiApp for TodoUi {
    type State = ();
    type Action = ();
    type HostProtocol = NoHostProtocol;

    fn root(_: &Self::State) -> impl View<Self::Action> {
        column([text("Todo"), text("No tasks yet.")])
            .foreground(runenui_core::Color::rgb(238, 240, 246))
    }

    fn update(
        _: &mut Self::State,
        _: Self::Action,
    ) -> impl IntoUpdateOutput<Self::Action, Self::HostProtocol> {
        Effects::none()
    }
}
