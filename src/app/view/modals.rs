use crate::app::state::FileApp;
use crate::types::*;
use cosmic::iced::widget::{Stack, opaque, pick_list};
use cosmic::iced::{Alignment, Length};
use cosmic::theme::Button as ButtonClass;
use cosmic::widget::{
    Space, button, column, container as iced_container, icon, mouse_area, row, text, text_input,
};
use cosmic::{Element, Theme};
use rust_i18n::t;

fn modal_overlay<'a>(content: Element<'a, Message>, on_dismiss: Message) -> Element<'a, Message> {
    let dismiss_bg =
        mouse_area(Space::new().width(Length::Fill).height(Length::Fill)).on_press(on_dismiss);
    let bg_overlay = opaque(
        iced_container(dismiss_bg)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &Theme| {
                let mut bg = cosmic::iced::Color::BLACK;
                bg.a = 0.65;
                iced_container::Style {
                    background: Some(cosmic::iced::Background::Color(bg)),
                    ..Default::default()
                }
            }),
    );
    let modal_layer = iced_container(opaque(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);
    Stack::new().push(bg_overlay).push(modal_layer).into()
}

pub fn render(app: &FileApp) -> Option<Element<'_, Message>> {
    if let Some((pos, target)) = &app.ui.context_menu {
        let mut clamped_x = pos.x;
        let mut clamped_y = pos.y;
        if clamped_x + 200.0 > app.ui.window_width {
            clamped_x -= 200.0;
        }
        if clamped_y + 350.0 > app.ui.window_height {
            clamped_y -= 350.0;
        }
        let separator = || -> Element<'_, Message> {
            iced_container(Space::new())
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(|_t: &Theme| iced_container::Style {
                    background: Some(cosmic::iced::Background::Color(
                        cosmic::iced::Color::from_rgba(0.5, 0.5, 0.5, 0.2),
                    )),
                    ..Default::default()
                })
                .into()
        };
        let btn = |label: &str, msg: Message| -> Element<'_, Message> {
            button::custom(text(t!(label).to_string()).size(14.0))
                .class(ButtonClass::Text)
                .on_press(msg)
                .width(Length::Fill)
                .padding(6)
                .into()
        };
        let mut col = column![].spacing(2);

        let in_trash = app.is_trash_dir(&app.fs.current_dir);

        match target {
            ContextTarget::File(path, icon) => {
                if in_trash {
                    col = col.push(btn(
                        "Restore",
                        Message::File(FileMsg::ActionRestore(path.clone())),
                    ));
                    col = col.push(separator());
                    col = col.push(btn(
                        "Permanently Delete",
                        Message::File(FileMsg::ActionTrash),
                    ));
                    col = col.push(separator());
                    col = col.push(btn(
                        "Properties",
                        Message::UI(UIMsg::OpenProperties(path.clone(), icon)),
                    ));
                } else {
                    let ext = path
                        .extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase();
                    let is_archive =
                        ["zip", "tar", "gz", "tgz", "7z", "zst", "bz3"].contains(&ext.as_str());
                    col = col.push(btn(
                        "Open",
                        Message::Sys(SysMsg::ActivateFile(path.clone())),
                    ));
                    col = col.push(btn(
                        "Open With...",
                        Message::UI(UIMsg::OpenWithModal(path.clone())),
                    ));
                    col = col.push(separator());
                    col = col.push(btn(
                        "Rename",
                        Message::UI(UIMsg::OpenRenameModal(path.clone())),
                    ));
                    col = col.push(btn("Copy", Message::File(FileMsg::ActionCopy)));
                    col = col.push(btn("Cut", Message::File(FileMsg::ActionCut)));
                    if is_archive {
                        col = col.push(btn(
                            "Extract Here",
                            Message::File(FileMsg::ActionExtract(path.clone())),
                        ));
                    }
                    col = col.push(btn(
                        "Compress...",
                        Message::UI(UIMsg::OpenCompressWizard(path.clone())),
                    ));
                    col = col.push(separator());
                    col = col.push(btn("Move to Trash", Message::File(FileMsg::ActionTrash)));
                    col = col.push(btn("Shred", Message::File(FileMsg::ActionShred)));
                    col = col.push(separator());
                    col = col.push(btn(
                        "Properties",
                        Message::UI(UIMsg::OpenProperties(path.clone(), icon)),
                    ));
                }
            }
            ContextTarget::EmptySpace => {
                if !in_trash {
                    col = col.push(btn("New Folder", Message::UI(UIMsg::OpenNewFolderModal)));
                    col = col.push(btn("New Document", Message::UI(UIMsg::OpenNewFileModal)));
                    col = col.push(separator());
                    col = col.push(btn(
                        "Batch Rename...",
                        Message::UI(UIMsg::OpenBatchRenameModal),
                    ));
                    col = col.push(btn("Paste", Message::File(FileMsg::ActionPaste(None))));
                }
                col = col.push(btn("Select All", Message::File(FileMsg::ActionSelectAll)));
                col = col.push(separator());
                col = col.push(btn(
                    "Open in Terminal",
                    Message::File(FileMsg::ActionOpenTerminal),
                ));
                col = col.push(btn(
                    "Open New Window",
                    Message::File(FileMsg::ActionForkWindow),
                ));
            }
        }
        let menu = iced_container(col)
            .style(FileApp::popover_style)
            .padding(6)
            .width(Length::Fixed(200.0));
        let menu_layer = iced_container(column![
            Space::new().height(Length::Fixed(clamped_y)),
            row![Space::new().width(Length::Fixed(clamped_x)), opaque(menu)]
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Start)
        .align_y(Alignment::Start);
        let dismiss = mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
            .on_press(Message::Sys(SysMsg::ClearContextMenu))
            .on_right_press(Message::Sys(SysMsg::ClearContextMenu));
        return Some(
            Stack::new()
                .push(opaque(
                    iced_container(dismiss)
                        .width(Length::Fill)
                        .height(Length::Fill),
                ))
                .push(menu_layer)
                .into(),
        );
    }

    if app.ui.batch_rename_modal {
        let modal = iced_container(
            column![
                text(t!("Batch Rename").to_string()).size(20.0),
                text("Pattern (Regex):").size(14.0),
                text_input("e.g. ^Image_(.*)\\.jpg$", &app.ui.batch_rename_pattern)
                    .on_input(|s| Message::UI(UIMsg::BatchRenamePatternChanged(s))),
                text("Replace (Use #SEQ for numbers, $1 for capture groups):").size(14.0),
                text_input("e.g. Vacation_#SEQ.jpg", &app.ui.batch_rename_replace)
                    .on_input(|s| Message::UI(UIMsg::BatchRenameReplaceChanged(s))),
                row![
                    button::custom(
                        text(t!("Cancel").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Standard)
                    .on_press(Message::UI(UIMsg::CloseAllModals))
                    .width(Length::FillPortion(1)),
                    button::custom(
                        text(t!("Confirm").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Suggested)
                    .on_press(Message::File(FileMsg::ConfirmBatchRename))
                    .width(Length::FillPortion(1))
                ]
                .spacing(10)
            ]
            .spacing(16),
        )
        .style(FileApp::popover_style)
        .padding(24)
        .width(Length::Fixed(450.0));
        return Some(modal_overlay(
            modal.into(),
            Message::UI(UIMsg::CloseAllModals),
        ));
    }

    if let Some(target) = app.ui.show_new_modal {
        let is_folder = target == "Folder";
        let modal = iced_container(
            column![
                text(format!("{} {}", t!("New"), t!(target))).size(20.0),
                text_input(t!("Name").to_string(), &app.ui.new_input)
                    .on_input(|s| Message::UI(UIMsg::NewInputChanged(s)))
                    .on_submit(move |_| if is_folder {
                        Message::File(FileMsg::ConfirmNewFolder)
                    } else {
                        Message::File(FileMsg::ConfirmNewFile)
                    }),
                row![
                    button::custom(
                        text(t!("Cancel").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Standard)
                    .on_press(Message::UI(UIMsg::CloseAllModals))
                    .width(Length::FillPortion(1)),
                    button::custom(
                        text(t!("Confirm").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Suggested)
                    .on_press(if is_folder {
                        Message::File(FileMsg::ConfirmNewFolder)
                    } else {
                        Message::File(FileMsg::ConfirmNewFile)
                    })
                    .width(Length::FillPortion(1))
                ]
                .spacing(10)
            ]
            .spacing(16),
        )
        .style(FileApp::popover_style)
        .padding(24)
        .width(Length::Fixed(400.0));
        return Some(modal_overlay(
            modal.into(),
            Message::UI(UIMsg::CloseAllModals),
        ));
    }

    if let Some(props) = &app.ui.properties_modal {
        let modal = iced_container(
            column![
                text(t!("Properties").to_string())
                    .width(Length::Fill)
                    .size(20.0),
                Space::new().height(Length::Fixed(8.0)),
                row![
                    icon::from_name(props.icon).size(48),
                    text(&props.path).size(14.0)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                Space::new().height(Length::Fixed(16.0)),
                text(format!("{}: {}", t!("Type"), props.file_type)).size(13.0),
                text(format!("{}: {}", t!("Size"), props.size_str)).size(13.0),
                text(format!(
                    "Contains: {}",
                    props
                        .items_count
                        .map_or("--".to_string(), |c| format!("{} Items", c))
                ))
                .size(13.0),
                Space::new().height(Length::Fixed(8.0)),
                text(format!("Created: {}", props.created)).size(13.0),
                text(format!("{}: {}", t!("Modified"), props.modified)).size(13.0),
                text(format!("Accessed: {}", props.accessed)).size(13.0),
                text(format!(
                    "Owner (UID): {} / Group (GID): {}",
                    props.owner, props.group
                ))
                .size(13.0),
                row![
                    text("Octal Mode:").size(13.0),
                    text_input("", &app.ui.properties_mode_input)
                        .on_input(|s| Message::UI(UIMsg::PropertiesModeChanged(s)))
                        .on_submit(|_| Message::File(FileMsg::ConfirmPermissionsChange))
                        .width(Length::Fixed(80.0)),
                    button::custom(text("Apply").size(13.0))
                        .class(ButtonClass::Suggested)
                        .on_press(Message::File(FileMsg::ConfirmPermissionsChange))
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                Space::new().height(Length::Fixed(16.0)),
                button::custom(text("Close").width(Length::Fill).align_x(Alignment::Center))
                    .class(ButtonClass::Standard)
                    .on_press(Message::UI(UIMsg::CloseAllModals))
                    .width(Length::Fill)
            ]
            .spacing(8),
        )
        .style(FileApp::popover_style)
        .padding(24)
        .width(Length::Fixed(400.0));
        return Some(modal_overlay(
            modal.into(),
            Message::UI(UIMsg::CloseAllModals),
        ));
    }

    if let Some((_, dest)) = &app.ui.conflict_modal {
        let modal = iced_container(
            column![
                text(t!("File Conflict").to_string()).size(20.0),
                text(format!(
                    "An item named '{}' already exists.",
                    dest.file_name().unwrap_or_default().to_string_lossy()
                )),
                row![
                    button::custom(
                        text(t!("Cancel").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Standard)
                    .on_press(Message::Task(TaskMsg::ResolveConflict(None)))
                    .width(Length::FillPortion(1)),
                    button::custom(
                        text(t!("Skip").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Standard)
                    .on_press(Message::Task(TaskMsg::ResolveConflict(Some(false))))
                    .width(Length::FillPortion(1)),
                    button::custom(
                        text(t!("Replace").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Destructive)
                    .on_press(Message::Task(TaskMsg::ResolveConflict(Some(true))))
                    .width(Length::FillPortion(1))
                ]
                .spacing(10)
            ]
            .spacing(16),
        )
        .style(FileApp::popover_style)
        .padding(24)
        .width(Length::Fixed(450.0));
        return Some(modal_overlay(
            modal.into(),
            Message::Task(TaskMsg::ResolveConflict(None)),
        ));
    }

    if let Some((action, targets)) = &app.ui.destructive_action_modal {
        let action_str = match action {
            DestructiveAction::Permadelete => "permanently delete",
            DestructiveAction::Shred => "securely shred",
        };
        let modal = iced_container(
            column![
                text(t!("Confirm Action").to_string()).size(20.0),
                text(format!(
                    "Are you sure you want to {} the selected {} item(s)?",
                    t!(action_str),
                    targets.len()
                )),
                row![
                    button::custom(
                        text(t!("Cancel").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Standard)
                    .on_press(Message::UI(UIMsg::CloseAllModals))
                    .width(Length::FillPortion(1)),
                    button::custom(
                        text(t!("Confirm").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Destructive)
                    .on_press(Message::File(FileMsg::ConfirmDestructiveAction(
                        action.clone(),
                        targets.clone()
                    )))
                    .width(Length::FillPortion(1))
                ]
                .spacing(10)
            ]
            .spacing(16),
        )
        .style(FileApp::popover_style)
        .padding(24)
        .width(Length::Fixed(450.0));
        return Some(modal_overlay(
            modal.into(),
            Message::UI(UIMsg::CloseAllModals),
        ));
    }

    if app.ui.rename_modal.is_some() {
        let modal = iced_container(
            column![
                text(t!("Rename").to_string()).size(20.0),
                text_input(t!("Name").to_string(), &app.ui.rename_input)
                    .on_input(|s| Message::UI(UIMsg::RenameInputChanged(s)))
                    .on_submit(|_| Message::File(FileMsg::ConfirmRename)),
                row![
                    button::custom(
                        text(t!("Cancel").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Standard)
                    .on_press(Message::UI(UIMsg::CloseAllModals))
                    .width(Length::FillPortion(1)),
                    button::custom(
                        text(t!("Confirm").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Suggested)
                    .on_press(Message::File(FileMsg::ConfirmRename))
                    .width(Length::FillPortion(1))
                ]
                .spacing(10)
            ]
            .spacing(16),
        )
        .style(FileApp::popover_style)
        .padding(24)
        .width(Length::Fixed(400.0));
        return Some(modal_overlay(
            modal.into(),
            Message::UI(UIMsg::CloseAllModals),
        ));
    }

    if app.ui.open_with_modal.is_some() {
        let modal = iced_container(
            column![
                text(t!("Open With...").to_string()).size(20.0),
                text("Command (e.g., 'code', 'gimp'):").size(14.0),
                text_input("", &app.ui.open_with_cmd)
                    .on_input(|s| Message::UI(UIMsg::OpenWithCmdChanged(s)))
                    .on_submit(|_| Message::File(FileMsg::ExecuteOpenWith)),
                row![
                    button::custom(
                        text(t!("Cancel").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Standard)
                    .on_press(Message::UI(UIMsg::CloseAllModals))
                    .width(Length::FillPortion(1)),
                    button::custom(
                        text(t!("Open").to_string())
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Suggested)
                    .on_press(Message::File(FileMsg::ExecuteOpenWith))
                    .width(Length::FillPortion(1))
                ]
                .spacing(10)
            ]
            .spacing(16),
        )
        .style(FileApp::popover_style)
        .padding(24)
        .width(Length::Fixed(400.0));
        return Some(modal_overlay(
            modal.into(),
            Message::UI(UIMsg::CloseAllModals),
        ));
    }

    if app.ui.compress_wizard.is_some() {
        let formats: Vec<String> = vec![
            "zip".to_string(),
            "tar.gz".to_string(),
            "tar.zst".to_string(),
            "tar.bz3".to_string(),
            "7z".to_string(),
        ];
        let levels: Vec<String> = vec![
            "Fast".to_string(),
            "Normal".to_string(),
            "Maximum".to_string(),
        ];

        let modal = iced_container(
            column![
                text(t!("Compress...")).size(20.0),
                text_input(t!("Name").to_string(), &app.ui.compress_name_input)
                    .on_input(|s| Message::UI(UIMsg::CompressNameChanged(s))),
                row![
                    text("Format:").size(14.0).width(Length::Fixed(80.0)),
                    pick_list(formats, Some(app.ui.compress_format.clone()), |f| {
                        Message::UI(UIMsg::CompressFormatChanged(f))
                    })
                    .width(Length::Fill)
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                row![
                    text("Level:").size(14.0).width(Length::Fixed(80.0)),
                    pick_list(
                        levels,
                        Some(app.ui.compress_level.clone()),
                        |l| Message::UI(UIMsg::CompressLevelChanged(l))
                    )
                    .width(Length::Fill)
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                Space::new().height(Length::Fixed(8.0)),
                row![
                    button::custom(
                        text(t!("Cancel"))
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Standard)
                    .on_press(Message::UI(UIMsg::CloseAllModals))
                    .width(Length::FillPortion(1)),
                    button::custom(
                        text(t!("Confirm"))
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                    )
                    .class(ButtonClass::Suggested)
                    .on_press(Message::File(FileMsg::ConfirmCompress))
                    .width(Length::FillPortion(1))
                ]
                .spacing(10)
            ]
            .spacing(16),
        )
        .style(FileApp::popover_style)
        .padding(24)
        .width(Length::Fixed(400.0));
        return Some(modal_overlay(
            modal.into(),
            Message::UI(UIMsg::CloseAllModals),
        ));
    }

    None
}
