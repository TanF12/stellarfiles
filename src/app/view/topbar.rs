use crate::app::state::FileApp;
use crate::types::*;
use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::theme::Button as ButtonClass;
use cosmic::widget::{button, container, icon, row, text, text_input};
use rust_i18n::t;

pub fn render(app: &FileApp) -> Element<'_, Message> {
    let mut top_actions = row![];
    if app.ui.mode == Mode::Manager {
        top_actions = top_actions.push(
            button::custom(icon::from_name("navbar-open-symbolic"))
                .class(ButtonClass::Standard)
                .on_press(Message::UI(UIMsg::ToggleSidebar)),
        );
    }

    top_actions = top_actions
        .push(
            button::custom(icon::from_name("go-previous-symbolic"))
                .class(ButtonClass::Standard)
                .on_press(Message::Nav(NavMsg::NavigateBack)),
        )
        .push(
            button::custom(icon::from_name("go-next-symbolic"))
                .class(ButtonClass::Standard)
                .on_press(Message::Nav(NavMsg::NavigateForward)),
        )
        .push(
            button::custom(icon::from_name("go-up-symbolic"))
                .class(ButtonClass::Standard)
                .on_press(Message::Nav(NavMsg::NavigateUp)),
        )
        .push(
            text_input("Path", &app.ui.path_input)
                .on_input(|s| Message::Nav(NavMsg::PathInputChanged(s)))
                .on_submit(|s| Message::Nav(NavMsg::NavigateToInput(s)))
                .width(Length::Fill),
        );

    let view_icon = if app.ui.view_mode == ViewMode::List {
        "view-grid-symbolic"
    } else {
        "view-list-symbolic"
    };
    top_actions = top_actions.push(
        button::custom(icon::from_name(view_icon))
            .class(ButtonClass::Standard)
            .on_press(Message::UI(UIMsg::ToggleViewMode)),
    );

    let hidden_icon = if app.ui.show_hidden {
        "image-red-eye-symbolic"
    } else {
        "document-properties-symbolic"
    };
    top_actions = top_actions.push(
        button::custom(
            row![icon::from_name(hidden_icon)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .class(ButtonClass::Standard)
        .on_press(Message::UI(UIMsg::ToggleHiddenFiles)),
    );

    if app.ui.search_visible {
        top_actions = top_actions
            .push(
                button::custom(text(if app.ui.search_regex { "[.*]" } else { ".*" }))
                    .class(if app.ui.search_regex {
                        ButtonClass::Suggested
                    } else {
                        ButtonClass::Standard
                    })
                    .on_press(Message::UI(UIMsg::ToggleRegex)),
            )
            .push(
                button::custom(text(if app.ui.search_deep { "[Deep]" } else { "Deep" }))
                    .class(if app.ui.search_deep {
                        ButtonClass::Suggested
                    } else {
                        ButtonClass::Standard
                    })
                    .on_press(Message::UI(UIMsg::ToggleDeepSearch)),
            )
            .push(
                text_input(t!("Search..."), &app.ui.search_query)
                    .on_input(|s| Message::UI(UIMsg::SearchChanged(s)))
                    .width(Length::Fixed(200.0)),
            )
            .push(
                button::custom(icon::from_name("window-close-symbolic"))
                    .class(ButtonClass::Standard)
                    .on_press(Message::UI(UIMsg::CloseSearch)),
            );
    } else {
        top_actions = top_actions.push(
            button::custom(icon::from_name("system-search-symbolic"))
                .class(ButtonClass::Standard)
                .on_press(Message::UI(UIMsg::ToggleSearch)),
        );
    }

    if app.is_trash_dir(&app.fs.current_dir) {
        top_actions = top_actions.push(
            button::custom(row![text(t!("Empty Trash"))].spacing(8))
                .class(ButtonClass::Destructive)
                .on_press(Message::File(FileMsg::EmptyTrash)),
        );
    }
    container(top_actions.spacing(8).align_y(Alignment::Center))
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fixed(UI_TOP_BAR_HEIGHT))
        .into()
}
