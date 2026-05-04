use crate::app::state::FileApp;
use crate::types::*;
use cosmic::iced::theme::Base;
use cosmic::iced::{Alignment, Length, Padding};
use cosmic::theme::Button as ButtonClass;
use cosmic::widget::{button, column, container, icon, row, text};
use cosmic::{Element, Theme};
use rust_i18n::t;
use std::path::PathBuf;

fn render_tree_node<'a>(
    app: &'a FileApp,
    path: &PathBuf,
    best_match: Option<&PathBuf>,
) -> Element<'a, Message> {
    let mut col = column![].spacing(2);
    if app.ui.expanded_tree_nodes.contains(path) {
        if let Some(children) = app.ui.tree_cache.get(path) {
            let mut children_col = column![].spacing(2);
            for child in children {
                if child.is_dir {
                    let is_expanded = app.ui.expanded_tree_nodes.contains(&child.path);
                    let is_active = best_match == Some(&child.path);
                    let toggle_icon = cosmic::widget::mouse_area(
                        icon::from_name(if is_expanded {
                            "go-down-symbolic"
                        } else {
                            "go-next-symbolic"
                        })
                        .size(16),
                    )
                    .on_press(Message::UI(UIMsg::ToggleSidebarNode(child.path.clone())));
                    let name_str = super::truncate_text(&child.name, 25);
                    let btn = button::custom(
                        row![
                            toggle_icon,
                            icon::from_name(child.icon_name).size(16),
                            text(name_str).size(14.0).width(Length::Fill)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .class(if is_active {
                        ButtonClass::Suggested
                    } else {
                        ButtonClass::Text
                    })
                    .on_press(Message::Nav(NavMsg::Navigate(child.path.clone())))
                    .width(Length::Fill)
                    .padding(Padding::from([4, 8]));
                    children_col = children_col.push(btn);
                    if is_expanded {
                        children_col =
                            children_col.push(render_tree_node(app, &child.path, best_match));
                    }
                }
            }
            let tree_guide = container(children_col)
                .padding(Padding::from([0, 0, 0, 10]))
                .style(|theme: &Theme| {
                    let mut bc = theme
                        .palette()
                        .map(|p| p.text)
                        .unwrap_or(cosmic::iced::Color::WHITE);
                    bc.a = 0.15;
                    container::Style {
                        border: cosmic::iced::Border {
                            color: bc,
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    }
                });
            col = col.push(row![
                cosmic::iced::widget::Space::new().width(Length::Fixed(20.0)),
                tree_guide
            ]);
        } else {
            col = col.push(row![
                cosmic::iced::widget::Space::new().width(Length::Fixed(20.0)),
                container(text(t!("Loading...").to_string()).size(13.0))
                    .padding(Padding::from([4, 8, 4, 10]))
            ]);
        }
    }
    col.into()
}

pub fn render<'a>(app: &'a FileApp, best_match_path: Option<PathBuf>) -> Element<'a, Message> {
    if !app.ui.sidebar_visible {
        return container(cosmic::iced::widget::Space::new())
            .width(Length::Fixed(0.0))
            .into();
    }
    let mut sidebar_col = column![].spacing(4);
    sidebar_col = sidebar_col.push(
        container(text(t!("Locations").to_string()).size(14.0))
            .padding(Padding::from([16, 8, 8, 16]))
            .width(Length::Fill),
    );

    for item in &app.ui.sidebar_items {
        let is_expanded = app.ui.expanded_tree_nodes.contains(&item.path);
        let is_active = best_match_path.as_ref() == Some(&item.path);
        let toggle_icon = cosmic::widget::mouse_area(
            icon::from_name(if is_expanded {
                "go-down-symbolic"
            } else {
                "go-next-symbolic"
            })
            .size(16),
        )
        .on_press(Message::UI(UIMsg::ToggleSidebarNode(item.path.clone())));
        let btn = button::custom(
            row![
                toggle_icon,
                icon::from_name(item.icon).size(18),
                text(super::truncate_text(&item.name, 25))
                    .size(14.0)
                    .width(Length::Fill)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .class(if is_active {
            ButtonClass::Suggested
        } else {
            ButtonClass::Text
        })
        .on_press(Message::Nav(NavMsg::Navigate(item.path.clone())))
        .width(Length::Fill)
        .padding(Padding::from([6, 12, 6, 8]));
        sidebar_col = sidebar_col.push(btn);
        if is_expanded {
            sidebar_col =
                sidebar_col.push(render_tree_node(app, &item.path, best_match_path.as_ref()));
        }
    }

    if !app.fs.system_disks.is_empty() {
        sidebar_col =
            sidebar_col.push(cosmic::iced::widget::Space::new().height(Length::Fixed(16.0)));
        sidebar_col = sidebar_col.push(
            container(text(t!("Devices").to_string()).size(14.0))
                .padding(Padding::from([8, 8, 8, 16]))
                .width(Length::Fill),
        );
        for disk in &app.fs.system_disks {
            let is_active = best_match_path.as_ref() == Some(&disk.mount_point);
            let btn = button::custom(
                row![
                    cosmic::iced::widget::Space::new().width(Length::Fixed(24.0)),
                    icon::from_name("drive-harddisk-symbolic").size(18),
                    text(super::truncate_text(&disk.name, 25))
                        .size(14.0)
                        .width(Length::Fill)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .class(if is_active {
                ButtonClass::Suggested
            } else {
                ButtonClass::Text
            })
            .on_press(Message::Nav(NavMsg::Navigate(disk.mount_point.clone())))
            .width(Length::Fill)
            .padding(Padding::from([6, 12, 6, 8]));
            sidebar_col = sidebar_col.push(btn);
        }
    }

    let scroller =
        cosmic::iced::widget::scrollable(container(sidebar_col).padding(Padding::from([
            UI_SIDEBAR_PADDING_TOP as u16,
            8,
            16,
            8,
        ])))
        .height(Length::Fill);
    container(scroller)
        .style(|theme: &Theme| {
            let bg = theme
                .palette()
                .map(|p| p.background)
                .unwrap_or(cosmic::iced::Color::from_rgb(0.1, 0.1, 0.1));
            let mut darker_bg = bg;
            darker_bg.r *= 0.95;
            darker_bg.g *= 0.95;
            darker_bg.b *= 0.95;
            container::Style {
                background: Some(cosmic::iced::Background::Color(darker_bg)),
                border: cosmic::iced::Border {
                    color: cosmic::iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        })
        .width(Length::Fixed(UI_SIDEBAR_WIDTH))
        .height(Length::Fill)
        .into()
}
