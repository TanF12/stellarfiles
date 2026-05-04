use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub const UI_TOP_BAR_HEIGHT: f32 = 60.0;
pub const UI_HEADER_HEIGHT: f32 = 40.0;
pub const UI_ROW_HEIGHT: f32 = 44.0;
pub const UI_SIDEBAR_WIDTH: f32 = 240.0;
pub const UI_SIDEBAR_PADDING_TOP: f32 = 12.0;

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum Mode {
    Manager,
    Open,
    Save(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ViewMode {
    List,
    Grid,
}

#[derive(Clone, Debug)]
pub struct AppFlags {
    pub mode: Mode,
    pub portal_rx: async_channel::Receiver<PortalRequest>,
}

#[derive(Clone, Debug)]
pub enum PortalRequest {
    OpenFile(async_channel::Sender<String>),
    SaveFile(String, async_channel::Sender<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClipboardAction {
    Copy,
    Cut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DestructiveAction {
    Permadelete,
    Shred,
}

#[derive(Clone, Debug)]
pub struct DiskInfo {
    pub name: Arc<str>,
    pub mount_point: PathBuf,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: Arc<str>,
    pub grid_name: Arc<str>,
    pub list_name: Arc<str>,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified: std::time::SystemTime,
    pub size_str: Arc<str>,
    pub modified_str: Arc<str>,
    pub file_type_str: Arc<str>,
    pub icon_name: &'static str,
}

#[derive(Clone, Debug)]
pub struct SidebarItem {
    pub name: String,
    pub icon: &'static str,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ContextTarget {
    File(PathBuf, &'static str),
    EmptySpace,
}

#[derive(Clone, Debug)]
pub struct PropertiesDetails {
    pub path: String,
    pub file_type: String,
    pub size_str: String,
    pub created: String,
    pub modified: String,
    pub accessed: String,
    pub owner: u32,
    pub group: u32,
    pub mode_octal: u32,
    pub items_count: Option<usize>,
    pub icon: &'static str,
}

#[derive(Clone, Debug)]
pub struct PasteState {
    pub total: usize,
    pub completed: usize,
    pub error_count: usize,
    pub pending: Vec<(PathBuf, PathBuf)>,
    pub is_cut: bool,
    pub overwrite_approved: bool,
}

#[derive(Clone, Debug)]
pub struct DeleteState {
    pub total: usize,
    pub completed: usize,
    pub error_count: usize,
    pub pending: Vec<PathBuf>,
    pub is_permanent: bool,
}

#[derive(Clone, Debug)]
pub struct BackgroundTask {
    pub title: String,
    pub current_bytes: u64,
    pub total_bytes: u64,
    pub active_file: String,
    pub cancel_token: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
pub enum ProgressMsg {
    Init {
        id: usize,
        total_bytes: u64,
    },
    Update {
        id: usize,
        bytes_chunk: u64,
        active_file: String,
    },
}

#[derive(Clone, Debug, PartialEq, Copy)]
pub enum SortKey {
    Name,
    Size,
    Modified,
}

#[derive(Clone, Debug, PartialEq, Copy)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Debug)]
pub enum NavMsg {
    Navigate(PathBuf),
    NavigateUp,
    NavigateBack,
    NavigateForward,
    NavigateToInput(String),
    PathInputChanged(String),
    PathInputDebounced(String, u64),
    RefreshCurrentDir,
    DirectoryLoaded(PathBuf, Vec<FileEntry>),
    DirectoryLoadFailed(String),
}

#[derive(Clone, Debug)]
pub enum FileMsg {
    ActionCopy,
    ActionCut,
    ActionPaste(Option<PathBuf>),
    ActionTrash,
    ActionShred,
    ActionRestore(PathBuf),
    ConfirmDestructiveAction(DestructiveAction, Vec<PathBuf>),
    EmptyTrash,
    ActionExtract(PathBuf),
    ActionOpenTerminal,
    ActionSelectAll,
    ActionForkWindow,
    ConfirmRename,
    ConfirmNewFolder,
    ConfirmNewFile,
    ConfirmCompress,
    ConfirmBatchRename,
    ConfirmPermissionsChange,
    ExecuteOpenWith,
}

#[derive(Clone, Debug)]
pub enum UIMsg {
    CloseAllModals,
    ToggleSidebar,
    ToggleSidebarNode(PathBuf),
    SidebarNodeLoaded(PathBuf, Vec<FileEntry>),
    ToggleHiddenFiles,
    SortBy(SortKey),
    ToggleSearch,
    CloseSearch,
    ToggleRegex,
    SearchChanged(String),
    SearchExecute(u64),
    SearchCompleted(Arc<Vec<FileEntry>>, Vec<usize>),
    ToggleDeepSearch,
    OpenBatchRenameModal,
    BatchRenamePatternChanged(String),
    BatchRenameReplaceChanged(String),
    HoverRow(usize, bool),
    ItemPressed(usize, PathBuf),
    ItemReleased(PathBuf),
    OpenProperties(PathBuf, &'static str),
    PropertiesLoaded(PropertiesDetails),
    PropertiesModeChanged(String),
    OpenRenameModal(PathBuf),
    RenameInputChanged(String),
    OpenNewFolderModal,
    OpenNewFileModal,
    NewInputChanged(String),
    OpenCompressWizard(PathBuf),
    CompressNameChanged(String),
    CompressFormatChanged(String),
    CompressLevelChanged(String),
    OpenWithModal(PathBuf),
    OpenWithCmdChanged(String),
    DialogConfirm,
    DialogCancel,
    DialogSaveNameChanged(String),
    ToggleViewMode,
}

#[derive(Clone, Debug)]
pub enum TaskMsg {
    ProcessNextPaste,
    ProcessNextPasteResult(usize, Result<(), String>),
    ResolveConflict(Option<bool>),
    PumpProgress(Option<ProgressMsg>),
    CancelTask(usize),
    ProcessNextDelete,
    ProcessNextDeleteResult(Result<(), String>),
    CommandFinished(usize, Result<String, String>),
}

#[derive(Clone, Debug)]
pub enum SysMsg {
    ListenFilesystem,
    FilesystemDelta(notify::Event),
    ProcessDebouncedFsEvents(u64),
    ThumbnailsLoaded(Vec<(PathBuf, u32, u32, Vec<u8>)>),
    ActivateFile(PathBuf),
    RightClickFile(PathBuf, &'static str),
    RightClickSpace,
    ClearContextMenu,
    ModifiersChanged(bool, bool),
    MouseMoved(cosmic::iced::Point),
    StartMarquee,
    AutoScroll,
    GlobalKeyPress(
        cosmic::iced::keyboard::Key,
        cosmic::iced::keyboard::Modifiers,
    ),
    DragEnd,
    Scrolled(cosmic::iced::widget::scrollable::Viewport),
    WindowResized(f32, f32),
    MoveCursor(isize),
    ActivateCursor,
    FilesDropped(Vec<PathBuf>),
    PortalReq(PortalRequest),
}

#[derive(Clone, Debug)]
pub enum Message {
    Nav(NavMsg),
    File(FileMsg),
    UI(UIMsg),
    Task(TaskMsg),
    Sys(SysMsg),
    NoOp,
    ExitApp,
}
