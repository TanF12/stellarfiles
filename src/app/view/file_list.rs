use crate::app::state::FileApp;
use crate::types::*;
use cosmic::iced::widget::{
    Space, column, container as iced_container, image as image_widget, mouse_area, row, text,
};
use cosmic::iced::{Alignment, Length, Padding};
use cosmic::widget::icon;
use cosmic::{Element, Theme};
use rust_i18n::t;
use std::borrow::Cow;

pub fn render(app: &FileApp) -> Element<'_, Message> {
    let is_grid = app.ui.view_mode == ViewMode::Grid;
    let sidebar_w = if app.ui.sidebar_visible && app.ui.mode == Mode::Manager {
        UI_SIDEBAR_WIDTH
    } else {
        0.0
    };

    let list_w = (app.ui.window_width - sidebar_w - 16.0).max(100.0);

    let (visual_item_height, effective_item_height) = if is_grid {
        (160.0, 168.0)
    } else {
        (UI_ROW_HEIGHT, UI_ROW_HEIGHT)
    };

    let item_width = if is_grid { 128.0 } else { list_w };

    let columns = if is_grid {
        (list_w / item_width).floor().max(1.0) as usize
    } else {
        1
    };

    let total_items = app.fs.filtered_entries.len();
    let total_rows = total_items.div_ceil(columns);
    let total_content_height = total_rows as f32 * effective_item_height;

    let exact_start_row = (app.ui.scroll_offset / effective_item_height).max(0.0) as usize;

    let buffer = if is_grid { 2 } else { 4 };
    let start_row = exact_start_row.saturating_sub(buffer);

    let base_visible_rows = (app.ui.window_height / effective_item_height).ceil() as usize;
    let visible_rows = base_visible_rows + (buffer * 2);

    let end_row = (start_row + visible_rows).min(total_rows);

    let start_idx = start_row * columns;
    let end_idx = (end_row * columns).min(total_items);

    let top_spacer_height = start_row as f32 * effective_item_height;

    let thumbs_cache = app.ui.thumbnails.try_borrow().ok();

    let mut rows_vec = Vec::with_capacity(end_row.saturating_sub(start_row));
    let mut current_row = row![];

    fn format_safe_name<'a>(s: &'a str, max_chars: usize) -> Cow<'a, str> {
        let char_count = s.chars().count();
        if char_count <= max_chars {
            if !s.contains(' ') && !s.contains('-') {
                return Cow::Borrowed(s);
            }
            let mut res = String::with_capacity(char_count);
            for c in s.chars() {
                res.push(match c {
                    ' ' => '\u{00A0}',
                    '-' => '\u{2011}',
                    _ => c,
                });
            }
            return Cow::Owned(res);
        }
        let available = max_chars.saturating_sub(3);
        if available == 0 {
            return Cow::Owned("...".to_string());
        }
        let keep_front = (available / 2) + (available % 2);
        let keep_back = available / 2;

        let mut res = String::with_capacity(max_chars);
        for c in s.chars().take(keep_front) {
            res.push(match c {
                ' ' => '\u{00A0}',
                '-' => '\u{2011}',
                _ => c,
            });
        }
        res.push_str("...");
        for c in s.chars().skip(char_count - keep_back) {
            res.push(match c {
                ' ' => '\u{00A0}',
                '-' => '\u{2011}',
                _ => c,
            });
        }
        Cow::Owned(res)
    }

    let is_clipboard_cut = app.tasks.clipboard_action == ClipboardAction::Cut;
    let has_clipboard_items = !app.tasks.clipboard.is_empty();

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

            let is_cut = is_clipboard_cut
                && has_clipboard_items
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

            let max_chars = if is_grid {
                15
            } else {
                let available_w = (list_w - 500.0).max(40.0);
                (available_w / 9.5) as usize
            };

            let name_source: &str = if is_grid {
                entry.grid_name.as_ref()
            } else {
                entry.name.as_ref()
            };
            let safe_name = format_safe_name(name_source, max_chars);

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

            let cell_inner = iced_container(row_content)
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

            let interactive_area = mouse_area(cell_inner)
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

            let outer_cell = iced_container(interactive_area)
                .width(if is_grid {
                    Length::Fixed(128.0)
                } else {
                    Length::Fill
                })
                .height(Length::Fixed(effective_item_height))
                .align_x(Alignment::Center)
                .align_y(Alignment::Start);

            if is_grid {
                current_row = current_row.push(outer_cell);
                if (idx - start_idx + 1) % columns == 0 || idx == end_idx - 1 {
                    rows_vec.push(current_row.into());
                    current_row = row![];
                }
            } else {
                rows_vec.push(outer_cell.into());
            }
        }
    }

    let rendered_rows_height = rows_vec.len() as f32 * effective_item_height;
    let bottom_spacer_height =
        (total_content_height - top_spacer_height - rendered_rows_height).max(0.0);

    let inner_list = iced_container(column![
        Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(top_spacer_height)),
        column(rows_vec),
        Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(bottom_spacer_height)),
    ])
    .width(Length::Fill)
    .height(Length::Fixed(total_content_height))
    .align_y(Alignment::Start);

    let list_start_y = if is_grid {
        UI_TOP_BAR_HEIGHT
    } else {
        UI_TOP_BAR_HEIGHT + UI_HEADER_HEIGHT
    };

    let mut final_col = column![inner_list];

    if total_content_height < app.ui.window_height - list_start_y {
        let remaining_height =
            (app.ui.window_height - list_start_y - total_content_height).max(0.0);
        final_col = final_col.push(
            mouse_area(
                Space::new()
                    .width(Length::Fill)
                    .height(Length::Fixed(remaining_height)),
            )
            .on_press(Message::Sys(SysMsg::StartMarquee))
            .on_right_press(Message::Sys(SysMsg::RightClickSpace)),
        );
    }

    let final_content = iced_container(final_col)
        .padding(Padding::from([0, 8]))
        .width(Length::Fill)
        .align_y(Alignment::Start);

    cosmic::iced::widget::scrollable(final_content)
        .id(cosmic::iced::widget::Id::new("main_scroll"))
        .on_scroll(|v| Message::Sys(SysMsg::Scrolled(v)))
        .height(Length::Fill)
        .into()
}
