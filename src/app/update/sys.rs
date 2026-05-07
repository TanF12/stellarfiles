use crate::app::state::FileApp;
use crate::types::*;
use cosmic::app::Task;
use std::time::{Duration, Instant};

fn apply_marquee_selection(app: &mut FileApp, start: cosmic::iced::Point) {
    let list_start_y = UI_TOP_BAR_HEIGHT + UI_HEADER_HEIGHT;
    let is_grid = app.ui.view_mode == ViewMode::Grid;
    let item_height = if is_grid { 160.0 } else { UI_ROW_HEIGHT };
    let item_width = if is_grid { 120.0 } else { app.ui.window_width };
    let columns = if is_grid {
        (app.ui.window_width / item_width).floor().max(1.0) as usize
    } else {
        1
    };

    let sidebar_w = if app.ui.sidebar_visible && app.ui.mode == Mode::Manager {
        UI_SIDEBAR_WIDTH
    } else {
        0.0
    };

    let relative_top =
        (start.y.min(app.ui.current_mouse.y) - list_start_y + app.ui.scroll_offset).max(0.0);
    let relative_bottom =
        (start.y.max(app.ui.current_mouse.y) - list_start_y + app.ui.scroll_offset).max(0.0);

    let relative_left = (start.x.min(app.ui.current_mouse.x) - sidebar_w - 8.0).max(0.0);
    let relative_right = (start.x.max(app.ui.current_mouse.x) - sidebar_w - 8.0).max(0.0);

    let start_row = (relative_top / item_height).floor() as usize;
    let end_row = (relative_bottom / item_height).floor() as usize;
    let start_col = if is_grid {
        (relative_left / item_width).floor() as usize
    } else {
        0
    };
    let end_col = if is_grid {
        (relative_right / item_width).floor() as usize
    } else {
        columns.saturating_sub(1)
    };

    app.fs.selected_files.clear();
    for row in start_row..=end_row {
        for col in start_col..=end_col {
            let idx = row * columns + col;
            if idx < app.fs.filtered_entries.len() {
                let e_idx = app.fs.filtered_entries[idx];
                if let Some(e) = app.fs.entries.get(e_idx) {
                    app.fs.selected_files.insert(e.path.clone());
                }
            }
        }
    }
}

