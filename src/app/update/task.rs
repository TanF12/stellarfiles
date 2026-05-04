use crate::app::state::FileApp;
use crate::types::*;
use cosmic::app::Task;
use rust_i18n::t;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub fn handle(app: &mut FileApp, msg: TaskMsg) -> Task<Message> {
    match msg {
        TaskMsg::ProcessNextPaste => {
            if let Some(state) = &mut app.tasks.paste_state {
                if let Some((src, dest)) = state.pending.last().cloned() {
                    if dest.starts_with(&src) {
                        app.ui.status_msg = "Cannot copy a directory into itself.".into();
                        state.error_count += 1;
                        state.pending.pop();
                        return Task::perform(async {}, |_| {
                            cosmic::Action::App(Message::Task(TaskMsg::ProcessNextPaste))
                        });
                    }
                    if dest.exists() && !state.overwrite_approved {
                        app.ui.conflict_modal = Some((src, dest));
                        return Task::none();
                    }

                    let is_cut = state.is_cut;
                    let overwrite = state.overwrite_approved;
                    state.overwrite_approved = false;

                    let id = app.tasks.next_task_id;
                    app.tasks.next_task_id += 1;
                    let cancel_token = Arc::new(AtomicBool::new(false));

                    app.tasks.active_tasks.insert(
                        id,
                        BackgroundTask {
                            title: if is_cut {
                                t!("Moving").to_string()
                            } else {
                                t!("Copying").to_string()
                            },
                            current_bytes: 0,
                            total_bytes: 0,
                            active_file: src
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into(),
                            cancel_token: cancel_token.clone(),
                        },
                    );

                    let tx = app.progress_tx.clone();
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || -> Result<(), String> {
                                let total_bytes = crate::file_ops::read::get_size(&src);
                                let _ = tx.send_blocking(ProgressMsg::Init { id, total_bytes });
                                if is_cut {
                                    if !overwrite && std::fs::rename(&src, &dest).is_ok() {
                                    } else if overwrite {
                                        std::fs::remove_dir_all(&dest)
                                            .or_else(|_| std::fs::remove_file(&dest))
                                            .ok();
                                        if std::fs::rename(&src, &dest).is_err() {
                                            crate::file_ops::copy::copy_recursive_safe(
                                                &src,
                                                &dest,
                                                &tx,
                                                true,
                                                id,
                                                &cancel_token,
                                            )
                                            .map_err(|e| e.to_string())?;
                                            std::fs::remove_dir_all(&src)
                                                .or_else(|_| std::fs::remove_file(&src))
                                                .ok();
                                        }
                                    } else {
                                        crate::file_ops::copy::copy_recursive_safe(
                                            &src,
                                            &dest,
                                            &tx,
                                            false,
                                            id,
                                            &cancel_token,
                                        )
                                        .map_err(|e| e.to_string())?;
                                        std::fs::remove_dir_all(&src)
                                            .or_else(|_| std::fs::remove_file(&src))
                                            .ok();
                                    }
                                } else {
                                    crate::file_ops::copy::copy_recursive_safe(
                                        &src,
                                        &dest,
                                        &tx,
                                        overwrite,
                                        id,
                                        &cancel_token,
                                    )
                                    .map_err(|e| e.to_string())?;
                                }
                                Ok(())
                            })
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                        },
                        move |r| {
                            cosmic::Action::App(Message::Task(TaskMsg::ProcessNextPasteResult(
                                id, r,
                            )))
                        },
                    )
                } else {
                    let final_msg = if state.error_count > 0 {
                        format!("Finished with {} error(s).", state.error_count)
                    } else {
                        "Paste complete.".into()
                    };
                    app.tasks.paste_state = None;
                    app.ui.status_msg = final_msg;
                    Task::perform(async {}, |_| {
                        cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
                    })
                }
            } else {
                Task::none()
            }
        }

        TaskMsg::ProcessNextPasteResult(id, res) => {
            app.tasks.active_tasks.remove(&id);
            if let Some(state) = &mut app.tasks.paste_state {
                state.pending.pop();
                match res {
                    Ok(_) => {
                        state.completed += 1;
                    }
                    Err(e) => {
                        if e.contains("Cancelled") {
                            app.ui.status_msg = "Paste cancelled.".into();
                            state.pending.clear();
                        } else {
                            app.ui.status_msg = format!("File Error: {}", e);
                            state.error_count += 1;
                        }
                    }
                }
            }
            Task::perform(async {}, |_| {
                cosmic::Action::App(Message::Task(TaskMsg::ProcessNextPaste))
            })
        }

        TaskMsg::ResolveConflict(choice) => {
            if let Some((_, _)) = app.ui.conflict_modal.take() {
                if let Some(true) = choice {
                    if let Some(state) = &mut app.tasks.paste_state {
                        state.overwrite_approved = true;
                    }
                    return app.update_logic(Message::Task(TaskMsg::ProcessNextPaste));
                } else if let Some(false) = choice {
                    if let Some(state) = &mut app.tasks.paste_state {
                        state.pending.pop();
                    }
                    return app.update_logic(Message::Task(TaskMsg::ProcessNextPaste));
                }
            }
            app.tasks.paste_state = None;
            app.ui.status_msg = "Paste cancelled.".into();
            Task::none()
        }

        TaskMsg::PumpProgress(msg_opt) => {
            if let Some(msg) = msg_opt {
                match msg {
                    ProgressMsg::Init { id, total_bytes } => {
                        if let Some(t) = app.tasks.active_tasks.get_mut(&id) {
                            t.total_bytes = total_bytes;
                        }
                    }
                    ProgressMsg::Update {
                        id,
                        bytes_chunk,
                        active_file,
                    } => {
                        if let Some(t) = app.tasks.active_tasks.get_mut(&id) {
                            t.current_bytes += bytes_chunk;
                            t.active_file = active_file;
                        }
                    }
                }
            }
            let rx = app.progress_rx.clone();
            Task::perform(async move { rx.recv().await.ok() }, |m| {
                cosmic::Action::App(Message::Task(TaskMsg::PumpProgress(m)))
            })
        }

        TaskMsg::CancelTask(id) => {
            if let Some(task) = app.tasks.active_tasks.get(&id) {
                task.cancel_token
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Task::none()
        }

        TaskMsg::ProcessNextDelete => {
            if let Some(state) = &mut app.tasks.delete_state {
                if let Some(target) = state.pending.pop() {
                    let is_perm = state.is_permanent;
                    let is_in_trash = app.is_trash_dir(&target);
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || -> Result<(), String> {
                                if is_perm {
                                    crate::file_ops::delete::safe_remove_recursively(&target)
                                        .map_err(|e| e.to_string())?;
                                    if is_in_trash
                                        && let Some(name) = target.file_name()
                                        && let Some(info_dir) =
                                            dirs::data_local_dir().map(|d| d.join("Trash/info"))
                                    {
                                        let info_file = info_dir
                                            .join(format!("{}.trashinfo", name.to_string_lossy()));
                                        let _ = std::fs::remove_file(info_file);
                                    }
                                } else {
                                    trash::delete(&target)
                                        .map_err(|e| format!("Trash error: {}", e))?;
                                }
                                Ok(())
                            })
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                        },
                        |res| {
                            cosmic::Action::App(Message::Task(TaskMsg::ProcessNextDeleteResult(
                                res,
                            )))
                        },
                    )
                } else {
                    app.ui.status_msg = if state.error_count > 0 {
                        format!("Finished with {} error(s).", state.error_count)
                    } else {
                        "Deletion complete.".into()
                    };
                    app.tasks.delete_state = None;
                    Task::perform(async {}, |_| {
                        cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
                    })
                }
            } else {
                Task::none()
            }
        }

        TaskMsg::ProcessNextDeleteResult(res) => {
            if let Some(state) = &mut app.tasks.delete_state {
                match res {
                    Ok(_) => state.completed += 1,
                    Err(e) => {
                        app.ui.status_msg = format!("Error: {}", e);
                        state.error_count += 1;
                    }
                }
            }
            Task::perform(async {}, |_| {
                cosmic::Action::App(Message::Task(TaskMsg::ProcessNextDelete))
            })
        }

        TaskMsg::CommandFinished(id, res) => {
            app.tasks.active_tasks.remove(&id);
            match res {
                Ok(m) => app.ui.status_msg = m,
                Err(e) => app.ui.status_msg = format!("Error: {}", e),
            };
            Task::perform(async {}, |_| {
                cosmic::Action::App(Message::Nav(NavMsg::RefreshCurrentDir))
            })
        }
    }
}
