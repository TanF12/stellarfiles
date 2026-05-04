use crate::app::state::FileApp;
use crate::types::*;
use cosmic::Element;
use cosmic::iced::{Alignment, Length, Padding};
use cosmic::theme::Button as ButtonClass;
use cosmic::widget::{Space, button, container, icon, row, text};
use rust_i18n::t;

pub fn render(app: &FileApp) -> Element<'_, Message> {
    if app.ui.view_mode == ViewMode::Grid {
        return Space::new().height(Length::Fixed(0.0)).into();
    }
    let sort_icon = if app.ui.sort_direction == SortDirection::Asc {
        "view-sort-ascending-symbolic"
    } else {
        "view-sort-descending-symbolic"
    };
    let name_icon: Element<'_, Message> = if app.ui.sort_key == SortKey::Name {
        icon::from_name(sort_icon).size(16).into()
    } else {
        Space::new().width(Length::Fixed(16.0)).into()
    };
    let date_icon: Element<'_, Message> = if app.ui.sort_key == SortKey::Modified {
        icon::from_name(sort_icon).size(16).into()
    } else {
        Space::new().width(Length::Fixed(16.0)).into()
    };
    let size_icon: Element<'_, Message> = if app.ui.sort_key == SortKey::Size {
        icon::from_name(sort_icon).size(16).into()
    } else {
        Space::new().width(Length::Fixed(16.0)).into()
    };

    let header = row![
        Space::new().width(Length::Fixed(24.0)),
        button::custom(
            row![text(t!("Name")).size(13.0), name_icon]
                .spacing(4)
                .align_y(Alignment::Center)
        )
        .class(ButtonClass::Text)
        .on_press(Message::UI(UIMsg::SortBy(SortKey::Name)))
        .width(Length::Fill),
        button::custom(
            row![text(t!("Type")).size(13.0)]
                .spacing(4)
                .align_y(Alignment::Center)
        )
        .class(ButtonClass::Text)
        .width(Length::Fixed(120.0)),
        button::custom(
            row![text(t!("Modified")).size(13.0), date_icon]
                .spacing(4)
                .align_y(Alignment::Center)
        )
        .class(ButtonClass::Text)
        .on_press(Message::UI(UIMsg::SortBy(SortKey::Modified)))
        .width(Length::Fixed(150.0)),
        button::custom(
            row![text(t!("Size")).size(13.0), size_icon]
                .spacing(4)
                .align_y(Alignment::Center)
        )
        .class(ButtonClass::Text)
        .on_press(Message::UI(UIMsg::SortBy(SortKey::Size)))
        .width(Length::Fixed(100.0)),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .padding(Padding::from([0, 16]));

    container(header)
        .height(Length::Fixed(UI_HEADER_HEIGHT))
        .width(Length::Fill)
        .into()
}
