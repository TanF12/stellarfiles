use crate::app::state::FileApp;
use crate::types::*;
use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::theme::Button as ButtonClass;
use cosmic::widget::{Space, button, container, row, text, text_input};
use rust_i18n::t;

pub fn render(app: &FileApp) -> Element<'_, Message> {
    if app.ui.mode == Mode::Manager {
        return container(Space::new()).height(Length::Fixed(0.0)).into();
    }
    let mut bot_row = row![].spacing(10).align_y(Alignment::Center);
    if let Mode::Save(_) = app.ui.mode {
        bot_row = bot_row.push(
            text_input("Filename", &app.ui.save_dialog_input)
                .on_input(|s| Message::UI(UIMsg::DialogSaveNameChanged(s)))
                .width(Length::Fill),
        );
    } else {
        bot_row = bot_row.push(Space::new().width(Length::Fill));
    }
    bot_row = bot_row.push(
        button::custom(
            text(t!("Cancel"))
                .width(Length::Fill)
                .align_x(Alignment::Center),
        )
        .class(ButtonClass::Standard)
        .on_press(Message::UI(UIMsg::DialogCancel))
        .width(Length::FillPortion(1)),
    );
    bot_row = bot_row.push(
        button::custom(
            text(if matches!(app.ui.mode, Mode::Save(_)) {
                t!("Save")
            } else {
                t!("Open")
            })
            .width(Length::Fill)
            .align_x(Alignment::Center),
        )
        .class(ButtonClass::Suggested)
        .on_press(Message::UI(UIMsg::DialogConfirm))
        .width(Length::FillPortion(1)),
    );
    container(bot_row).padding(12).width(Length::Fill).into()
}
