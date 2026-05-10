use crate::app::state::FileApp;
use crate::math::sanitize_filename;
use crate::types::*;
use cosmic::app::Task;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub fn handle(app: &mut FileApp, msg: FileMsg) -> Task<Message> {
    match msg {
        FileMsg::ActionCopy => {
            app.ui.close_modals();
            if !app.fs.selected_files.is_empty() {
                app.tasks.clipboard = app.fs.selected_files.iter().cloned().collect();
                app.tasks.clipboard_action = ClipboardAction::Copy;
                app.ui.status_msg = format!("Copied {} items.", app.tasks.clipboard.len());
            }
            Task::none()
        }
        FileMsg::ActionCut => {
            app.ui.close_modals();
            if !app.fs.selected_files.is_empty() {
                app.tasks.clipboard = app.fs.selected_files.iter().cloned().collect();
                app.tasks.clipboard_action = ClipboardAction::Cut;
                app.ui.status_msg = format!("Cut {} items.", app.tasks.clipboard.len());
            }
            Task::none()
        }
        FileMsg::ActionPaste(dest_opt) => {
            app.ui.close_modals();
            if !app.tasks.clipboard.is_empty() {
                let dest_dir = dest_opt.unwrap_or_else(|| app.fs.current_dir.clone());
                let mut new_pending: Vec<(PathBuf, PathBuf)> = app
                    .tasks
                    .clipboard
                    .iter()
                    .map(|src| {
                        (
                            src.clone(),
                            dest_dir.join(src.file_name().unwrap_or_default()),
                        )
                    })
                    .collect();
                new_pending.reverse();
                if let Some(state) = &mut app.tasks.paste_state {
                    state.pending.extend(new_pending);
                    state.total += app.tasks.clipboard.len();
                    app.tasks.clipboard.clear();
                    Task::none()
                } else {
                    app.tasks.paste_state = Some(PasteState {
                        total: app.tasks.clipboard.len(),
                        completed: 0,
                        error_count: 0,
                        pending: new_pending,
                        is_cut: app.tasks.clipboard_action == ClipboardAction::Cut,
                        overwrite_approved: false,
                    });
                    app.tasks.clipboard.clear();
                    Task::perform(async {}, |_| {
                        cosmic::Action::App(Message::Task(TaskMsg::ProcessNextPaste))
                    })
                }
            } else {
                Task::none()
            }
        }
        FileMsg::ActionTrash => {
            let targets: Vec<_> = app.fs.selected_files.iter().cloned().collect();
            if targets.is_empty() {
                return Task::none();
            }
            app.ui.close_modals();
            if app.is_trash_dir(&app.fs.current_dir) {
                app.ui.destructive_action_modal = Some((DestructiveAction::Permadelete, targets));
                return Task::none();
            }
            app.tasks.delete_state = Some(DeleteState {
                total: targets.len(),
                completed: 0,
                error_count: 0,
                pending: targets,
                is_permanent: false,
            });
            Task::perform(async {}, |_| {
                cosmic::Action::App(Message::Task(TaskMsg::ProcessNextDelete))
            })
        }
        FileMsg::ActionRestore(path) => {
            app.ui.close_modals();
            let file_name = path.file_name().unwrap();
            let data_dir = dirs::data_local_dir().unwrap_or_default();
            let info_path = data_dir
                .join("Trash/info")
                .join(format!("{}.trashinfo", file_name.to_string_lossy()));

            if info_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&info_path) {
                    for line in content.lines() {
                        if let Some(orig_path_str) = line.strip_prefix("Path=") {
                            let orig_path = PathBuf::from(orig_path_str);
                            if let Some(parent) = orig_path.parent() {
                                std::fs::create_dir_all(parent).ok();
                            }
                            if std::fs::rename(&path, &orig_path).is_ok() {
                                std::fs::remove_file(&info_path).ok();
                                app.ui.status_msg = format!("Restored to {}", orig_path.display());
                            } else {
                                app.ui.status_msg = "Failed to move file during restore.".into();
                            }
                            break;
                        }
                    }
                }
            } else {
                app.ui.status_msg = "Trash info not found, cannot restore.".into();
            }
            Task::perform(async {}, |_| {
                cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
            })
        }
        FileMsg::ActionShred => {
            let targets: Vec<_> = app.fs.selected_files.iter().cloned().collect();
            if targets.is_empty() {
                return Task::none();
            }
            app.ui.close_modals();
            app.ui.destructive_action_modal = Some((DestructiveAction::Shred, targets));
            Task::none()
        }
        FileMsg::ConfirmDestructiveAction(action, targets) => {
            app.ui.destructive_action_modal = None;
            match action {
                DestructiveAction::Shred => {
                    Task::perform(crate::file_ops::shred::shred_paths(targets), |res| {
                        cosmic::Action::App(Message::Task(TaskMsg::CommandFinished(
                            0,
                            res.map_err(|e| e.to_string()),
                        )))
                    })
                }
                DestructiveAction::Permadelete => {
                    app.tasks.delete_state = Some(DeleteState {
                        total: targets.len(),
                        completed: 0,
                        error_count: 0,
                        pending: targets,
                        is_permanent: true,
                    });
                    Task::perform(async {}, |_| {
                        cosmic::Action::App(Message::Task(TaskMsg::ProcessNextDelete))
                    })
                }
            }
        }
        FileMsg::EmptyTrash => Task::perform(
            async move {
                tokio::task::spawn_blocking(move || -> Result<String, String> {
                    if let Ok(items) = trash::os_limited::list() {
                        let _ = trash::os_limited::purge_all(items);
                    }
                    if let Some(data_dir) = dirs::data_local_dir() {
                        for folder in ["Trash/files", "Trash/info"] {
                            let dir = data_dir.join(folder);
                            if let Ok(entries) = std::fs::read_dir(&dir) {
                                for entry in entries.flatten() {
                                    let _ = crate::file_ops::delete::safe_remove_recursively(
                                        &entry.path(),
                                    );
                                }
                            }
                        }
                    }
                    Ok("Trash has been natively and aggressively emptied.".to_string())
                })
                .await
                .unwrap_or_else(|e| Err(format!("Task failed: {}", e)))
            },
            |res| cosmic::Action::App(Message::Task(TaskMsg::CommandFinished(0, res))),
        ),
        FileMsg::ActionExtract(path) => {
            app.ui.close_modals();
            let id = app.tasks.next_task_id;
            app.tasks.next_task_id += 1;
            let cancel_token = Arc::new(AtomicBool::new(false));
            app.tasks.active_tasks.insert(
                id,
                BackgroundTask {
                    title: "Extracting".into(),
                    current_bytes: 0,
                    total_bytes: 0,
                    active_file: "Archive Contents".into(),
                    cancel_token: cancel_token.clone(),
                },
            );
            let tx = app.progress_tx.clone();
            Task::perform(
                crate::file_ops::archive::extract_archive(
                    path,
                    app.fs.current_dir.clone(),
                    tx,
                    id,
                    cancel_token,
                ),
                move |res| {
                    cosmic::Action::App(Message::Task(TaskMsg::CommandFinished(
                        id,
                        res.map_err(|e| e.to_string()),
                    )))
                },
            )
        }
        FileMsg::ActionOpenTerminal => {
            app.ui.close_modals();
            let current_dir = app.fs.current_dir.clone();
            Task::perform(
                async move {
                    let term =
                        std::env::var("TERMINAL").unwrap_or_else(|_| "xdg-terminal-exec".into());
                    if std::process::Command::new(&term)
                        .current_dir(&current_dir)
                        .spawn()
                        .is_err()
                    {
                        let _ = std::process::Command::new("x-terminal-emulator")
                            .current_dir(&current_dir)
                            .spawn();
                    }
                },
                |_| cosmic::Action::App(Message::NoOp),
            )
        }
        FileMsg::ActionSelectAll => {
            app.ui.close_modals();
            app.fs.selected_files = app.fs.entries.iter().map(|e| e.path.clone()).collect();
            Task::none()
        }
        FileMsg::ActionForkWindow => {
            app.ui.close_modals();
            if let Ok(exe) = std::env::current_exe() {
                let mut cmd = std::process::Command::new(exe);
                cmd.current_dir(&app.fs.current_dir);
                let _ = cmd.spawn();
            }
            Task::none()
        }
        FileMsg::ConfirmRename => {
            if let Some(target) = app.ui.rename_modal.take() {
                match sanitize_filename(&app.ui.rename_input) {
                    Ok(safe_name) => {
                        let new_path = target.with_file_name(safe_name);
                        if let Err(e) = std::fs::rename(&target, &new_path) {
                            app.ui.status_msg = format!("Failed to rename: {}", e);
                        }
                    }
                    Err(e) => {
                        app.ui.status_msg = e.to_string();
                        return Task::none();
                    }
                }
                Task::perform(async {}, |_| {
                    cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
                })
            } else {
                Task::none()
            }
        }
        FileMsg::ConfirmNewFolder => {
            if app.ui.show_new_modal.take().is_some() {
                match sanitize_filename(&app.ui.new_input) {
                    Ok(safe_name) => {
                        let target = app.fs.current_dir.join(safe_name);
                        if let Err(e) = std::fs::create_dir_all(target) {
                            app.ui.status_msg = format!("Error: {}", e);
                        }
                    }
                    Err(e) => {
                        app.ui.status_msg = e.to_string();
                        return Task::none();
                    }
                }
                Task::perform(async {}, |_| {
                    cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
                })
            } else {
                Task::none()
            }
        }
        FileMsg::ConfirmNewFile => {
            if app.ui.show_new_modal.take().is_some() {
                match sanitize_filename(&app.ui.new_input) {
                    Ok(safe_name) => {
                        let target = app.fs.current_dir.join(safe_name);
                        if let Err(e) = std::fs::File::create(target) {
                            app.ui.status_msg = format!("Error: {}", e);
                        }
                    }
                    Err(e) => {
                        app.ui.status_msg = e.to_string();
                        return Task::none();
                    }
                }
                Task::perform(async {}, |_| {
                    cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
                })
            } else {
                Task::none()
            }
        }
        FileMsg::ConfirmBatchRename => {
            app.ui.close_modals();
            let targets: Vec<_> = app.fs.selected_files.iter().cloned().collect();
            if !targets.is_empty() {
                let pattern = app.ui.batch_rename_pattern.clone();
                let replace = app.ui.batch_rename_replace.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::file_ops::rename::execute_batch_rename(
                                &targets, &pattern, &replace,
                            )
                        })
                        .await
                        .unwrap_or(Err("Thread crashed".into()))
                    },
                    |res| {
                        let msg = match res {
                            Ok(c) => format!("Successfully renamed {} items.", c),
                            Err(e) => format!("Batch rename failed: {}", e),
                        };
                        cosmic::Action::App(Message::Task(TaskMsg::CommandFinished(0, Ok(msg))))
                    },
                )
            } else {
                Task::none()
            }
        }
        FileMsg::ConfirmCompress => {
            if let Some(target) = app.ui.compress_wizard.take() {
                match sanitize_filename(&app.ui.compress_name_input) {
                    Ok(safe_name) => {
                        let name = safe_name;

                        let id = app.tasks.next_task_id;
                        app.tasks.next_task_id += 1;
                        let cancel_token = Arc::new(AtomicBool::new(false));
                        app.tasks.active_tasks.insert(
                            id,
                            BackgroundTask {
                                title: "Compressing".into(),
                                current_bytes: 0,
                                total_bytes: 0,
                                active_file: target
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into(),
                                cancel_token: cancel_token.clone(),
                            },
                        );
                        let tx = app.progress_tx.clone();
                        let paths: Vec<std::path::PathBuf> =
                            app.fs.selected_files.iter().cloned().collect();
                        Task::perform(
                            crate::file_ops::archive::compress_path(
                                crate::file_ops::archive::CompressOptions {
                                    paths,
                                    dest_name: name.to_string(),
                                    format: app.ui.compress_format.clone(),
                                    level: app.ui.compress_level.clone(),
                                    current_dir: app.fs.current_dir.clone(),
                                    tx,
                                    id,
                                    cancel: cancel_token,
                                },
                            ),
                            move |res| {
                                cosmic::Action::App(Message::Task(TaskMsg::CommandFinished(
                                    id,
                                    res.map_err(|e| e.to_string()),
                                )))
                            },
                        )
                    }
                    Err(e) => {
                        app.ui.status_msg = e.to_string();
                        Task::none()
                    }
                }
            } else {
                Task::none()
            }
        }
        FileMsg::ConfirmPermissionsChange => {
            if let Some(props) = app.ui.properties_modal.as_ref() {
                let path = PathBuf::from(&props.path);
                if let Ok(octal) = u32::from_str_radix(&app.ui.properties_mode_input, 8) {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ =
                            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(octal));
                        app.ui.close_modals();
                        return Task::perform(async {}, |_| {
                            cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
                        });
                    }
                }
            }
            Task::none()
        }
        FileMsg::ExecuteOpenWith => {
            if let Some(target) = app.ui.open_with_modal.take() {
                let cmd = app.ui.open_with_cmd.clone();
                let dir = app.fs.current_dir.clone();
                app.ui.close_modals();
                Task::perform(
                    async move {
                        let parts: Vec<&str> = cmd.split_whitespace().collect();
                        if !parts.is_empty() {
                            let _ = std::process::Command::new(parts[0])
                                .args(&parts[1..])
                                .arg(target)
                                .current_dir(&dir)
                                .spawn();
                        }
                    },
                    |_| cosmic::Action::App(Message::NoOp),
                )
            } else {
                Task::none()
            }
        }
    }
}
