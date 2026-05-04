use crate::app::state::FileApp;
use crate::types::{Message, NavMsg, SysMsg, UIMsg};
use cosmic::app::Task;
use notify::Watcher;
use rust_i18n::t;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn navigate_to(app: &mut FileApp, path: PathBuf, is_historical: bool) -> Task<Message> {
    if !is_historical && app.fs.current_dir != path {
        app.fs.history_back.push(app.fs.current_dir.clone());
        app.fs.history_forward.clear();
    }

    app.ui.path_history.put(
        app.fs.current_dir.clone(),
        (app.ui.scroll_offset, app.ui.keyboard_cursor),
    );

    app.ui.path_input = path.to_string_lossy().into_owned();
    app.ui.path_version += 1;
    app.ui.close_modals();
    app.ui.search_query.clear();
    app.ui.search_visible = false;
    app.ui.status_msg = format!("{} {}...", t!("Loading..."), path.display());

    if let Some(w) = &mut app.fs.watcher {
        let _ = w.unwatch(&app.fs.current_dir);
        let _ = w.watch(&path, notify::RecursiveMode::NonRecursive);
    }

    app.fs.entries = Arc::new(Vec::new());
    app.fs.filtered_entries = Vec::new();
    app.ui.scroll_offset = 0.0;
    app.ui.keyboard_cursor = None;
    app.fs.current_dir = path.clone();

    let path_clone_async = path.clone();
    let path_clone_callback = path.clone();

    let load_task = Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(15)).await;
            crate::file_ops::read::load_directory(path_clone_async).await
        },
        move |res| match res {
            Ok(e) => cosmic::Action::App(Message::Nav(NavMsg::DirectoryLoaded(
                path_clone_callback.clone(),
                e,
            ))),
            Err(e) => cosmic::Action::App(Message::Nav(NavMsg::DirectoryLoadFailed(e))),
        },
    );

    let mut tasks = vec![load_task];
    if !app.ui.path_history.contains(&path) {
        let scroll_snap = cosmic::iced::widget::scrollable::snap_to(
            cosmic::iced::widget::Id::new("main_scroll"),
            cosmic::iced::widget::scrollable::RelativeOffset::START.into(),
        );
        tasks.push(scroll_snap);
    }

    Task::batch(tasks)
}

pub fn handle(app: &mut FileApp, msg: NavMsg) -> Task<Message> {
    match msg {
        NavMsg::NavigateUp => {
            let path = app
                .fs
                .current_dir
                .parent()
                .unwrap_or(&app.fs.current_dir)
                .to_path_buf();
            navigate_to(app, path, false)
        }
        NavMsg::Navigate(path) => navigate_to(app, path, false),
        NavMsg::NavigateBack => {
            if let Some(prev) = app.fs.history_back.pop() {
                app.fs.history_forward.push(app.fs.current_dir.clone());
                return navigate_to(app, prev, true);
            }
            Task::none()
        }
        NavMsg::NavigateForward => {
            if let Some(next) = app.fs.history_forward.pop() {
                app.fs.history_back.push(app.fs.current_dir.clone());
                return navigate_to(app, next, true);
            }
            Task::none()
        }
        NavMsg::PathInputChanged(val) => {
            app.ui.path_input = val.clone();
            app.ui.path_version += 1;
            let version = app.ui.path_version;
            Task::perform(
                async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    (val, version)
                },
                |(v, ver)| cosmic::Action::App(Message::Nav(NavMsg::PathInputDebounced(v, ver))),
            )
        }
        NavMsg::PathInputDebounced(val, version) => {
            if version == app.ui.path_version
                && val != app.fs.current_dir.to_string_lossy().into_owned()
            {
                let path = PathBuf::from(&val);
                if path.exists() && path.is_dir() {
                    return navigate_to(app, path, false);
                }
            }
            Task::none()
        }
        NavMsg::NavigateToInput(val) => {
            let path = PathBuf::from(&val);
            if path.exists() && path.is_dir() {
                return navigate_to(app, path, false);
            }
            Task::none()
        }
        NavMsg::RefreshCurrentDir => {
            let path = app.fs.current_dir.clone();
            Task::perform(
                crate::file_ops::read::load_directory(path.clone()),
                move |res| match res {
                    Ok(e) => {
                        cosmic::Action::App(Message::Nav(NavMsg::DirectoryLoaded(path.clone(), e)))
                    }
                    Err(e) => cosmic::Action::App(Message::Nav(NavMsg::DirectoryLoadFailed(e))),
                },
            )
        }
        NavMsg::DirectoryLoaded(path, entries) => {
            if path != app.fs.current_dir {
                return Task::none();
            }

            let new_entries = Arc::new(entries);
            let mut tasks = vec![app.sort_and_filter_entries(new_entries.clone())];

            let folder_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Root".into());
            let title_string = format!("{} — Stellarfiles", folder_name);

            tasks.push(Task::perform(
                async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    title_string
                },
                |t| cosmic::Action::App(Message::UI(UIMsg::UpdateWindowTitle(t))),
            ));

            if let Some(&(scroll, cursor)) = app.ui.path_history.get(&path) {
                app.ui.scroll_offset = scroll;
                app.ui.keyboard_cursor = cursor;
                let scroll_task = cosmic::iced::widget::scrollable::scroll_to(
                    cosmic::iced::widget::Id::new("main_scroll"),
                    cosmic::iced::widget::scrollable::AbsoluteOffset {
                        x: Some(0.0),
                        y: Some(scroll),
                    },
                );
                tasks.push(scroll_task);
            } else {
                let scroll_snap = cosmic::iced::widget::scrollable::snap_to(
                    cosmic::iced::widget::Id::new("main_scroll"),
                    cosmic::iced::widget::scrollable::RelativeOffset::START.into(),
                );
                tasks.push(scroll_snap);
            }

            let visual_paths: Vec<_> = new_entries
                .iter()
                .filter(|e| {
                    e.icon_name.contains("image")
                        || e.icon_name.contains("video")
                        || e.icon_name.contains("pdf")
                })
                .map(|e| e.path.clone())
                .collect();

            if !visual_paths.is_empty() {
                tasks.push(Task::perform(
                    crate::file_ops::read::generate_thumbnails(visual_paths),
                    |t| cosmic::Action::App(Message::Sys(SysMsg::ThumbnailsLoaded(t))),
                ));
            }

            app.ui.status_msg = String::new();
            tasks.push(Task::perform(async {}, |_| {
                cosmic::Action::App(Message::Sys(SysMsg::ListenFilesystem))
            }));

            Task::batch(tasks)
        }
        NavMsg::DirectoryLoadFailed(e) => {
            app.ui.status_msg = format!("Error loading directory: {}", e);
            app.fs.entries = Arc::new(Vec::new());
            app.fs.filtered_entries = Vec::new();
            Task::none()
        }
    }
}
