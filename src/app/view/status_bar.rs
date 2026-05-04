use crate::app::state::FileApp;
use crate::math::format_bytes;
use crate::types::*;
use cosmic::iced::{Alignment, Length, Padding};
use cosmic::theme::Button as ButtonClass;
use cosmic::widget::{Space, button, column, container as iced_container, icon, row, text};
use cosmic::{Element, Theme};
use rust_i18n::t;

pub fn render(app: &FileApp) -> Element<'_, Message> {
    if !app.tasks.active_tasks.is_empty() {
        let mut tasks_col = column![].spacing(4);
        for (id, task) in &app.tasks.active_tasks {
            let pct = if task.total_bytes == 0 {
                0.0
            } else {
                (task.current_bytes as f32 / task.total_bytes as f32).clamp(0.0, 1.0)
            };
            let fill_w = 200.0 * pct;
            let bar_w = if task.total_bytes == 0 { 200.0 } else { fill_w };
            let bg_col = if task.total_bytes == 0 { 0.8 } else { 0.2 };
            let bar = row![
                iced_container(Space::new())
                    .width(Length::Fixed(bar_w))
                    .height(Length::Fixed(4.0))
                    .style(|t: &Theme| FileApp::active_color_style(t, 1.0)),
                iced_container(Space::new())
                    .width(Length::Fixed(200.0 - bar_w))
                    .height(Length::Fixed(4.0))
                    .style(move |t: &Theme| FileApp::active_color_style(t, bg_col)),
            ];
            let count_txt = if task.total_bytes == 0 {
                format_bytes(task.current_bytes)
            } else {
                format!(
                    "{} / {}",
                    format_bytes(task.current_bytes),
                    format_bytes(task.total_bytes)
                )
            };
            let cancel_btn = button::custom(icon::from_name("process-stop-symbolic").size(16))
                .class(ButtonClass::Destructive)
                .on_press(Message::Task(TaskMsg::CancelTask(*id)));
            tasks_col = tasks_col.push(
                iced_container(
                    row![
                        text(format!("{}...", t!(&task.title)))
                            .width(Length::Fixed(110.0))
                            .size(13.0),
                        bar,
                        Space::new().width(Length::Fixed(12.0)),
                        text(count_txt).width(Length::Fixed(150.0)).size(13.0),
                        text(format!("| {}", task.active_file))
                            .width(Length::Fill)
                            .size(13.0),
                        cancel_btn
                    ]
                    .align_y(Alignment::Center),
                )
                .padding(4)
                .width(Length::Fill),
            );
        }
        iced_container(tasks_col)
            .padding(8)
            .width(Length::Fill)
            .into()
    } else if let Some(state) = &app.tasks.delete_state {
        let pct = if state.total == 0 {
            0.0
        } else {
            state.completed as f32 / state.total as f32
        };
        let fill_w = 200.0 * pct;
        let bar = row![
            iced_container(Space::new())
                .width(Length::Fixed(fill_w))
                .height(Length::Fixed(4.0))
                .style(|t: &Theme| FileApp::active_color_style(t, 1.0)),
            iced_container(Space::new())
                .width(Length::Fixed(200.0 - fill_w))
                .height(Length::Fixed(4.0))
                .style(|t: &Theme| FileApp::active_color_style(t, 0.2))
        ];
        iced_container(
            row![
                text(if state.is_permanent {
                    t!("Deleting...")
                } else {
                    t!("Moving to Trash...")
                })
                .width(Length::Fixed(180.0))
                .size(13.0),
                bar,
                Space::new().width(Length::Fixed(12.0)),
                text(format!("{} / {}", state.completed, state.total)).size(13.0)
            ]
            .align_y(Alignment::Center),
        )
        .padding(8)
        .width(Length::Fill)
        .into()
    } else {
        iced_container(text(t!(&app.ui.status_msg)).size(13.0))
            .padding(Padding::from([4, 12]))
            .width(Length::Fill)
            .into()
    }
}
