use crate::types::*;
use cosmic::app::Core;
use lru::LruCache;
use notify::RecommendedWatcher;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

pub struct FileSystemState {
    pub current_dir: PathBuf,
    pub history_back: Vec<PathBuf>,
    pub history_forward: Vec<PathBuf>,
    pub entries: Arc<Vec<FileEntry>>,
    pub filtered_entries: Vec<usize>,
    pub selected_files: HashSet<PathBuf>,
    pub system_disks: Vec<DiskInfo>,
    pub watcher: Option<RecommendedWatcher>,
    pub pending_fs_events: Vec<notify::Event>,
    pub fs_debounce_version: u64,
}

pub struct UIState {
    pub mode: Mode,
    pub view_mode: ViewMode,
    pub path_input: String,
    pub path_version: u64,
    pub search_visible: bool,
    pub search_regex: bool,
    pub search_query: String,
    pub search_version: u64,
    pub search_deep: bool,
    pub type_buffer: String,
    pub last_typing_time: Option<Instant>,
    pub show_hidden: bool,
    pub sidebar_visible: bool,
    pub sidebar_items: Vec<SidebarItem>,
    pub expanded_tree_nodes: HashSet<PathBuf>,
    pub tree_cache: HashMap<PathBuf, Vec<FileEntry>>,
    pub sort_key: SortKey,
    pub sort_direction: SortDirection,
    pub window_width: f32,
    pub window_height: f32,
    pub scroll_offset: f32,
    pub status_msg: String,
    pub context_menu: Option<(cosmic::iced::Point, ContextTarget)>,
    pub properties_modal: Option<PropertiesDetails>,
    pub properties_mode_input: String,
    pub destructive_action_modal: Option<(DestructiveAction, Vec<PathBuf>)>,
    pub rename_modal: Option<PathBuf>,
    pub rename_input: String,
    pub conflict_modal: Option<(PathBuf, PathBuf)>,
    pub show_new_modal: Option<&'static str>,
    pub new_input: String,
    pub compress_wizard: Option<PathBuf>,
    pub compress_name_input: String,
    pub compress_format: String,
    pub compress_level: String,
    pub save_dialog_input: String,
    pub batch_rename_modal: bool,
    pub batch_rename_pattern: String,
    pub batch_rename_replace: String,
    pub open_with_modal: Option<PathBuf>,
    pub open_with_cmd: String,
    pub path_history: LruCache<PathBuf, (f32, Option<usize>)>,
    pub hovered_row: Option<usize>,
    pub last_clicked_idx: Option<usize>,
    pub keyboard_cursor: Option<usize>,
    pub last_release_time: Option<Instant>,
    pub last_release_path: Option<PathBuf>,
    pub current_mouse: cosmic::iced::Point,
    pub selection_start: Option<cosmic::iced::Point>,
    pub item_drag_start: Option<cosmic::iced::Point>,
    pub is_dragging_marquee: bool,
    pub is_dragging_items: bool,
    pub thumbnails: RefCell<LruCache<PathBuf, cosmic::iced::widget::image::Handle>>,
}

impl UIState {
    pub fn close_modals(&mut self) {
        self.context_menu = None;
        self.properties_modal = None;
        self.destructive_action_modal = None;
        self.conflict_modal = None;
        self.show_new_modal = None;
        self.compress_wizard = None;
        self.rename_modal = None;
        self.batch_rename_modal = false;
        self.open_with_modal = None;
    }
}

pub struct TaskManager {
    pub active_tasks: HashMap<usize, BackgroundTask>,
    pub next_task_id: usize,
    pub clipboard: Vec<PathBuf>,
    pub clipboard_action: ClipboardAction,
    pub paste_state: Option<PasteState>,
    pub delete_state: Option<DeleteState>,
}

pub struct FileApp {
    pub core: Core,
    pub fs: FileSystemState,
    pub ui: UIState,
    pub tasks: TaskManager,
    pub ctrl_pressed: bool,
    pub shift_pressed: bool,
    pub progress_tx: async_channel::Sender<ProgressMsg>,
    pub progress_rx: async_channel::Receiver<ProgressMsg>,
    pub portal_tx: Option<async_channel::Sender<String>>,
    pub notify_rx: Option<async_channel::Receiver<notify::Result<notify::Event>>>,
}

