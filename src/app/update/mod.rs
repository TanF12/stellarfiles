pub mod file;
pub mod nav;
pub mod sys;
pub mod task;
pub mod ui;

use crate::app::state::FileApp;
use crate::types::{Message, SortDirection, SortKey};
use cosmic::app::Task;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use rayon::prelude::*;
use regex::Regex;
use std::sync::Arc;

impl FileApp {
    pub fn update_logic(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Nav(msg) => nav::handle(self, msg),
            Message::File(msg) => file::handle(self, msg),
            Message::UI(msg) => ui::handle(self, msg),
            Message::Task(msg) => task::handle(self, msg),
            Message::Sys(msg) => sys::handle(self, msg),
            Message::NoOp => Task::none(),
            Message::ExitApp => {
                std::process::exit(0);
            }
        }
    }

    pub fn is_trash_dir(&self, path: &std::path::Path) -> bool {
        if let Some(trash_dir) = dirs::data_local_dir().map(|d| d.join("Trash/files")) {
            path.starts_with(&trash_dir)
        } else {
            false
        }
    }

    pub fn sort_and_filter_entries(
        &self,
        entries_ref: Arc<Vec<crate::types::FileEntry>>,
    ) -> Task<Message> {
        let query = self.ui.search_query.clone();
        let is_regex = self.ui.search_regex;
        let search_deep = self.ui.search_deep;
        let show_hidden = self.ui.show_hidden;
        let sort_key = self.ui.sort_key;
        let sort_direction = self.ui.sort_direction;
        let root_dir = self.fs.current_dir.clone();

        let displaying_search = !query.trim().is_empty();

        if !displaying_search {
            let needs_refresh = entries_ref
                .iter()
                .any(|e| e.path.parent() != Some(&root_dir));
            if needs_refresh {
                return Task::perform(async {}, |_| {
                    cosmic::Action::App(Message::Nav(crate::types::NavMsg::RefreshCurrentDir))
                });
            }
        }

        Task::perform(
            tokio::task::spawn_blocking(move || {
                let matcher = SkimMatcherV2::default();
                let rx = if is_regex && displaying_search {
                    Regex::new(&query).ok()
                } else {
                    None
                };

                let mut final_entries = entries_ref.clone();

                if search_deep && displaying_search {
                    let mut active_entries = Vec::new();
                    for entry in walkdir::WalkDir::new(&root_dir)
                        .into_iter()
                        .filter_map(|e| e.ok())
                    {
                        let name = entry.file_name().to_string_lossy();
                        if !show_hidden && name.starts_with('.') {
                            continue;
                        }

                        let is_match = if let Some(ref r) = rx {
                            r.is_match(&name)
                        } else {
                            matcher.fuzzy_match(&name, &query).is_some()
                        };

                        if is_match && let Ok(meta) = entry.metadata() {
                            active_entries.push(crate::file_ops::read::create_entry(
                                entry.path().to_path_buf(),
                                meta,
                            ));
                        }
                    }
                    final_entries = Arc::new(active_entries);
                }

                let mut filtered: Vec<(i64, usize)> = final_entries
                    .par_iter()
                    .enumerate()
                    .filter_map(|(idx, e)| {
                        if !show_hidden && e.name.starts_with('.') {
                            return None;
                        }
                        if !displaying_search {
                            return Some((0, idx));
                        }

                        if search_deep {
                            if rx.is_some() {
                                return Some((100, idx));
                            } else if let Some(score) = matcher.fuzzy_match(&e.name, &query) {
                                return Some((score, idx));
                            }
                            Some((0, idx))
                        } else {
                            if let Some(ref r) = rx {
                                if r.is_match(&e.name) {
                                    return Some((100, idx));
                                }
                            } else if let Some(score) = matcher.fuzzy_match(&e.name, &query) {
                                return Some((score, idx));
                            }
                            None
                        }
                    })
                    .collect();

                filtered.par_sort_by(|a, b| {
                    let ea = &final_entries[a.1];
                    let eb = &final_entries[b.1];
                    let dir_cmp = eb.is_dir.cmp(&ea.is_dir);
                    if dir_cmp != std::cmp::Ordering::Equal {
                        return dir_cmp;
                    }
                    let primary = match sort_key {
                        SortKey::Name => ea.name.cmp(&eb.name),
                        SortKey::Size => ea.size_bytes.cmp(&eb.size_bytes),
                        SortKey::Modified => ea.modified.cmp(&eb.modified),
                    };
                    let sorted = if sort_direction == SortDirection::Asc {
                        primary
                    } else {
                        primary.reverse()
                    };
                    if !displaying_search {
                        sorted
                    } else {
                        b.0.cmp(&a.0).then(sorted)
                    }
                });

                let indices = filtered.into_iter().map(|(_, idx)| idx).collect();
                (final_entries, indices)
            }),
            |res| {
                if let Ok((entries, indices)) = res {
                    cosmic::Action::App(Message::UI(crate::types::UIMsg::SearchCompleted(
                        entries, indices,
                    )))
                } else {
                    cosmic::Action::App(Message::NoOp)
                }
            },
        )
    }
}
