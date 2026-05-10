use crate::app::state::FileApp;
use crate::types::*;
use cosmic::app::Task;
use std::time::Duration;

pub fn handle(app: &mut FileApp, msg: UIMsg) -> Task<Message> {
    match msg {
        UIMsg::UpdateWindowTitle(title) => app.core.set_title(None, title),
        UIMsg::CloseAllModals => {
            app.ui.close_modals();
            Task::none()
        }
        UIMsg::ToggleDeepSearch => {
            app.ui.search_deep = !app.ui.search_deep;
            app.sort_and_filter_entries(std::sync::Arc::clone(&app.fs.entries))
        }
        UIMsg::OpenBatchRenameModal => {
            app.ui.close_modals();
            app.ui.batch_rename_modal = true;
            app.ui.batch_rename_pattern.clear();
            app.ui.batch_rename_replace.clear();
            Task::none()
        }
        UIMsg::BatchRenamePatternChanged(s) => {
            app.ui.batch_rename_pattern = s;
            Task::none()
        }
        UIMsg::BatchRenameReplaceChanged(s) => {
            app.ui.batch_rename_replace = s;
            Task::none()
        }
        UIMsg::ToggleSidebar => {
            app.ui.sidebar_visible = !app.ui.sidebar_visible;
            Task::none()
        }
        UIMsg::ToggleSidebarNode(path) => {
            if app.ui.expanded_tree_nodes.contains(&path) {
                app.ui.expanded_tree_nodes.remove(&path);
                Task::none()
            } else {
                app.ui.expanded_tree_nodes.insert(path.clone());
                let path_clone = path.clone();
                Task::perform(
                    async move { crate::file_ops::read::load_directory(path_clone.clone()).await },
                    move |res| {
                        if let Ok(entries) = res {
                            cosmic::Action::App(Message::UI(UIMsg::SidebarNodeLoaded(
                                path, entries,
                            )))
                        } else {
                            cosmic::Action::App(Message::NoOp)
                        }
                    },
                )
            }
        }
        UIMsg::SidebarNodeLoaded(path, entries) => {
            app.ui.tree_cache.insert(path, entries);
            Task::none()
        }
        UIMsg::ToggleHiddenFiles => {
            app.ui.show_hidden = !app.ui.show_hidden;
            app.sort_and_filter_entries(std::sync::Arc::clone(&app.fs.entries))
        }
        UIMsg::SortBy(key) => {
            if app.ui.sort_key == key {
                app.ui.sort_direction = if app.ui.sort_direction == SortDirection::Asc {
                    SortDirection::Desc
                } else {
                    SortDirection::Asc
                };
            } else {
                app.ui.sort_key = key;
                app.ui.sort_direction = SortDirection::Asc;
            }
            app.sort_and_filter_entries(std::sync::Arc::clone(&app.fs.entries))
        }
        UIMsg::ToggleSearch => {
            app.ui.search_visible = true;
            Task::none()
        }
        UIMsg::CloseSearch => {
            app.ui.search_visible = false;
            app.ui.search_query.clear();
            app.ui.type_buffer.clear();
            app.sort_and_filter_entries(std::sync::Arc::clone(&app.fs.entries))
        }
        UIMsg::ToggleRegex => {
            app.ui.search_regex = !app.ui.search_regex;
            app.sort_and_filter_entries(std::sync::Arc::clone(&app.fs.entries))
        }
        UIMsg::SearchChanged(query) => {
            app.ui.search_query = query;
            app.ui.search_version += 1;
            let version = app.ui.search_version;
            Task::perform(
                async move {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    version
                },
                |v| cosmic::Action::App(Message::UI(UIMsg::SearchExecute(v))),
            )
        }
        UIMsg::SearchExecute(version) => {
            if version == app.ui.search_version {
                app.sort_and_filter_entries(std::sync::Arc::clone(&app.fs.entries))
            } else {
                Task::none()
            }
        }
        UIMsg::SearchCompleted(entries, filtered_indices) => {
            app.fs.entries = entries;
            app.fs.filtered_entries = filtered_indices;
            let valid_paths: std::collections::HashSet<_> =
                app.fs.entries.iter().map(|e| e.path.clone()).collect();
            app.fs.selected_files.retain(|p| valid_paths.contains(p));
            Task::none()
        }
        UIMsg::HoverRow(idx, entering) => {
            if entering {
                app.ui.hovered_row = Some(idx);
            } else if app.ui.hovered_row == Some(idx) {
                app.ui.hovered_row = None;
            }
            Task::none()
        }
        UIMsg::ItemPressed(idx, path) => {
            app.ui.close_modals();
            app.ui.selection_start = None;
            app.ui.is_dragging_marquee = false;
            app.ui.keyboard_cursor = Some(idx);
            if app.shift_pressed
                && let Some(last_idx) = app.ui.last_clicked_idx
            {
                let start = last_idx.min(idx);
                let end = last_idx.max(idx);
                for i in start..=end {
                    if let Some(&e_idx) = app.fs.filtered_entries.get(i)
                        && let Some(entry) = app.fs.entries.get(e_idx)
                    {
                        app.fs.selected_files.insert(entry.path.clone());
                    }
                }
            } else if app.ctrl_pressed {
                if !app.fs.selected_files.insert(path.clone()) {
                    app.fs.selected_files.remove(&path);
                }
                app.ui.last_clicked_idx = Some(idx);
            } else {
                if !app.fs.selected_files.contains(&path) {
                    app.fs.selected_files.clear();
                    app.fs.selected_files.insert(path.clone());
                    app.ui.last_clicked_idx = Some(idx);
                }
            }
            if matches!(app.ui.mode, Mode::Save(_)) && !path.is_dir() {
                app.ui.save_dialog_input = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
            }
            app.ui.item_drag_start = Some(app.ui.current_mouse);
            app.ui.is_dragging_items = false;
            Task::none()
        }
        UIMsg::ItemReleased(path) => {
            if app.ui.is_dragging_items {
                if path.is_dir()
                    && path != app.fs.current_dir
                    && !app.fs.selected_files.contains(&path)
                {
                    app.tasks.clipboard = app.fs.selected_files.iter().cloned().collect();
                    app.tasks.clipboard_action = ClipboardAction::Cut;

                    app.ui.close_modals();
                    app.ui.selection_start = None;
                    app.ui.item_drag_start = None;
                    app.ui.is_dragging_marquee = false;
                    app.ui.is_dragging_items = false;

                    return Task::perform(async {}, move |_| {
                        cosmic::Action::App(Message::File(FileMsg::ActionPaste(Some(path.clone()))))
                    });
                }
            } else {
                let now = std::time::Instant::now();
                if let Some(last_time) = app.ui.last_release_time
                    && let Some(last_path) = &app.ui.last_release_path
                    && last_path == &path
                    && now.duration_since(last_time).as_millis() < 500
                {
                    let path_clone = path.clone();
                    return Task::perform(async move { path_clone }, |p| {
                        cosmic::Action::App(Message::Sys(SysMsg::ActivateFile(p)))
                    });
                }
                app.ui.last_release_time = Some(now);
                app.ui.last_release_path = Some(path.clone());
            }

            app.ui.item_drag_start = None;
            app.ui.is_dragging_items = false;
            Task::none()
        }
        UIMsg::OpenProperties(path, icon) => {
            app.ui.close_modals();
            Task::perform(crate::file_ops::read::fetch_properties(path, icon), |p| {
                cosmic::Action::App(Message::UI(UIMsg::PropertiesLoaded(p)))
            })
        }
        UIMsg::PropertiesLoaded(props) => {
            app.ui.properties_mode_input = format!("{:04o}", props.mode_octal);
            app.ui.properties_modal = Some(props);
            Task::none()
        }
        UIMsg::PropertiesModeChanged(s) => {
            app.ui.properties_mode_input = s;
            Task::none()
        }
        UIMsg::OpenWithModal(path) => {
            app.ui.close_modals();
            app.ui.open_with_modal = Some(path);
            app.ui.open_with_cmd.clear();
            Task::none()
        }
        UIMsg::OpenWithCmdChanged(s) => {
            app.ui.open_with_cmd = s;
            Task::none()
        }
        UIMsg::OpenRenameModal(path) => {
            app.ui.close_modals();
            app.ui.rename_input = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            app.ui.rename_modal = Some(path);
            Task::none()
        }
        UIMsg::RenameInputChanged(v) => {
            app.ui.rename_input = v;
            Task::none()
        }
        UIMsg::OpenNewFolderModal => {
            app.ui.close_modals();
            app.ui.show_new_modal = Some("Folder");
            app.ui.new_input.clear();
            Task::none()
        }
        UIMsg::OpenNewFileModal => {
            app.ui.close_modals();
            app.ui.show_new_modal = Some("File");
            app.ui.new_input.clear();
            Task::none()
        }
        UIMsg::NewInputChanged(v) => {
            app.ui.new_input = v;
            Task::none()
        }
        UIMsg::OpenCompressWizard(path) => {
            app.ui.context_menu = None;
            let count = app.fs.selected_files.len();

            let base_name = if count > 1 {
                "Archive".to_string()
            } else {
                let mut stem = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();

                let exts = [".tar", ".gz", ".zst", ".bz3", ".zip", ".7z", ".iso"];
                for ext in &exts {
                    if stem.ends_with(ext) {
                        stem = stem[..stem.len() - ext.len()].to_string();
                    }
                }
                stem
            };

            let extension = match app.ui.compress_format.as_str() {
                "tar.gz" => ".tar.gz",
                "tar.zst" => ".tar.zst",
                "tar.bz3" => ".tar.bz3",
                "7z" => ".7z",
                _ => ".zip",
            };

            app.ui.compress_name_input = format!("{}{}", base_name, extension);
            app.ui.compress_wizard = Some(path);
            cosmic::app::Task::none()
        }
        UIMsg::CompressNameChanged(n) => {
            app.ui.compress_name_input = n;
            Task::none()
        }
        UIMsg::CompressFormatChanged(f) => {
            let get_ext = |format: &str| match format {
                "tar.gz" => ".tar.gz",
                "tar.zst" => ".tar.zst",
                "tar.bz3" => ".tar.bz3",
                "7z" => ".7z",
                _ => ".zip",
            };

            let old_ext = get_ext(&app.ui.compress_format);
            let new_ext = get_ext(&f);

            app.ui.compress_format = f;

            if app.ui.compress_name_input.ends_with(old_ext) {
                let base_name = app.ui.compress_name_input.trim_end_matches(old_ext);
                app.ui.compress_name_input = format!("{}{}", base_name, new_ext);
            }

            cosmic::app::Task::none()
        }
        UIMsg::CompressLevelChanged(l) => {
            app.ui.compress_level = l;
            Task::none()
        }
        UIMsg::DialogSaveNameChanged(n) => {
            app.ui.save_dialog_input = n;
            Task::none()
        }
        UIMsg::DialogConfirm => {
            let path = if let Mode::Save(_) = app.ui.mode {
                app.fs
                    .current_dir
                    .join(&app.ui.save_dialog_input)
                    .to_string_lossy()
                    .into_owned()
            } else {
                app.fs
                    .selected_files
                    .iter()
                    .next()
                    .unwrap_or(&app.fs.current_dir)
                    .to_string_lossy()
                    .into_owned()
            };
            let tx_opt = app.portal_tx.take();
            Task::perform(
                async move {
                    if let Some(tx) = tx_opt {
                        let _ = tx.send(path).await;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                },
                |_| cosmic::Action::App(Message::ExitApp),
            )
        }
        UIMsg::DialogCancel => Task::perform(
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
            },
            |_| cosmic::Action::App(Message::ExitApp),
        ),
        UIMsg::ToggleViewMode => {
            app.ui.view_mode = if app.ui.view_mode == ViewMode::List {
                ViewMode::Grid
            } else {
                ViewMode::List
            };
            Task::none()
        }
    }
}