impl cosmic::Application for FileApp {
    type Executor = cosmic::executor::Default;
    type Flags = AppFlags;
    type Message = Message;
    const APP_ID: &'static str = "stellarfiles";

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, cosmic::app::Task<Self::Message>) {
        let (progress_tx, progress_rx) = async_channel::unbounded();
        let (notify_tx, notify_rx) = async_channel::unbounded();
        let watcher = notify::recommended_watcher(move |res| {
            let _ = notify_tx.send_blocking(res);
        })
        .ok();
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"));
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));

        let app = FileApp {
            core,
            fs: FileSystemState {
                current_dir: flags.start_path.clone(),
                history_back: Vec::new(),
                history_forward: Vec::new(),
                entries: Arc::new(Vec::new()),
                filtered_entries: Vec::new(),
                selected_files: HashSet::new(),
                system_disks: crate::file_ops::read::load_disks(),
                watcher,
                pending_fs_events: Vec::new(),
                fs_debounce_version: 0,
            },
            ui: UIState {
                mode: flags.mode,
                view_mode: ViewMode::List,
                path_input: String::new(),
                path_version: 0,
                search_visible: false,
                search_regex: false,
                search_query: String::new(),
                search_version: 0,
                search_deep: false,
                type_buffer: String::new(),
                last_typing_time: None,
                show_hidden: false,
                sidebar_visible: true,
                sidebar_items: vec![
                    SidebarItem {
                        name: "Home".into(),
                        icon: "user-home-symbolic",
                        path: home.clone(),
                    },
                    SidebarItem {
                        name: "Documents".into(),
                        icon: "folder-documents-symbolic",
                        path: dirs::document_dir().unwrap_or_else(|| home.join("Documents")),
                    },
                    SidebarItem {
                        name: "Downloads".into(),
                        icon: "folder-download-symbolic",
                        path: dirs::download_dir().unwrap_or_else(|| home.join("Downloads")),
                    },
                    SidebarItem {
                        name: "Trash".into(),
                        icon: "user-trash-symbolic",
                        path: data_dir.join("Trash/files"),
                    },
                ],
                expanded_tree_nodes: HashSet::new(),
                tree_cache: HashMap::new(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                window_width: 1024.0,
                window_height: 768.0,
                scroll_offset: 0.0,
                status_msg: "Ready".into(),
                context_menu: None,
                properties_modal: None,
                properties_mode_input: String::new(),
                destructive_action_modal: None,
                rename_modal: None,
                rename_input: String::new(),
                conflict_modal: None,
                show_new_modal: None,
                new_input: String::new(),
                compress_wizard: None,
                compress_name_input: String::new(),
                compress_format: "zip".into(),
                compress_level: "Normal".into(),
                save_dialog_input: String::new(),
                batch_rename_modal: false,
                batch_rename_pattern: String::new(),
                batch_rename_replace: String::new(),
                open_with_modal: None,
                open_with_cmd: String::new(),
                path_history: LruCache::new(std::num::NonZeroUsize::new(50).unwrap()),
                hovered_row: None,
                last_clicked_idx: None,
                keyboard_cursor: None,
                last_release_time: None,
                last_release_path: None,
                current_mouse: cosmic::iced::Point::default(),
                selection_start: None,
                item_drag_start: None,
                is_dragging_marquee: false,
                is_dragging_items: false,
                thumbnails: RefCell::new(LruCache::new(std::num::NonZeroUsize::new(200).unwrap())),
            },
            tasks: TaskManager {
                active_tasks: HashMap::new(),
                next_task_id: 1,
                clipboard: Vec::new(),
                clipboard_action: ClipboardAction::Copy,
                paste_state: None,
                delete_state: None,
            },
            ctrl_pressed: false,
            shift_pressed: false,
            progress_tx,
            progress_rx: progress_rx.clone(),
            portal_tx: None,
            notify_rx: Some(notify_rx),
        };

        let start_dir = flags.start_path.clone();
        let init_task = cosmic::app::Task::perform(async move { start_dir }, |p| {
            cosmic::Action::App(Message::Nav(NavMsg::Navigate(p)))
        });
        let portal_task =
            cosmic::app::Task::perform(async move { flags.portal_rx.recv().await.unwrap() }, |r| {
                cosmic::Action::App(Message::Sys(SysMsg::PortalReq(r)))
            });
        let pump_task =
            cosmic::app::Task::perform(async move { progress_rx.recv().await.ok() }, |m| {
                cosmic::Action::App(Message::Task(TaskMsg::PumpProgress(m)))
            });

        (
            app,
            cosmic::app::Task::batch(vec![init_task, portal_task, pump_task]),
        )
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Self::Message> {
        use cosmic::iced::event::{self, Event};
        use cosmic::iced::{keyboard, mouse, window};
        let events = event::listen_with(|event, status, _| match event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if status == event::Status::Ignored =>
            {
                Some(Message::Sys(SysMsg::GlobalKeyPress(key, modifiers)))
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => Some(Message::Sys(
                SysMsg::ModifiersChanged(modifiers.control(), modifiers.shift()),
            )),
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::Sys(SysMsg::MouseMoved(position)))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Message::Sys(SysMsg::DragEnd))
            }
            Event::Window(window::Event::Resized(size)) => {
                Some(Message::Sys(SysMsg::WindowResized(size.width, size.height)))
            }
            Event::Window(window::Event::FileDropped(paths)) => {
                Some(Message::Sys(SysMsg::FilesDropped(paths)))
            }
            _ => None,
        });
        let mut subs = vec![events];
        if self.ui.is_dragging_marquee || self.ui.is_dragging_items {
            subs.push(
                cosmic::iced::time::every(std::time::Duration::from_millis(16))
                    .map(|_| Message::Sys(SysMsg::AutoScroll)),
            );
        }
        cosmic::iced::Subscription::batch(subs)
    }

    fn update(&mut self, message: Self::Message) -> cosmic::app::Task<Self::Message> {
        self.update_logic(message)
    }
    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        self.view_logic()
    }
}