pub fn handle(app: &mut FileApp, msg: SysMsg) -> Task<Message> {
    match msg {
        SysMsg::ModifiersChanged(ctrl, shift) => {
            app.ctrl_pressed = ctrl;
            app.shift_pressed = shift;
            Task::none()
        }
        SysMsg::WindowResized(w, h) => {
            app.ui.window_width = w;
            app.ui.window_height = h;

            let folder_name = app
                .fs
                .current_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Root".into());

            app.core
                .set_title(None, format!("{} — Stellarfiles", folder_name))
        }
        SysMsg::StartMarquee => {
            app.ui.close_modals();
            if !app.ctrl_pressed && !app.shift_pressed {
                app.fs.selected_files.clear();
                app.ui.last_clicked_idx = None;
                app.ui.keyboard_cursor = None;
            }
            app.ui.selection_start = Some(app.ui.current_mouse);
            app.ui.item_drag_start = None;
            app.ui.is_dragging_marquee = false;
            app.ui.is_dragging_items = false;
            Task::none()
        }
        SysMsg::AutoScroll => {
            if !app.ui.is_dragging_marquee && !app.ui.is_dragging_items {
                return Task::none();
            }
            let mut dy = 0.0;
            let edge_threshold = 40.0;
            let list_start_y = UI_TOP_BAR_HEIGHT + UI_HEADER_HEIGHT;
            let top_edge = list_start_y + edge_threshold;
            let bottom_edge = app.ui.window_height - edge_threshold;

            if app.ui.current_mouse.y < top_edge {
                dy = app.ui.current_mouse.y - top_edge;
            } else if app.ui.current_mouse.y > bottom_edge {
                dy = app.ui.current_mouse.y - bottom_edge;
            }
            if dy != 0.0 {
                dy = dy.clamp(-30.0, 30.0);
                let speed = 1.0;
                let effective_item_height = if app.ui.view_mode == ViewMode::Grid {
                    168.0
                } else {
                    UI_ROW_HEIGHT
                };
                let columns = if app.ui.view_mode == ViewMode::Grid {
                    let sidebar_w = if app.ui.sidebar_visible && app.ui.mode == Mode::Manager {
                        UI_SIDEBAR_WIDTH
                    } else {
                        0.0
                    };
                    let list_w = app.ui.window_width - sidebar_w;
                    (list_w / 128.0).floor().max(1.0)
                } else {
                    1.0
                };
                let total_rows = (app.fs.filtered_entries.len() as f32 / columns).ceil();

                let bottom_bar_h = if app.ui.mode == Mode::Manager {
                    0.0
                } else {
                    60.0
                };
                let status_h = 40.0;
                let visible_h = app.ui.window_height - list_start_y - status_h - bottom_bar_h;

                let max_scroll = ((total_rows * effective_item_height) - visible_h).max(0.0);
                let new_offset = (app.ui.scroll_offset + dy * speed).clamp(0.0, max_scroll);

                if new_offset != app.ui.scroll_offset {
                    app.ui.scroll_offset = new_offset;
                    if app.ui.is_dragging_marquee
                        && let Some(start) = app.ui.selection_start
                    {
                        apply_marquee_selection(app, start);
                    }
                    return cosmic::iced::widget::scrollable::scroll_to(
                        cosmic::iced::widget::Id::new("main_scroll"),
                        cosmic::iced::widget::scrollable::AbsoluteOffset {
                            x: Some(0.0),
                            y: Some(app.ui.scroll_offset),
                        },
                    );
                }
            }
            Task::none()
        }
        SysMsg::MouseMoved(pos) => {
            app.ui.current_mouse = pos;
            if app.ui.is_dragging_marquee
                && let Some(start) = app.ui.selection_start
            {
                apply_marquee_selection(app, start);
            } else if let Some(start) = app.ui.selection_start {
                let distance = (start.x - pos.x).abs() + (start.y - pos.y).abs();
                if !app.ui.is_dragging_marquee && distance > 5.0 {
                    app.ui.is_dragging_marquee = true;
                }
            } else if let Some(start) = app.ui.item_drag_start {
                let distance = (start.x - pos.x).abs() + (start.y - pos.y).abs();
                if !app.ui.is_dragging_items && distance > 5.0 {
                    app.ui.is_dragging_items = true;
                }
            }
            Task::none()
        }
        SysMsg::DragEnd => {
            let mut dropped_dest: Option<std::path::PathBuf> = None;
            let sidebar_w = if app.ui.sidebar_visible && app.ui.mode == Mode::Manager {
                UI_SIDEBAR_WIDTH
            } else {
                0.0
            };

            if app.ui.is_dragging_items
                && app.ui.current_mouse.x > 0.0
                && app.ui.current_mouse.y > 0.0
            {
                if app.ui.current_mouse.x < sidebar_w {
                    let mut current_y = UI_SIDEBAR_PADDING_TOP + 46.0;
                    for item in &app.ui.sidebar_items {
                        if app.ui.current_mouse.y >= current_y
                            && app.ui.current_mouse.y <= current_y + 36.0
                        {
                            dropped_dest = Some(item.path.clone());
                            break;
                        }
                        current_y += 40.0;
                        if app.ui.expanded_tree_nodes.contains(&item.path)
                            && let Some(children) = app.ui.tree_cache.get(&item.path)
                        {
                            current_y += children.len() as f32 * 36.0;
                        }
                    }
                } else {
                    let list_start_y = UI_TOP_BAR_HEIGHT + UI_HEADER_HEIGHT;
                    let is_grid = app.ui.view_mode == ViewMode::Grid;
                    let list_w = app.ui.window_width - sidebar_w;

                    let item_h = if is_grid { 168.0 } else { UI_ROW_HEIGHT };
                    let columns = if is_grid {
                        (list_w / 128.0).floor().max(1.0) as usize
                    } else {
                        1
                    };

                    let relative_y =
                        (app.ui.current_mouse.y - list_start_y + app.ui.scroll_offset).max(0.0);
                    let relative_x = (app.ui.current_mouse.x - sidebar_w - 8.0).max(0.0);

                    let row_idx = (relative_y / item_h).floor() as usize;
                    let col_idx = if is_grid {
                        (relative_x / 128.0).floor() as usize
                    } else {
                        0
                    };

                    let idx = row_idx * columns + col_idx;

                    if idx < app.fs.filtered_entries.len() {
                        let entry_idx = app.fs.filtered_entries[idx];
                        if let Some(entry) = app.fs.entries.get(entry_idx)
                            && entry.is_dir
                        {
                            dropped_dest = Some(entry.path.clone());
                        }
                    }
                }

                if let Some(dest) = dropped_dest
                    && dest.is_dir()
                    && dest != app.fs.current_dir
                    && !app.fs.selected_files.contains(&dest)
                {
                    app.tasks.clipboard = app.fs.selected_files.iter().cloned().collect();
                    app.tasks.clipboard_action = ClipboardAction::Cut;

                    app.ui.close_modals();
                    app.ui.selection_start = None;
                    app.ui.item_drag_start = None;
                    app.ui.is_dragging_marquee = false;
                    app.ui.is_dragging_items = false;

                    return Task::perform(async {}, move |_| {
                        cosmic::Action::App(Message::File(FileMsg::ActionPaste(Some(dest.clone()))))
                    });
                }
            }

            app.ui.selection_start = None;
            app.ui.item_drag_start = None;
            app.ui.is_dragging_marquee = false;
            app.ui.is_dragging_items = false;
            Task::none()
        }
        SysMsg::Scrolled(viewport) => {
            app.ui.scroll_offset = viewport.absolute_offset().y;
            Task::none()
        }
        SysMsg::MoveCursor(delta) => {
            if app.fs.filtered_entries.is_empty() {
                return Task::none();
            }
            let new_idx = if let Some(idx) = app.ui.keyboard_cursor {
                (idx as isize + delta).clamp(0, app.fs.filtered_entries.len() as isize - 1) as usize
            } else {
                0
            };
            app.ui.keyboard_cursor = Some(new_idx);
            let entry_idx = app.fs.filtered_entries[new_idx];

            if let Some(entry) = app.fs.entries.get(entry_idx)
                && !app.ctrl_pressed
                && !app.shift_pressed
            {
                app.fs.selected_files.clear();
                app.fs.selected_files.insert(entry.path.clone());
            }

            let list_start_y = UI_TOP_BAR_HEIGHT + UI_HEADER_HEIGHT;
            let sidebar_w = if app.ui.sidebar_visible && app.ui.mode == Mode::Manager {
                UI_SIDEBAR_WIDTH
            } else {
                0.0
            };
            let list_w = app.ui.window_width - sidebar_w;
            let is_grid = app.ui.view_mode == ViewMode::Grid;

            let effective_item_height = if is_grid { 168.0 } else { UI_ROW_HEIGHT };
            let columns = if is_grid {
                (list_w / 128.0).floor().max(1.0) as usize
            } else {
                1
            };

            let row = new_idx / columns;
            let item_y = row as f32 * effective_item_height;

            let bottom_bar_h = if app.ui.mode == Mode::Manager {
                0.0
            } else {
                60.0
            };
            let status_h = 40.0;
            let padding_margin = 20.0;
            let visible_h =
                app.ui.window_height - list_start_y - status_h - bottom_bar_h - padding_margin;

            let mut offset = app.ui.scroll_offset;
            if item_y < offset {
                offset = item_y;
            } else if item_y + effective_item_height > offset + visible_h {
                offset = item_y + effective_item_height - visible_h;
            }

            if offset != app.ui.scroll_offset {
                app.ui.scroll_offset = offset;
                return cosmic::iced::widget::scrollable::scroll_to(
                    cosmic::iced::widget::Id::new("main_scroll"),
                    cosmic::iced::widget::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(offset),
                    },
                );
            }

            Task::none()
        }
        SysMsg::ActivateCursor => {
            if let Some(idx) = app.ui.keyboard_cursor {
                let entry_idx = app.fs.filtered_entries[idx];
                if let Some(entry) = app.fs.entries.get(entry_idx) {
                    let p = entry.path.clone();
                    return Task::perform(async move { p }, |p| {
                        cosmic::Action::App(Message::Sys(SysMsg::ActivateFile(p)))
                    });
                }
            }
            Task::none()
        }
        SysMsg::GlobalKeyPress(key, modifiers) => {
            use cosmic::iced::keyboard::key::Named;
            let is_modal_active = app.ui.rename_modal.is_some()
                || app.ui.batch_rename_modal
                || app.ui.show_new_modal.is_some()
                || app.ui.compress_wizard.is_some()
                || app.ui.open_with_modal.is_some()
                || app.ui.properties_modal.is_some()
                || matches!(app.ui.mode, Mode::Save(_));

            if let cosmic::iced::keyboard::Key::Named(Named::F5) = key {
                if let Ok(mut cache) = app.ui.thumbnails.try_borrow_mut() {
                    cache.clear();
                }
                return Task::perform(async {}, |_| {
                    cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
                });
            }

            if app.ui.search_visible && !is_modal_active {
                if let cosmic::iced::keyboard::Key::Named(Named::Backspace) = key {
                    app.ui.search_query.pop();
                    app.ui.type_buffer = app.ui.search_query.clone();
                    let q = app.ui.search_query.clone();
                    return Task::perform(async move { q }, |query| {
                        cosmic::Action::App(Message::UI(UIMsg::SearchChanged(query)))
                    });
                }
                if let cosmic::iced::keyboard::Key::Named(Named::Escape) = key {
                    return Task::perform(async {}, |_| {
                        cosmic::Action::App(Message::UI(UIMsg::CloseSearch))
                    });
                }
            }

            if let cosmic::iced::keyboard::Key::Character(c) = &key
                && !modifiers.control()
                && !modifiers.alt()
                && !is_modal_active
            {
                let now = Instant::now();
                if let Some(last) = app.ui.last_typing_time
                    && now.duration_since(last) > Duration::from_millis(1500)
                {
                    app.ui.type_buffer.clear();
                }
                app.ui.type_buffer.push_str(c.as_str());
                app.ui.last_typing_time = Some(now);
                app.ui.search_visible = true;
                let q = app.ui.type_buffer.clone();
                return Task::perform(async move { q }, |query| {
                    cosmic::Action::App(Message::UI(UIMsg::SearchChanged(query)))
                });
            }

            if !is_modal_active && !app.ui.search_visible {
                match &key {
                    cosmic::iced::keyboard::Key::Named(Named::Delete) => {
                        return Task::perform(async {}, |_| {
                            cosmic::Action::App(Message::File(FileMsg::ActionTrash))
                        });
                    }
                    cosmic::iced::keyboard::Key::Named(Named::ArrowUp) => {
                        return Task::perform(async {}, |_| {
                            cosmic::Action::App(Message::Sys(SysMsg::MoveCursor(-1)))
                        });
                    }
                    cosmic::iced::keyboard::Key::Named(Named::ArrowDown) => {
                        return Task::perform(async {}, |_| {
                            cosmic::Action::App(Message::Sys(SysMsg::MoveCursor(1)))
                        });
                    }
                    cosmic::iced::keyboard::Key::Named(Named::Enter) => {
                        return Task::perform(async {}, |_| {
                            cosmic::Action::App(Message::Sys(SysMsg::ActivateCursor))
                        });
                    }
                    cosmic::iced::keyboard::Key::Named(Named::Backspace) => {
                        return Task::perform(async {}, |_| {
                            cosmic::Action::App(Message::Nav(NavMsg::NavigateUp))
                        });
                    }
                    _ => {}
                }
                if modifiers.control()
                    && let cosmic::iced::keyboard::Key::Character(c) = &key
                {
                    match c.as_str() {
                        "c" | "C" => {
                            return Task::perform(async {}, |_| {
                                cosmic::Action::App(Message::File(FileMsg::ActionCopy))
                            });
                        }
                        "x" | "X" => {
                            return Task::perform(async {}, |_| {
                                cosmic::Action::App(Message::File(FileMsg::ActionCut))
                            });
                        }
                        "v" | "V" => {
                            return Task::perform(async {}, |_| {
                                cosmic::Action::App(Message::File(FileMsg::ActionPaste(None)))
                            });
                        }
                        "a" | "A" => {
                            return Task::perform(async {}, |_| {
                                cosmic::Action::App(Message::File(FileMsg::ActionSelectAll))
                            });
                        }
                        _ => {}
                    }
                }
            }
            if modifiers.alt()
                && let cosmic::iced::keyboard::Key::Named(Named::ArrowUp | Named::ArrowLeft) = key
            {
                return Task::perform(async {}, |_| {
                    cosmic::Action::App(Message::Nav(NavMsg::NavigateUp))
                });
            }
            Task::none()
        }
        SysMsg::FilesDropped(paths) => {
            let mut dropped_dest = app.fs.current_dir.clone();

            let sidebar_w = if app.ui.sidebar_visible && app.ui.mode == Mode::Manager {
                UI_SIDEBAR_WIDTH
            } else {
                0.0
            };
            if app.ui.current_mouse.x > sidebar_w {
                let is_grid = app.ui.view_mode == ViewMode::Grid;
                let list_start_y = if is_grid {
                    UI_TOP_BAR_HEIGHT
                } else {
                    UI_TOP_BAR_HEIGHT + UI_HEADER_HEIGHT
                };

                let relative_y =
                    (app.ui.current_mouse.y - list_start_y + app.ui.scroll_offset).max(0.0);
                let relative_x = (app.ui.current_mouse.x - sidebar_w - 8.0).max(0.0);

                let item_w = if is_grid { 120.0 } else { app.ui.window_width };
                let item_h = if is_grid { 160.0 } else { UI_ROW_HEIGHT };
                let columns = if is_grid {
                    (app.ui.window_width / item_w).floor().max(1.0) as usize
                } else {
                    1
                };

                let row_idx = (relative_y / item_h).floor() as usize;
                let col_idx = if is_grid {
                    (relative_x / item_w).floor() as usize
                } else {
                    0
                };
                let idx = row_idx * columns + col_idx;

                if idx < app.fs.filtered_entries.len() {
                    let entry_idx = app.fs.filtered_entries[idx];
                    if let Some(entry) = app.fs.entries.get(entry_idx)
                        && entry.is_dir
                    {
                        dropped_dest = entry.path.clone();
                    }
                }
            }

            app.tasks.clipboard = paths;
            app.tasks.clipboard_action = ClipboardAction::Copy;
            Task::perform(
                async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                },
                move |_| {
                    cosmic::Action::App(Message::File(FileMsg::ActionPaste(Some(
                        dropped_dest.clone(),
                    ))))
                },
            )
        }
        SysMsg::ListenFilesystem => {
            if let Some(rx) = app.notify_rx.as_ref() {
                let rxc = rx.clone();
                Task::perform(async move { rxc.recv().await.unwrap() }, |res| match res {
                    Ok(event) => cosmic::Action::App(Message::Sys(SysMsg::FilesystemDelta(event))),
                    Err(_) => cosmic::Action::App(Message::NoOp),
                })
            } else {
                Task::none()
            }
        }
        SysMsg::FilesystemDelta(event) => {
            app.fs.pending_fs_events.push(event);
            app.fs.fs_debounce_version += 1;
            let current_version = app.fs.fs_debounce_version;

            Task::perform(
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    current_version
                },
                |v| cosmic::Action::App(Message::Sys(SysMsg::ProcessDebouncedFsEvents(v))),
            )
        }
        SysMsg::ProcessDebouncedFsEvents(version) => {
            if version == app.fs.fs_debounce_version {
                let has_events = !app.fs.pending_fs_events.is_empty();
                app.fs.pending_fs_events.clear();

                if has_events {
                    return Task::perform(async {}, |_| {
                        cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
                    });
                } else {
                    return Task::perform(async {}, |_| {
                        cosmic::Action::App(Message::Sys(SysMsg::ListenFilesystem))
                    });
                }
            }
            Task::none()
        }
        SysMsg::ThumbnailsLoaded(thumbs) => {
            if let Ok(mut cache) = app.ui.thumbnails.try_borrow_mut() {
                for (path, w, h, bytes) in thumbs {
                    cache.put(
                        path,
                        cosmic::iced::widget::image::Handle::from_rgba(w, h, bytes),
                    );
                }
            }
            Task::none()
        }
        SysMsg::ActivateFile(path) => {
            app.ui.close_modals();
            if path.is_dir() {
                Task::perform(async move { path }, |p| {
                    cosmic::Action::App(Message::Nav(NavMsg::Navigate(p)))
                })
            } else if app.ui.mode == Mode::Manager {
                let _ = open::that(&path);
                Task::none()
            } else {
                Task::perform(async {}, |_| {
                    cosmic::Action::App(Message::UI(UIMsg::DialogConfirm))
                })
            }
        }
        SysMsg::RightClickFile(path, icon) => {
            if !app.fs.selected_files.contains(&path) {
                app.fs.selected_files.clear();
                app.fs.selected_files.insert(path.clone());
            }
            app.ui.context_menu = Some((app.ui.current_mouse, ContextTarget::File(path, icon)));
            Task::none()
        }
        SysMsg::RightClickSpace => {
            app.fs.selected_files.clear();
            app.ui.context_menu = Some((app.ui.current_mouse, ContextTarget::EmptySpace));
            Task::none()
        }
        SysMsg::ClearContextMenu => {
            app.ui.context_menu = None;
            Task::none()
        }
        SysMsg::PortalReq(req) => {
            app.ui.close_modals();
            match req {
                PortalRequest::OpenFile(tx) => {
                    app.ui.mode = Mode::Open;
                    app.portal_tx = Some(tx);
                }
                PortalRequest::SaveFile(name, tx) => {
                    app.ui.mode = Mode::Save(name.clone());
                    app.ui.save_dialog_input = name;
                    app.portal_tx = Some(tx);
                }
            }
            Task::none()
        }
    }
}
