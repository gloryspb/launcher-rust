use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::keyboard::key::Named;
use iced::widget::{
    button, checkbox, column, container, horizontal_space, image, mouse_area, opaque, radio, row,
    scrollable, stack, text, text_input,
};
use iced::window;
use iced::{
    event, keyboard, Alignment, Color, Element, Event, Length, Subscription, Task, Theme,
};
use iced::{Center, Fill};

use crate::core::{app_data_dir, is_gate_open, is_steam_uri};
use crate::drop_resolve::{self, ResolvedDrop};
use crate::game_store::{Game, GameStore};
use crate::settings::{AppMode, Settings};

pub fn run(settings: Settings) -> iced::Result {
    iced::application("Game Launcher", Launcher::update, Launcher::view)
        .subscription(Launcher::subscription)
        .theme(|_| Theme::Dark)
        .centered()
        .window(window::Settings {
            size: iced::Size::new(920.0, 520.0),
            min_size: Some(iced::Size::new(920.0, 520.0)),
            ..window::Settings::default()
        })
        .run_with(|| Launcher::new(settings))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Tiles,
}

#[derive(Debug, Clone)]
enum Modal {
    None,
    Alert(String),
    ConfirmDelete { ids: Vec<String> },
    AddManual {
        name: String,
        path: String,
        icon: String,
    },
    Edit {
        id: String,
        name: String,
        icon_path: String,
    },
    Batch {
        rows: Vec<BatchRow>,
    },
}

#[derive(Debug, Clone)]
struct BatchRow {
    enabled: bool,
    name: String,
    target_path: String,
    icon_source: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    ModifiersChanged(keyboard::Modifiers),
    ViewMode(ViewMode),
    RowClicked(String),
    RowRightClick(String),
    TileClicked(String),
    MoveUp,
    MoveDown,
    Launch,
    RemovePressed,
    ConfirmDeleteYes,
    ConfirmDeleteNo,
    AddPressed,
    AddPickExe,
    AddPickIcon,
    AddName(String),
    AddPath(String),
    AddIcon(String),
    AddConfirm,
    AddCancel,
    EditPressed,
    EditName(String),
    EditIcon(String),
    EditPickIcon,
    EditClearIcon,
    EditSave,
    EditCancel,
    BatchToggle(usize, bool),
    BatchName(usize, String),
    BatchIcon(usize, String),
    BatchPickIcon(usize),
    BatchIconPicked(usize, Option<PathBuf>),
    BatchConfirm,
    BatchCancel,
    AddPickMany,
    WindowEvent(window::Event),
    DeleteKey,
    DismissAlert,
    PickExeResult(Option<PathBuf>),
    PickIconResult(Option<PathBuf>),
    PickAddMultiResult(Option<Vec<PathBuf>>),
}

pub struct Launcher {
    app_dir: PathBuf,
    store: GameStore,
    view_mode: ViewMode,
    /// Selected game ids (list: ctrl toggles; tiles: ctrl toggles).
    selection: HashSet<String>,
    modifiers: keyboard::Modifiers,
    last_click: Option<(String, Instant)>,
    modal: Modal,
    mode: AppMode,
}

