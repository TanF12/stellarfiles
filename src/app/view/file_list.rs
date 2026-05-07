use crate::app::state::FileApp;
use crate::types::*;
use cosmic::iced::widget::{
    Space, column, container as iced_container, image as image_widget, mouse_area, row, text,
};
use cosmic::iced::{Alignment, Length, Padding};
use cosmic::widget::icon;
use cosmic::{Element, Theme};
use rust_i18n::t;

pub fn render(app: &FileApp) -> Element<'_, Message> {
    let is_grid = app.ui.view_mode == ViewMode::Grid;
    let sidebar_w = if app.ui.sidebar_visible && app.ui.mode == Mode::Manager {
        UI_SIDEBAR_WIDTH
    } else {
        0.0
    };

    let list_w = app.ui.window_width - sidebar_w;
    let item_width = if is_grid { 120.0 } else { list_w };
    let visual_item_height = if is_grid { 160.0 } else { UI_ROW_HEIGHT };
    let effective_item_height = if is_grid { 168.0 } else { UI_ROW_HEIGHT };

    let columns = if is_grid {
        (list_w / 128.0).floor().max(1.0) as usize
    } else {
        1
    };

    let exact_start_row = (app.ui.scroll_offset / effective_item_height)
        .floor()
        .max(0.0) as usize;

    let buffer_rows = 5;
    let start_row = exact_start_row.saturating_sub(buffer_rows);
    let visible_rows =
        (app.ui.window_height / effective_item_height).ceil() as usize + (buffer_rows * 2);

    let start_idx = start_row * columns;
    let end_idx = (start_idx + visible_rows * columns).min(app.fs.filtered_entries.len());

    let total_rows = (app.fs.filtered_entries.len() as f32 / columns as f32).ceil() as usize;
    let bottom_rows = total_rows.saturating_sub(start_row + visible_rows);

    let top_spacer_height = (start_row as f32) * effective_item_height;
    let bottom_spacer_height = (bottom_rows as f32) * effective_item_height;
    let thumbs_cache = app.ui.thumbnails.try_borrow().ok();

    let mut rows_vec = Vec::new();
    let mut current_row = row![].spacing(8);

    for idx in start_idx..end_idx {
        if let Some(&entry_idx) = app.fs.filtered_entries.get(idx)
            && let Some(entry) = app.fs.entries.get(entry_idx)
        {
            let is_selected = app.fs.selected_files.contains(&entry.path);
            let is_drop_target = app.ui.is_dragging_items
                && app.ui.hovered_row == Some(idx)
                && entry.is_dir
                && !app.fs.selected_files.contains(&entry.path);

            let is_hovered = app.ui.hovered_row == Some(idx)
                && !app.ui.is_dragging_marquee
                && !app.ui.is_dragging_items
                && app.ui.selection_start.is_none();
            let is_cut = app.tasks.clipboard_action == ClipboardAction::Cut
                && app.tasks.clipboard.contains(&entry.path);

            let graphic_size = if is_grid { 64 } else { 24 };
            let mut graphic: Element<'_, Message> =
                icon::from_name(entry.icon_name).size(graphic_size).into();

            if let Some(ref cache) = thumbs_cache
                && let Some(handle) = cache.peek(&entry.path)
            {
                graphic = image_widget(handle.clone())
                    .width(Length::Fixed(graphic_size as f32))
                    .height(Length::Fixed(graphic_size as f32))
                    .into();
            }

            let display_name = if is_grid {
                entry.grid_name.to_string()
            } else {
                let available_w = (list_w - 500.0).max(40.0);
                let max_chars = (available_w / 9.5) as usize;

                crate::app::view::truncate_text(&entry.name, max_chars)
            };

            let safe_name = display_name
                .replace(" ", "\u{00A0}")
                .replace("-", "\u{2011}");

            let mut text_item = row![
                text(safe_name)
                    .width(Length::Fill)
                    .align_x(if is_grid {
                        Alignment::Center
                    } else {
                        Alignment::Start
                    })
                    .size(14.0)
            ];

            if is_cut {
                text_item = text_item.push(text(format!(" ({})", t!("Cut"))).size(13.0));
            }

            let row_content: Element<'_, Message> = if is_grid {
                column![graphic, text_item.align_y(Alignment::Center)]
                    .align_x(Alignment::Center)
                    .spacing(8)
                    .into()
            } else {
                row![
                    graphic,
                    text_item.width(Length::Fill).align_y(Alignment::Center),
                    text(entry.file_type_str.as_ref())
                        .width(Length::Fixed(120.0))
                        .size(13.0),
                    text(entry.modified_str.as_ref())
                        .width(Length::Fixed(150.0))
                        .size(13.0),
                    text(entry.size_str.as_ref())
                        .width(Length::Fixed(100.0))
                        .size(13.0)
                ]
                .spacing(16)
                .align_y(Alignment::Center)
                .into()
            };

            let c = iced_container(row_content)
                .width(if is_grid {
                    Length::Fixed(112.0)
                } else {
                    Length::Fill
                })
                .height(Length::Fixed(visual_item_height))
                .padding(if is_grid {
                    Padding::from([12, 4])
                } else {
                    Padding::from([0, 16])
                })
                .align_y(Alignment::Center)
                .style(move |theme: &Theme| {
                    if is_selected {
                        FileApp::active_color_style(theme, 0.15)
                    } else if is_hovered {
                        FileApp::active_color_style(theme, 0.05)
                    } else if is_drop_target {
                        FileApp::active_color_style(theme, 0.25)
                    } else {
                        iced_container::Style::default()
                    }
                });

            let interactive_area = mouse_area(iced_container(c).width(if is_grid {
                Length::Fixed(item_width - 8.0)
            } else {
                Length::Fill
            }))
            .interaction(if is_hovered {
                cosmic::iced::mouse::Interaction::Pointer
            } else {
                cosmic::iced::mouse::Interaction::default()
            })
            .on_enter(Message::UI(UIMsg::HoverRow(idx, true)))
            .on_exit(Message::UI(UIMsg::HoverRow(idx, false)))
            .on_press(Message::UI(UIMsg::ItemPressed(idx, entry.path.clone())))
            .on_release(Message::UI(UIMsg::ItemReleased(entry.path.clone())))
            .on_right_press(Message::Sys(SysMsg::RightClickFile(
                entry.path.clone(),
                entry.icon_name,
            )));

            if is_grid {
                current_row = current_row.push(interactive_area);
                if (idx - start_idx + 1).is_multiple_of(columns) || idx == end_idx - 1 {
                    rows_vec.push(current_row.into());
                    current_row = row![].spacing(8);
                }
            } else {
                rows_vec.push(interactive_area.into());
            }
        }
    }

    let list_col = column(rows_vec).spacing(if is_grid { 8 } else { 0 });
    let mut outer_col = column![];

    if top_spacer_height > 0.0 {
        outer_col = outer_col.push(
            mouse_area(
                Space::new()
                    .width(Length::Fill)
                    .height(Length::Fixed(top_spacer_height)),
            )
            .on_press(Message::Sys(SysMsg::StartMarquee))
            .on_right_press(Message::Sys(SysMsg::RightClickSpace)),
        );
    }

    outer_col = outer_col.push(list_col);

    if bottom_spacer_height > 0.0 {
        outer_col = outer_col.push(
            mouse_area(
                Space::new()
                    .width(Length::Fill)
                    .height(Length::Fixed(bottom_spacer_height)),
            )
            .on_press(Message::Sys(SysMsg::StartMarquee))
            .on_right_press(Message::Sys(SysMsg::RightClickSpace)),
        );
    }

    let final_content = iced_container(outer_col).padding(Padding::from([0, 8]));
    let content_height = total_rows as f32 * effective_item_height;

    let list_start_y = if is_grid {
        UI_TOP_BAR_HEIGHT
    } else {
        UI_TOP_BAR_HEIGHT + UI_HEADER_HEIGHT
    };
    let remaining_height = (app.ui.window_height - list_start_y - content_height).max(64.0);

    let void_area = mouse_area(
        Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(remaining_height)),
    )
    .on_press(Message::Sys(SysMsg::StartMarquee))
    .on_right_press(Message::Sys(SysMsg::RightClickSpace));

    cosmic::iced::widget::scrollable(column![final_content, void_area].height(Length::Fill))
        .id(cosmic::iced::widget::Id::new("main_scroll"))
        .on_scroll(|v| Message::Sys(SysMsg::Scrolled(v)))
        .height(Length::Fill)
        .into()
}
