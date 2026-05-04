pub mod bottom_bar;
pub mod file_list;
pub mod header;
pub mod modals;
pub mod sidebar;
pub mod status_bar;
pub mod topbar;

use crate::app::state::FileApp;
use crate::types::*;
use cosmic::iced::theme::Base;
use cosmic::iced::widget::Stack;
use cosmic::iced::widget::{Space, column, container, row};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{icon, text};
use cosmic::{Element, Theme};

impl FileApp {
    pub fn active_color_style(theme: &Theme, bg_alpha: f32) -> container::Style {
        let primary = theme
            .palette()
            .map(|p| p.primary)
            .unwrap_or(cosmic::iced::Color::from_rgb(0.2, 0.5, 0.8));
        let mut bg = primary;
        bg.a = bg_alpha;
        container::Style {
            background: Some(cosmic::iced::Background::Color(bg)),
            border: cosmic::iced::Border {
                color: if bg_alpha > 0.3 {
                    cosmic::iced::Color::TRANSPARENT
                } else {
                    primary
                },
                width: 0.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        }
    }

    pub fn popover_style(theme: &Theme) -> container::Style {
        let bg = theme
            .palette()
            .map(|p| p.background)
            .unwrap_or(cosmic::iced::Color::from_rgb(0.12, 0.12, 0.12));
        container::Style {
            background: Some(cosmic::iced::Background::Color(bg)),
            border: cosmic::iced::Border {
                color: cosmic::iced::Color::from_rgba(0.5, 0.5, 0.5, 0.2),
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        }
    }

    pub fn view_logic(&self) -> Element<'_, Message> {
        let mut max_len = 0;
        let mut best_match_path: Option<std::path::PathBuf> = None;
        for item in &self.ui.sidebar_items {
            if self.fs.current_dir.starts_with(&item.path) {
                let len = item.path.as_os_str().len();
                if len > max_len {
                    max_len = len;
                    best_match_path = Some(item.path.clone());
                }
            }
        }
        for disk in &self.fs.system_disks {
            if self.fs.current_dir.starts_with(&disk.mount_point) {
                let len = disk.mount_point.as_os_str().len();
                if len > max_len {
                    max_len = len;
                    best_match_path = Some(disk.mount_point.clone());
                }
            }
        }

        let sidebar = sidebar::render(self, best_match_path.clone());
        let top_bar = topbar::render(self);
        let header = header::render(self);
        let list = file_list::render(self);
        let status = status_bar::render(self);
        let bottom = bottom_bar::render(self);

        let main_area = column![top_bar, header, list, container(status), bottom]
            .width(Length::Fill)
            .spacing(0);
        let base_ui: Element<'_, Message> = row![sidebar, main_area]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let mut app_stack = Stack::new().push(base_ui);

        if self.ui.is_dragging_marquee {
            if let Some(start) = self.ui.selection_start {
                let x = start.x.min(self.ui.current_mouse.x);
                let y = start.y.min(self.ui.current_mouse.y);
                let w = (start.x - self.ui.current_mouse.x).abs();
                let h = (start.y - self.ui.current_mouse.y).abs();
                let selection_box = container(Space::new())
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .style(|theme: &Theme| Self::active_color_style(theme, 0.15));
                let overlay: Element<'_, Message> = container(column![
                    Space::new().height(Length::Fixed(y)),
                    row![Space::new().width(Length::Fixed(x)), selection_box]
                ])
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Start)
                .align_y(Alignment::Start)
                .into();
                app_stack = app_stack.push(overlay);
            }
        } else if self.ui.is_dragging_items && self.ui.item_drag_start.is_some() {
            let count = self.fs.selected_files.len();
            let drag_badge = container(
                row![
                    icon::from_name("edit-copy-symbolic"),
                    text(format!("Moving {} item(s)", count))
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .style(Self::popover_style)
            .padding(12);
            let overlay: Element<'_, Message> = container(column![
                Space::new().height(Length::Fixed(self.ui.current_mouse.y - 20.0)),
                row![
                    Space::new().width(Length::Fixed(self.ui.current_mouse.x + 10.0)),
                    drag_badge
                ]
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Start)
            .align_y(Alignment::Start)
            .into();
            app_stack = app_stack.push(overlay);
        }
        if let Some(modal) = modals::render(self) {
            app_stack = app_stack.push(modal);
        }
        app_stack.into()
    }
}

pub fn truncate_text(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count > max {
        let available = max.saturating_sub(3);
        if available == 0 {
            return "...".to_string();
        }
        let keep_front = (available / 2) + (available % 2);
        let keep_back = available / 2;
        let first: String = s.chars().take(keep_front).collect();
        let last: String = s.chars().skip(count - keep_back).collect();
        format!("{}...{}", first, last)
    } else {
        s.to_string()
    }
}