impl Launcher {
    fn new(settings: Settings) -> (Self, Task<Message>) {
        let app_dir = app_data_dir();
        let store = GameStore::new(&app_dir);
        (
            Self {
                app_dir,
                store,
                view_mode: ViewMode::List,
                selection: HashSet::new(),
                modifiers: keyboard::Modifiers::default(),
                last_click: None,
                modal: Modal::None,
                mode: settings.mode,
            },
            Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::events().map(|(_id, ev)| Message::WindowEvent(ev)),
            event::listen_with(|event, _status, _id| match event {
                Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                    Some(Message::ModifiersChanged(m))
                }
                _ => None,
            }),
            keyboard::on_key_press(|key, _modifiers| match key.as_ref() {
                keyboard::Key::Named(Named::Delete) => Some(Message::DeleteKey),
                _ => None,
            }),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ModifiersChanged(m) => {
                self.modifiers = m;
                Task::none()
            }
            Message::ViewMode(m) => {
                self.view_mode = m;
                self.selection.clear();
                self.last_click = None;
                Task::none()
            }
            Message::RowClicked(id) => {
                let now = Instant::now();
                let double = self
                    .last_click
                    .as_ref()
                    .is_some_and(|(prev, t)| {
                        prev == &id && now.duration_since(*t) < Duration::from_millis(450)
                    });
                self.last_click = Some((id.clone(), now));

                if double {
                    return Task::done(Message::Launch);
                }

                if self.modifiers.control() {
                    if self.selection.contains(&id) {
                        self.selection.remove(&id);
                    } else {
                        self.selection.insert(id);
                    }
                } else {
                    self.selection.clear();
                    self.selection.insert(id);
                }
                Task::none()
            }
            Message::RowRightClick(id) => {
                self.selection.clear();
                self.selection.insert(id);
                Task::none()
            }
            Message::TileClicked(id) => {
                if self.modifiers.control() {
                    if self.selection.contains(&id) {
                        self.selection.remove(&id);
                    } else {
                        self.selection.insert(id);
                    }
                } else {
                    self.selection.clear();
                    self.selection.insert(id);
                }
                self.last_click = None;
                Task::none()
            }
            Message::MoveUp => {
                if self.view_mode == ViewMode::List && self.selection.len() == 1 {
                    let id = self.selection.iter().next().unwrap().clone();
                    let _ = self.store.move_game(&id, -1);
                }
                Task::none()
            }
            Message::MoveDown => {
                if self.view_mode == ViewMode::List && self.selection.len() == 1 {
                    let id = self.selection.iter().next().unwrap().clone();
                    let _ = self.store.move_game(&id, 1);
                }
                Task::none()
            }
            Message::Launch => {
                if !is_gate_open(&self.app_dir) {
                    self.modal = Modal::Alert(
                        "Сначала выполни задание (гейт закрыт).".to_string(),
                    );
                    return Task::none();
                }
                if self.selection.len() != 1 {
                    self.modal = Modal::Alert("Для запуска выбери ровно одну игру.".to_string());
                    return Task::none();
                }
                let id = self.selection.iter().next().unwrap().clone();
                let Some(g) = self.store.get(&id) else {
                    self.modal = Modal::Alert("Игра не найдена.".to_string());
                    return Task::none();
                };
                let p = g.path.trim();
                if p.is_empty() {
                    self.modal = Modal::Alert("Пустой путь.".to_string());
                    return Task::none();
                }
                if !is_steam_uri(p) && !Path::new(p).exists() {
                    self.modal = Modal::Alert("Файл игры не найден.".to_string());
                    return Task::none();
                }
                if let Err(e) = self.store.launch(&id) {
                    self.modal = Modal::Alert(e.to_string());
                }
                Task::none()
            }
            Message::RemovePressed => {
                if self.selection.is_empty() {
                    self.modal = Modal::Alert("Выбери игру из списка.".to_string());
                    return Task::none();
                }
                let ids: Vec<String> = self.selection.iter().cloned().collect();
                self.modal = Modal::ConfirmDelete { ids };
                Task::none()
            }
            Message::ConfirmDeleteYes => {
                if let Modal::ConfirmDelete { ids } = std::mem::replace(&mut self.modal, Modal::None)
                {
                    let mut errs = Vec::new();
                    for id in ids {
                        if let Err(e) = self.store.remove(&id) {
                            errs.push(format!("{id}: {e}"));
                        }
                    }
                    self.selection.clear();
                    if !errs.is_empty() {
                        self.modal = Modal::Alert(errs.join("\n"));
                    }
                }
                Task::none()
            }
            Message::ConfirmDeleteNo => {
                self.modal = Modal::None;
                Task::none()
            }
            Message::AddPressed => {
                self.modal = Modal::AddManual {
                    name: String::new(),
                    path: String::new(),
                    icon: String::new(),
                };
                Task::none()
            }
            Message::AddPickExe => Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        rfd::FileDialog::new()
                            .add_filter("Игры и ярлыки", &["exe", "lnk", "url"])
                            .pick_file()
                    })
                    .await
                    .ok()
                    .flatten()
                },
                Message::PickExeResult,
            ),
            Message::AddPickIcon | Message::EditPickIcon => Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        rfd::FileDialog::new()
                            .add_filter("Картинки", &["png", "gif", "ppm", "pgm", "ico"])
                            .pick_file()
                    })
                    .await
                    .ok()
                    .flatten()
                },
                Message::PickIconResult,
            ),
            Message::PickExeResult(p) => {
                if let Modal::AddManual { path, .. } = &mut self.modal {
                    if let Some(p) = p {
                        *path = p.to_string_lossy().to_string();
                    }
                }
                Task::none()
            }
            Message::PickIconResult(p) => {
                if let Some(p) = p {
                    let s = p.to_string_lossy().to_string();
                    match &mut self.modal {
                        Modal::AddManual { icon, .. } => *icon = s,
                        Modal::Edit { icon_path, .. } => *icon_path = s,
                        _ => {}
                    }
                }
                Task::none()
            }
            Message::PickAddMultiResult(paths) => {
                if let Some(paths) = paths {
                    self.open_batch_from_paths(paths);
                }
                Task::none()
            }
            Message::AddName(s) => {
                if let Modal::AddManual { name, .. } = &mut self.modal {
                    *name = s;
                }
                Task::none()
            }
            Message::AddPath(s) => {
                if let Modal::AddManual { path, .. } = &mut self.modal {
                    *path = s;
                }
                Task::none()
            }
            Message::AddIcon(s) => {
                if let Modal::AddManual { icon, .. } = &mut self.modal {
                    *icon = s;
                }
                Task::none()
            }
            Message::AddConfirm => {
                if let Modal::AddManual { name, path, icon } =
                    std::mem::replace(&mut self.modal, Modal::None)
                {
                    match self.store.add(name.trim(), path.trim(), icon.trim()) {
                        Ok(()) => {}
                        Err(e) => self.modal = Modal::Alert(e.to_string()),
                    }
                }
                Task::none()
            }
            Message::AddCancel => {
                self.modal = Modal::None;
                Task::none()
            }
            Message::EditPressed => {
                if self.selection.len() != 1 {
                    self.modal = Modal::Alert("Выбери одну игру для изменения.".to_string());
                    return Task::none();
                }
                let id = self.selection.iter().next().unwrap().clone();
                let Some(g) = self.store.get(&id).cloned() else {
                    self.modal = Modal::Alert("Игра не найдена.".to_string());
                    return Task::none();
                };
                self.modal = Modal::Edit {
                    id,
                    name: g.name,
                    icon_path: g.icon,
                };
                Task::none()
            }
            Message::EditName(s) => {
                if let Modal::Edit { name, .. } = &mut self.modal {
                    *name = s;
                }
                Task::none()
            }
            Message::EditIcon(s) => {
                if let Modal::Edit { icon_path, .. } = &mut self.modal {
                    *icon_path = s;
                }
                Task::none()
            }
            Message::EditClearIcon => {
                if let Modal::Edit { icon_path, .. } = &mut self.modal {
                    icon_path.clear();
                }
                Task::none()
            }
            Message::EditSave => {
                if let Modal::Edit { id, name, icon_path } =
                    std::mem::replace(&mut self.modal, Modal::None)
                {
                    if let Err(e) = self
                        .store
                        .update_game_meta(&id, name.trim(), icon_path.trim())
                    {
                        self.modal = Modal::Alert(e.to_string());
                    }
                }
                Task::none()
            }
            Message::EditCancel => {
                self.modal = Modal::None;
                Task::none()
            }
            Message::BatchToggle(i, v) => {
                if let Modal::Batch { rows } = &mut self.modal {
                    if let Some(r) = rows.get_mut(i) {
                        r.enabled = v;
                    }
                }
                Task::none()
            }
            Message::BatchName(i, s) => {
                if let Modal::Batch { rows } = &mut self.modal {
                    if let Some(r) = rows.get_mut(i) {
                        r.name = s;
                    }
                }
                Task::none()
            }
            Message::BatchIcon(i, s) => {
                if let Modal::Batch { rows } = &mut self.modal {
                    if let Some(r) = rows.get_mut(i) {
                        r.icon_source = s;
                    }
                }
                Task::none()
            }
            Message::BatchPickIcon(i) => Task::perform(
                async move {
                    let picked = tokio::task::spawn_blocking(|| {
                        rfd::FileDialog::new()
                            .add_filter("Картинки", &["png", "gif", "ppm", "pgm", "ico"])
                            .pick_file()
                    })
                    .await
                    .ok()
                    .flatten();
                    (i, picked)
                },
                |(i, p)| Message::BatchIconPicked(i, p),
            ),
            Message::BatchIconPicked(i, p) => {
                if let Some(p) = p {
                    let s = p.to_string_lossy().to_string();
                    if let Modal::Batch { rows } = &mut self.modal {
                        if let Some(r) = rows.get_mut(i) {
                            r.icon_source = s;
                        }
                    }
                }
                Task::none()
            }
            Message::BatchConfirm => {
                if let Modal::Batch { rows } = std::mem::replace(&mut self.modal, Modal::None) {
                    let mut selected = 0usize;
                    let mut added = 0usize;
                    let mut errs = Vec::new();
                    for r in rows {
                        if !r.enabled {
                            continue;
                        }
                        selected += 1;
                        match self.store.add(
                            r.name.trim(),
                            r.target_path.trim(),
                            r.icon_source.trim(),
                        ) {
                            Ok(()) => added += 1,
                            Err(e) => errs.push(format!("{}: {e}", r.name.trim())),
                        }
                    }
                    if selected == 0 {
                        self.modal =
                            Modal::Alert("Выбери хотя бы одну строку для добавления.".to_string());
                    } else if !errs.is_empty() {
                        self.modal = Modal::Alert(format!(
                            "Добавлено: {added}. Ошибки:\n{}",
                            errs.join("\n")
                        ));
                    } else if added == 0 {
                        self.modal = Modal::Alert("Не удалось добавить игры.".to_string());
                    }
                }
                Task::none()
            }
            Message::BatchCancel => {
                self.modal = Modal::None;
                Task::none()
            }
            Message::AddPickMany => Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        rfd::FileDialog::new()
                            .add_filter("Игры и ярлыки", &["exe", "lnk", "url"])
                            .pick_files()
                    })
                    .await
                    .ok()
                    .flatten()
                },
                Message::PickAddMultiResult,
            ),
            Message::WindowEvent(ev) => {
                if let window::Event::FileDropped(path) = ev {
                    let mut paths = vec![path];
                    self.ingest_dropped_paths(&mut paths);
                }
                Task::none()
            }
            Message::DeleteKey => {
                if matches!(self.modal, Modal::None) {
                    return Task::done(Message::RemovePressed);
                }
                Task::none()
            }
            Message::DismissAlert => {
                self.modal = Modal::None;
                Task::none()
            }
        }
    }

    fn ingest_dropped_paths(&mut self, paths: &mut Vec<PathBuf>) {
        let mut valid: Vec<ResolvedDrop> = Vec::new();
        let mut skipped = 0usize;
        for p in paths.drain(..) {
            match drop_resolve::resolve_drop_path(&p) {
                Ok(r) => valid.push(r),
                Err(_) => skipped += 1,
            }
        }
        if valid.is_empty() {
            let msg = if skipped > 0 {
                format!("Нет подходящих файлов (пропущено: {skipped}). Перетащи .lnk / .url / .exe")
            } else {
                "Перетащи .lnk / .url / .exe".to_string()
            };
            self.modal = Modal::Alert(msg);
            return;
        }
        if skipped > 0 {
            // Non-blocking note: still open batch; user sees batch first
            let _ = skipped;
        }
        self.modal = Modal::Batch {
            rows: valid
                .into_iter()
                .map(|r| BatchRow {
                    enabled: true,
                    name: r.name,
                    target_path: r.target_path,
                    icon_source: r.icon_source,
                })
                .collect(),
        };
    }

    fn open_batch_from_paths(&mut self, paths: Vec<PathBuf>) {
        let mut buf = paths;
        self.ingest_dropped_paths(&mut buf);
    }

    fn view(&self) -> Element<'_, Message> {
        let gate_ok = is_gate_open(&self.app_dir);
        let mode_badge = match self.mode {
            AppMode::Debug => text("MODE: DEBUG").size(12).style(|_| text::Style {
                color: Some(Color::from_rgb8(0xff, 0xcc, 0x00)),
            }),
            AppMode::Release => text("MODE: RELEASE").size(12).style(|_| text::Style {
                color: Some(Color::from_rgb8(0xaa, 0xaa, 0xaa)),
            }),
        };
        let status = if gate_ok {
            text("✓ Задание выполнено").style(|_| text::Style {
                color: Some(Color::from_rgb8(0x4e, 0xcc, 0xa3)),
            })
        } else {
            text("✗ Сначала выполни задание").style(|_| text::Style {
                color: Some(Color::from_rgb8(0xb8, 0x5c, 0x5c)),
            })
        };

        let can_launch = gate_ok && self.selection.len() == 1;
        let has_sel = !self.selection.is_empty();
        let list_single = self.view_mode == ViewMode::List && self.selection.len() == 1;

        let toolbar = row![
            button("▶ Запустить")
                .on_press_maybe(can_launch.then_some(Message::Launch)),
            button("+ Добавить").on_press(Message::AddPressed),
            button("+ Несколько…").on_press(Message::AddPickMany),
            button("✎ Изменить").on_press(Message::EditPressed),
            button("✕ Удалить").on_press_maybe(has_sel.then_some(Message::RemovePressed)),
            horizontal_space(),
            radio("Список", ViewMode::List, Some(self.view_mode), Message::ViewMode),
            radio("Плитки", ViewMode::Tiles, Some(self.view_mode), Message::ViewMode),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let reorder: Element<'_, Message> = if self.view_mode == ViewMode::List {
            row![
                button("↑").on_press_maybe(list_single.then_some(Message::MoveUp)),
                button("↓").on_press_maybe(list_single.then_some(Message::MoveDown)),
                text(" (порядок в списке)").size(12),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        } else {
            row![].into()
        };

        let body: Element<'_, Message> = match self.view_mode {
            ViewMode::List => self.view_list(),
            ViewMode::Tiles => self.view_tiles(),
        };

        let main = column![
            text("GAME LAUNCHER").size(20),
            mode_badge,
            status,
            toolbar,
            reorder,
            container(body).height(Fill).padding(8),
            text(format!(
                "Данные: {}",
                self.store.config_path.display()
            ))
            .size(11),
        ]
        .spacing(8)
        .padding(12);

        let under: Element<'_, Message> = match &self.modal {
            Modal::None => column![opaque(main)].into(),
            Modal::Alert(msg) => stack![
                opaque(main),
                opaque(
                    container(
                        column![
                            text(msg).size(14),
                            button("OK").on_press(Message::DismissAlert),
                        ]
                        .spacing(12)
                        .max_width(420),
                    )
                    .padding(24)
                    .style(|_theme| container::Style {
                        text_color: Some(Color::WHITE),
                        background: Some(Color::from_rgba8(30, 30, 40, 240.0 / 255.0).into()),
                        border: iced::Border {
                            radius: 8.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .center_x(Fill)
                    .center_y(Fill)
                    .width(Fill)
                    .height(Fill)
                )
            ]
            .into(),
            Modal::ConfirmDelete { ids } => stack![
                opaque(main),
                opaque(
                    container(
                        column![
                            text(format!("Удалить {} игр?", ids.len())).size(16),
                            row![
                                button("Да").on_press(Message::ConfirmDeleteYes),
                                button("Нет").on_press(Message::ConfirmDeleteNo),
                            ]
                            .spacing(12),
                        ]
                        .spacing(16),
                    )
                    .padding(24)
                    .style(|_theme| container::Style {
                        text_color: Some(Color::WHITE),
                        background: Some(Color::from_rgba8(30, 30, 40, 240.0 / 255.0).into()),
                        border: iced::Border {
                            radius: 8.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .center_x(Fill)
                    .center_y(Fill)
                    .width(Fill)
                    .height(Fill)
                )
            ]
            .into(),
            Modal::AddManual { name, path, icon } => stack![
                opaque(main),
                opaque(
                    container(
                        column![
                            text("Добавить игру").size(18),
                            text("Название:"),
                            text_input("Название", name)
                                .on_input(Message::AddName)
                                .padding(6),
                            text("Путь:"),
                            row![
                                text_input("Путь к .exe / .lnk / .url", path)
                                    .on_input(Message::AddPath)
                                    .padding(6)
                                    .width(Fill),
                                button("…").on_press(Message::AddPickExe),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            text("Иконка (необязательно):"),
                            row![
                                text_input("Путь к иконке", icon)
                                    .on_input(Message::AddIcon)
                                    .padding(6)
                                    .width(Fill),
                                button("…").on_press(Message::AddPickIcon),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            row![
                                button("Добавить").on_press(Message::AddConfirm),
                                button("Отмена").on_press(Message::AddCancel),
                            ]
                            .spacing(12),
                        ]
                        .spacing(10)
                        .max_width(480),
                    )
                    .padding(20)
                    .style(|_theme| container::Style {
                        text_color: Some(Color::WHITE),
                        background: Some(Color::from_rgba8(25, 25, 35, 245.0 / 255.0).into()),
                        border: iced::Border {
                            radius: 10.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .center_x(Fill)
                    .center_y(Fill)
                    .width(Fill)
                    .height(Fill)
                )
            ]
            .into(),
            Modal::Edit {
                id,
                name,
                icon_path,
            } => {
                let path_label = self
                    .store
                    .get(id.as_str())
                    .map(|g| g.path.as_str())
                    .unwrap_or("");
                stack![
                    opaque(main),
                    opaque(
                        container(
                            column![
                                text("Изменить игру").size(18),
                                text("Название:"),
                                text_input("Название", name)
                                    .on_input(Message::EditName)
                                    .padding(6),
                                text("Путь (только чтение):"),
                                text(path_label).size(12),
                                text("Иконка (путь к файлу, пусто = сброс):"),
                                row![
                                    text_input("Иконка", icon_path)
                                        .on_input(Message::EditIcon)
                                        .padding(6)
                                        .width(Fill),
                                    button("…").on_press(Message::EditPickIcon),
                                ]
                                .spacing(8)
                                .align_y(Alignment::Center),
                                row![
                                    button("Убрать иконку").on_press(Message::EditClearIcon),
                                ],
                                row![
                                    button("Сохранить").on_press(Message::EditSave),
                                    button("Отмена").on_press(Message::EditCancel),
                                ]
                                .spacing(12),
                            ]
                            .spacing(10)
                            .max_width(480),
                        )
                        .padding(20)
                        .style(|_theme| container::Style {
                            text_color: Some(Color::WHITE),
                            background: Some(Color::from_rgba8(25, 25, 35, 245.0 / 255.0).into()),
                            border: iced::Border {
                                radius: 10.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .center_x(Fill)
                        .center_y(Fill)
                        .width(Fill)
                        .height(Fill)
                    )
                ]
                .into()
            }
            Modal::Batch { rows } => stack![
                opaque(main),
                opaque(
                    container(
                        column![
                            text(format!("Добавить игры: {} (снимай галочки с лишних)", rows.len()))
                                .size(16),
                            scrollable(
                                column(
                                    rows
                                        .iter()
                                        .enumerate()
                                        .map(|(i, r)| {
                                            batch_row_view(i, r).map(move |m| match m {
                                                BatchMsg::Toggle(v) => Message::BatchToggle(i, v),
                                                BatchMsg::Name(s) => Message::BatchName(i, s),
                                                BatchMsg::Icon(s) => Message::BatchIcon(i, s),
                                                BatchMsg::PickIcon => Message::BatchPickIcon(i),
                                            })
                                        })
                                        .collect::<Vec<_>>(),
                                )
                                .spacing(10)
                                .padding(4),
                            )
                            .height(Length::Fixed(280.)),
                            row![
                                button("Добавить выбранные").on_press(Message::BatchConfirm),
                                button("Отмена").on_press(Message::BatchCancel),
                            ]
                            .spacing(12),
                        ]
                        .spacing(12)
                        .max_width(560),
                    )
                    .padding(16)
                    .style(|_theme| container::Style {
                        text_color: Some(Color::WHITE),
                        background: Some(Color::from_rgba8(22, 22, 32, 248.0 / 255.0).into()),
                        border: iced::Border {
                            radius: 10.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .center_x(Fill)
                    .center_y(Fill)
                    .width(Fill)
                    .height(Fill)
                )
            ]
            .into(),
        };

        container(under).width(Fill).height(Fill).into()
    }

    fn view_list(&self) -> Element<'_, Message> {
        if self.store.games.is_empty() {
            container(
                text("Нет игр. Добавь игры кнопкой выше или перетащи сюда .lnk / .exe")
                    .size(14)
                    .style(|_| text::Style {
                        color: Some(Color::from_rgb8(0x88, 0x88, 0x98)),
                    }),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .style(|_theme| container::Style {
                background: Some(Color::from_rgb8(0x10, 0x14, 0x24).into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    color: Color::from_rgb8(0x3a, 0x40, 0x5a),
                    width: 1.0,
                },
                ..Default::default()
            })
            .padding(16)
            .into()
        } else {
            let list_column: Vec<Element<'_, Message>> = self.store.games.iter().map(|g| self.game_row_list(g)).collect();
            let list_container = container(
                column(list_column)
                    .spacing(2)
                    .width(Fill),
            )
            .style(|_theme| container::Style {
                background: Some(Color::from_rgb8(0x10, 0x14, 0x24).into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    color: Color::from_rgb8(0x3a, 0x40, 0x5a),
                    width: 1.0,
                },
                ..Default::default()
            })
            .padding(4);

            scrollable(list_container)
                .height(Fill)
                .into()
        }
    }

    fn game_row_list<'a>(&'a self, g: &'a Game) -> Element<'a, Message> {
        let selected = self.selection.contains(&g.id);
        let mark = if GameStore::path_exists_for_display(&g.path) {
            "✓"
        } else {
            "✗"
        };
        let mark_color = if GameStore::path_exists_for_display(&g.path) {
            Color::from_rgb8(0x4e, 0xcc, 0xa3)
        } else {
            Color::from_rgb8(0xb8, 0x5c, 0x5c)
        };
        let icon_el: Element<'a, Message> = icon_widget(
            &g.icon,
            self.store.config_path.parent().unwrap_or(Path::new(".")),
            32,
        );
        let name_el = text(&g.name).size(14);
        let path_el = text(&g.path)
            .size(11)
            .style(|_| text::Style {
                color: Some(Color::from_rgb8(0x70, 0x70, 0x80)),
            });
        let mark_el = text(mark).size(14).style(move |_| text::Style {
            color: Some(mark_color),
        });

        let row_content = row![
            icon_el,
            mark_el,
            column![name_el, path_el]
                .spacing(2)
                .width(Fill),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(12);

        let bg = if selected {
            Color::from_rgb8(0x2a, 0x38, 0x50)
        } else {
            Color::from_rgb8(0x1a, 0x1f, 0x30)
        };
        let border_color = if selected {
            Color::from_rgb8(0x4a, 0x60, 0x80)
        } else {
            Color::from_rgb8(0x28, 0x30, 0x44)
        };
        let id = g.id.clone();

        mouse_area(
            container(row_content)
                .width(Fill)
                .style(move |_theme| container::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        radius: 6.0.into(),
                        color: border_color,
                        width: 1.0,
                    },
                    ..Default::default()
                }),
        )
        .on_press(Message::RowClicked(id.clone()))
        .on_right_press(Message::RowRightClick(id))
        .into()
    }

    fn view_tiles(&self) -> Element<'_, Message> {
        const COLS: usize = 4;
        let mut rows_e: Vec<Element<'_, Message>> = Vec::new();
        let games = &self.store.games;
        let mut col = 0usize;
        let mut current = row![].spacing(8).align_y(Alignment::Start);
        for g in games {
            let tile = self.game_tile(g);
            current = current.push(tile);
            col += 1;
            if col >= COLS {
                rows_e.push(current.into());
                current = row![].spacing(8);
                col = 0;
            }
        }
        if col > 0 {
            rows_e.push(current.into());
        }
        scrollable(column(rows_e).spacing(12).width(Fill))
            .height(Fill)
            .into()
    }

    fn game_tile<'a>(&'a self, g: &'a Game) -> Element<'a, Message> {
        let selected = self.selection.contains(&g.id);
        let icon_el = icon_widget(
            &g.icon,
            self.store.config_path.parent().unwrap_or(Path::new(".")),
            72,
        );
        let title = text(&g.name).size(12).align_x(Center).width(Fill);
        let inner = column![icon_el, title]
            .spacing(6)
            .align_x(Center)
            .width(Length::Fixed(120.));
        let bg = if selected {
            Color::from_rgb8(0x2e, 0x7d, 0x5a)
        } else {
            Color::from_rgb8(0x1a, 0x1a, 0x2e)
        };
        mouse_area(
            container(inner)
                .padding(10)
                .style(move |_theme| container::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        )
        .on_press(Message::TileClicked(g.id.clone()))
        .into()
    }
}

#[derive(Clone)]
enum BatchMsg {
    Toggle(bool),
    Name(String),
    Icon(String),
    PickIcon,
}

fn batch_row_view(i: usize, r: &BatchRow) -> Element<'_, BatchMsg> {
    column![
        row![
            checkbox("", r.enabled)
                .on_toggle(BatchMsg::Toggle)
                .size(18),
            text(format!("#{i}")).size(11),
            horizontal_space(),
        ]
        .align_y(Alignment::Center),
        text_input("Название", &r.name)
            .on_input(BatchMsg::Name)
            .padding(4),
        text(&r.target_path).size(10),
        row![
            text_input("Иконка", &r.icon_source)
                .on_input(BatchMsg::Icon)
                .padding(4)
                .width(Fill),
            button("…").on_press(BatchMsg::PickIcon),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(6)
    .padding(8)
    .into()
}

fn icon_widget<'a>(icon: &'a str, config_parent: &'a Path, size: u16) -> Element<'a, Message> {
    let p = drop_resolve::normalize_icon_path_for_preview(icon, config_parent);
    if icon.trim().is_empty() || !p.exists() {
        return container(text("—").size(14))
            .width(Length::Fixed(f32::from(size)))
            .height(Length::Fixed(f32::from(size)))
            .center_x(Length::Fixed(f32::from(size)))
            .center_y(Length::Fixed(f32::from(size)))
            .style(|_theme| container::Style {
                background: Some(Color::from_rgb8(0x0f, 0x34, 0x60).into()),
                ..Default::default()
            })
            .into();
    }
    image(p)
        .width(Length::Fixed(f32::from(size)))
        .height(Length::Fixed(f32::from(size)))
        .into()
}