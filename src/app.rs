use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::keyboard::key::Named;
use iced::widget::{
    button, checkbox, column, container, horizontal_space, image, mouse_area, opaque, radio, row,
    scrollable, stack, text, text_input,
};
use iced::window;
use iced::{
    event, keyboard, mouse, Alignment, Color, Element, Event, Length, Point, Subscription, Task,
    Theme,
};
use iced::{Center, Fill};

use crate::core::{app_data_dir, is_gate_open, is_uri};
use crate::drop_resolve::{self, ResolvedDrop};
use crate::game_store::{Game, GameStore};
use crate::settings::{AppMode, Settings};

pub fn run(settings: Settings, startup_warnings: Vec<String>) -> iced::Result {
    iced::application("Game Launcher", Launcher::update, Launcher::view)
        .subscription(Launcher::subscription)
        .theme(|_| Theme::Dark)
        .centered()
        .window(window::Settings {
            size: iced::Size::new(920.0, 520.0),
            min_size: Some(iced::Size::new(920.0, 520.0)),
            ..window::Settings::default()
        })
        .run_with(|| Launcher::new(settings, startup_warnings))
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
    ConfirmDelete {
        ids: Vec<String>,
    },
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
struct DragState {
    source_id: String,
    hover_id: Option<String>,
    started_at: Point,
    is_active: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    ModifiersChanged(keyboard::Modifiers),
    ViewMode(ViewMode),
    GamePressed(String),
    GameRightPressed(String),
    DragEntered(String),
    DragExited(String),
    CursorMoved(Point),
    PointerReleased,
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
    WindowResized(f32),
}

pub struct Launcher {
    app_dir: PathBuf,
    store: GameStore,
    view_mode: ViewMode,
    window_width: f32,
    selection: HashSet<String>,
    modifiers: keyboard::Modifiers,
    cursor_position: Option<Point>,
    drag_state: Option<DragState>,
    last_click: Option<(String, Instant)>,
    icon_cache: RefCell<HashMap<String, image::Handle>>,
    modal: Modal,
    mode: AppMode,
}

impl Launcher {
    fn new(settings: Settings, mut startup_warnings: Vec<String>) -> (Self, Task<Message>) {
        let app_dir = app_data_dir();
        let mut store = GameStore::new(&app_dir);
        if let Some(warning) = store.take_startup_warning() {
            startup_warnings.push(warning);
        }
        let modal = if startup_warnings.is_empty() {
            Modal::None
        } else {
            Modal::Alert(startup_warnings.join("\n\n"))
        };
        (
            Self {
                app_dir,
                store,
                view_mode: ViewMode::List,
                window_width: 920.0,
                selection: HashSet::new(),
                modifiers: keyboard::Modifiers::default(),
                cursor_position: None,
                drag_state: None,
                last_click: None,
                icon_cache: RefCell::new(HashMap::new()),
                modal,
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
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    Some(Message::CursorMoved(position))
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::PointerReleased)
                }
                Event::Window(window::Event::Resized(size)) => {
                    Some(Message::WindowResized(size.width))
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
                self.drag_state = None;
                self.last_click = None;
                Task::none()
            }
            Message::GamePressed(id) => {
                let now = Instant::now();
                let double = self.view_mode == ViewMode::List
                    && self.last_click.as_ref().is_some_and(|(prev, t)| {
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
                        self.selection.insert(id.clone());
                    }
                } else {
                    self.selection.clear();
                    self.selection.insert(id.clone());
                }
                self.drag_state = self.game_index(&id).map(|_| DragState {
                    source_id: id,
                    hover_id: None,
                    started_at: self.cursor_position.unwrap_or(Point::ORIGIN),
                    is_active: false,
                });
                Task::none()
            }
            Message::GameRightPressed(id) => {
                self.selection.clear();
                self.selection.insert(id);
                self.drag_state = None;
                Task::none()
            }
            Message::DragEntered(id) => {
                if let Some(drag) = &mut self.drag_state {
                    drag.hover_id = Some(id);
                }
                Task::none()
            }
            Message::DragExited(id) => {
                if let Some(drag) = &mut self.drag_state {
                    if drag.hover_id.as_deref() == Some(id.as_str()) {
                        drag.hover_id = None;
                    }
                }
                Task::none()
            }
            Message::CursorMoved(position) => {
                self.cursor_position = Some(position);
                if let Some(drag) = &mut self.drag_state {
                    if !drag.is_active {
                        let delta = position - drag.started_at;
                        if delta.x.abs() > 6.0 || delta.y.abs() > 6.0 {
                            drag.is_active = true;
                            drag.hover_id = Some(drag.source_id.clone());
                            self.last_click = None;
                        }
                    }
                }
                Task::none()
            }
            Message::PointerReleased => {
                self.finish_drag_reorder();
                Task::none()
            }
            Message::Launch => {
                if !is_gate_open(&self.app_dir) {
                    self.modal = Modal::Alert("Сначала выполни задание (гейт закрыт).".to_string());
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
                if !is_uri(p) && !Path::new(p).exists() {
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
                if let Modal::ConfirmDelete { ids } =
                    std::mem::replace(&mut self.modal, Modal::None)
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
                if let Modal::Edit {
                    id,
                    name,
                    icon_path,
                } = std::mem::replace(&mut self.modal, Modal::None)
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
            Message::WindowResized(width) => {
                self.window_width = width;
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

    fn game_index(&self, game_id: &str) -> Option<usize> {
        self.store.games.iter().position(|game| game.id == game_id)
    }

    fn finish_drag_reorder(&mut self) {
        let Some(drag) = self.drag_state.take() else {
            return;
        };

        if !drag.is_active {
            return;
        }

        let Some(target_id) = drag.hover_id else {
            return;
        };

        if target_id == drag.source_id {
            return;
        }

        let Some(target_index) = self.game_index(&target_id) else {
            return;
        };

        if let Err(error) = self.store.move_game_to(&drag.source_id, target_index) {
            self.modal = Modal::Alert(error.to_string());
        }
    }

    fn is_drag_source(&self, game_id: &str) -> bool {
        self.drag_state
            .as_ref()
            .is_some_and(|drag| drag.is_active && drag.source_id == game_id)
    }

    fn is_drag_target(&self, game_id: &str) -> bool {
        self.drag_state.as_ref().is_some_and(|drag| {
            drag.is_active && drag.source_id != game_id && drag.hover_id.as_deref() == Some(game_id)
        })
    }

    fn drag_preview_game<'a>(&'a self, target_id: &str) -> Option<&'a Game> {
        let drag = self.drag_state.as_ref()?;
        if !drag.is_active
            || drag.source_id == target_id
            || drag.hover_id.as_deref() != Some(target_id)
        {
            return None;
        }

        self.store.get(&drag.source_id)
    }

    fn icon_widget(
        &self,
        icon: &str,
        config_parent: &Path,
        size: u16,
    ) -> Element<'static, Message> {
        let path = drop_resolve::normalize_icon_path_for_preview(icon, config_parent);
        if icon.trim().is_empty() || !path.exists() {
            return container(text("-").size(14))
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

        let key = path.to_string_lossy().to_string();
        let handle = {
            let mut cache = self.icon_cache.borrow_mut();
            cache
                .entry(key)
                .or_insert_with(|| image::Handle::from_path(&path))
                .clone()
        };

        image(handle)
            .width(Length::Fixed(f32::from(size)))
            .height(Length::Fixed(f32::from(size)))
            .into()
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

        let toolbar = row![
            button("▶ Запустить").on_press_maybe(can_launch.then_some(Message::Launch)),
            button("+ Добавить").on_press(Message::AddPressed),
            button("+ Несколько…").on_press(Message::AddPickMany),
            button("✎ Изменить").on_press(Message::EditPressed),
            button("✕ Удалить").on_press_maybe(has_sel.then_some(Message::RemovePressed)),
            horizontal_space(),
            radio(
                "Список",
                ViewMode::List,
                Some(self.view_mode),
                Message::ViewMode
            ),
            radio(
                "Плитки",
                ViewMode::Tiles,
                Some(self.view_mode),
                Message::ViewMode
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let body: Element<'_, Message> = match self.view_mode {
            ViewMode::List => self.view_list(),
            ViewMode::Tiles => self.view_tiles(),
        };

        let body_container =
            container(body)
                .height(Fill)
                .padding(4)
                .style(|_theme| container::Style {
                    background: Some(Color::from_rgb8(0x10, 0x14, 0x24).into()),
                    border: iced::Border {
                        radius: 8.0.into(),
                        color: Color::from_rgb8(0x3a, 0x40, 0x5a),
                        width: 1.0,
                    },
                    ..Default::default()
                });

        let main = column![
            text("GAME LAUNCHER").size(20),
            mode_badge,
            status,
            toolbar,
            body_container,
            text(format!("Данные: {}", self.store.config_path.display())).size(11),
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
                                row![button("Убрать иконку").on_press(Message::EditClearIcon),],
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
                            text(format!(
                                "Добавить игры: {} (снимай галочки с лишних)",
                                rows.len()
                            ))
                            .size(16),
                            scrollable(
                                column(
                                    rows.iter()
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
            .padding(16)
            .into()
        } else {
            let list_column: Vec<Element<'_, Message>> = self
                .store
                .games
                .iter()
                .map(|g| self.game_row_list(g))
                .collect();

            scrollable(column(list_column).spacing(8).padding(8).width(Fill))
                .height(Fill)
                .into()
        }
    }

    fn game_row_list<'a>(&'a self, g: &'a Game) -> Element<'a, Message> {
        let selected = self.selection.contains(&g.id);
        let drag_source = self.is_drag_source(&g.id);
        let drag_target = self.is_drag_target(&g.id);
        let preview_game = self.drag_preview_game(&g.id);
        let display_game = preview_game.unwrap_or(g);
        let is_preview = preview_game.is_some();
        let mark = if GameStore::path_exists_for_display(&display_game.path) {
            "✓"
        } else {
            "✗"
        };
        let mark_color = if GameStore::path_exists_for_display(&display_game.path) {
            Color::from_rgb8(0x4e, 0xcc, 0xa3)
        } else {
            Color::from_rgb8(0xb8, 0x5c, 0x5c)
        };
        let icon_el: Element<'static, Message> = self.icon_widget(
            &display_game.icon,
            self.store.config_path.parent().unwrap_or(Path::new(".")),
            32,
        );
        let name_el = text(&display_game.name).size(14);
        let path_el = text(&display_game.path).size(11).style(|_| text::Style {
            color: Some(Color::from_rgb8(0x70, 0x70, 0x80)),
        });
        let mark_el = text(mark).size(14).style(move |_| text::Style {
            color: Some(mark_color),
        });
        let meta_col = if is_preview {
            column![
                text("PREVIEW").size(10).style(|_| text::Style {
                    color: Some(Color::from_rgb8(0xf0, 0xc6, 0x63)),
                }),
                name_el,
                path_el,
            ]
        } else {
            column![name_el, path_el]
        }
        .spacing(2)
        .width(Fill);

        let row_content = row![icon_el, mark_el, meta_col,]
            .spacing(12)
            .align_y(Alignment::Center)
            .padding(12);

        let bg = if drag_target {
            Color::from_rgb8(0x54, 0x5a, 0x1f)
        } else if drag_source {
            Color::from_rgb8(0x25, 0x2d, 0x3d)
        } else if selected {
            Color::from_rgb8(0x2a, 0x38, 0x50)
        } else {
            Color::from_rgb8(0x1a, 0x1f, 0x30)
        };
        let border_color = if drag_target {
            Color::from_rgb8(0xd4, 0xb4, 0x44)
        } else if drag_source {
            Color::from_rgb8(0x5e, 0x7b, 0xa0)
        } else if selected {
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
        .on_press(Message::GamePressed(id.clone()))
        .on_right_press(Message::GameRightPressed(id.clone()))
        .on_enter(Message::DragEntered(id.clone()))
        .on_exit(Message::DragExited(id))
        .into()
    }

    fn view_tiles(&self) -> Element<'_, Message> {
        let games = &self.store.games;
        if games.is_empty() {
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
            .padding(16)
            .into()
        } else {
            let total = games.len();
            let tile_width: f32 = 100.0;
            let spacing: f32 = 8.0;
            let cols = ((self.window_width - spacing) / (tile_width + spacing)).floor() as usize;
            let cols = if cols < 1 { 1 } else { cols };

            let row_count = total.div_ceil(cols);

            let mut rows_vec: Vec<Element<'_, Message>> = Vec::with_capacity(row_count);
            for row_idx in 0..row_count {
                let tiles: Vec<Element<'_, Message>> = games
                    .iter()
                    .skip(row_idx * cols)
                    .take(cols)
                    .map(|g| self.game_tile(g, tile_width))
                    .collect();
                rows_vec.push(row(tiles).spacing(spacing).width(Fill).into());
            }

            let content = column(rows_vec).spacing(spacing).padding(8);

            if row_count <= 1 {
                container(content)
                    .width(Fill)
                    .height(Fill)
                    .center_y(Fill)
                    .into()
            } else {
                scrollable(content).height(Fill).into()
            }
        }
    }

    fn game_tile<'a>(&'a self, g: &'a Game, width: f32) -> Element<'a, Message> {
        let selected = self.selection.contains(&g.id);
        let drag_source = self.is_drag_source(&g.id);
        let drag_target = self.is_drag_target(&g.id);
        let preview_game = self.drag_preview_game(&g.id);
        let display_game = preview_game.unwrap_or(g);
        let is_preview = preview_game.is_some();
        let icon_el: Element<'static, Message> = self.icon_widget(
            &display_game.icon,
            self.store.config_path.parent().unwrap_or(Path::new(".")),
            64,
        );
        let name = truncate_tile_name(&display_game.name);
        let title = text(name).size(12).align_x(Center);
        let preview_badge: Element<'_, Message> = if is_preview {
            text("PREVIEW")
                .size(10)
                .style(|_| text::Style {
                    color: Some(Color::from_rgb8(0xf0, 0xc6, 0x63)),
                })
                .into()
        } else {
            text("").size(10).into()
        };
        let inner = column![preview_badge, icon_el, title]
            .spacing(4)
            .align_x(Center)
            .width(Length::Fixed(width));
        let bg = if drag_target {
            Color::from_rgb8(0x5a, 0x56, 0x1f)
        } else if drag_source {
            Color::from_rgb8(0x1b, 0x28, 0x38)
        } else if selected {
            Color::from_rgb8(0x2e, 0x7d, 0x5a)
        } else {
            Color::from_rgb8(0x1a, 0x1a, 0x2e)
        };
        let border_color = if drag_target {
            Color::from_rgb8(0xd4, 0xb4, 0x44)
        } else if drag_source {
            Color::from_rgb8(0x62, 0x86, 0xa6)
        } else if selected {
            Color::from_rgb8(0x4a, 0x80, 0xa0)
        } else {
            Color::from_rgb8(0x30, 0x38, 0x50)
        };
        let id = g.id.clone();
        mouse_area(
            container(inner)
                .padding(8)
                .width(Length::Fixed(width))
                .style(move |_theme| container::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        radius: 8.0.into(),
                        color: border_color,
                        width: 1.0,
                    },
                    ..Default::default()
                }),
        )
        .on_press(Message::GamePressed(id.clone()))
        .on_right_press(Message::GameRightPressed(id.clone()))
        .on_enter(Message::DragEntered(id.clone()))
        .on_exit(Message::DragExited(id))
        .into()
    }
}

fn truncate_tile_name(value: &str) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(12).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        value.to_string()
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
            checkbox("", r.enabled).on_toggle(BatchMsg::Toggle).size(18),
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
