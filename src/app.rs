use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ::image::imageops::FilterType;
use ::image::io::Reader as ImageReader;
use iced::keyboard::key::Named;
use iced::widget::{
    button, checkbox, column, container, horizontal_space, image, lazy, mouse_area, opaque, row,
    scrollable, stack, text, text_input, vertical_space,
};
use iced::window;
use iced::{
    event, keyboard, mouse, Alignment, Color, ContentFit, Element, Event, Length, Point,
    Subscription, Task, Theme,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    preview_item: GameItemView,
    hover_id: Option<String>,
    started_at: Point,
    is_active: bool,
}

#[derive(Debug, Clone)]
struct IconVisual {
    key: String,
    handle: Option<image::Handle>,
    placeholder: String,
}

impl Hash for IconVisual {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
        self.placeholder.hash(state);
    }
}

#[derive(Debug, Clone, Hash)]
struct GameItemView {
    id: String,
    name: String,
    path: String,
    source_label: String,
    source_badge: String,
    icon: IconVisual,
}

#[derive(Debug, Clone, Hash)]
struct LibraryInteractionState {
    selected_ids: Vec<String>,
    hovered_game_id: Option<String>,
    drag_source_id: Option<String>,
    drag_hover_id: Option<String>,
    drag_active: bool,
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
    SearchChanged(String),
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
    hovered_game_id: Option<String>,
    last_click: Option<(String, Instant)>,
    icon_cache: RefCell<HashMap<String, image::Handle>>,
    library_snapshot: Rc<Vec<GameItemView>>,
    library_revision: u64,
    search_query: String,
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
        let mut launcher = Self {
            app_dir,
            store,
            view_mode: ViewMode::Tiles,
            window_width: 920.0,
            selection: HashSet::new(),
            modifiers: keyboard::Modifiers::default(),
            cursor_position: None,
            drag_state: None,
            hovered_game_id: None,
            last_click: None,
            icon_cache: RefCell::new(HashMap::new()),
            library_snapshot: Rc::new(Vec::new()),
            library_revision: 0,
            search_query: String::new(),
            modal,
            mode: settings.mode,
        };
        launcher.refresh_library_snapshot();
        (launcher, Task::none())
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
                self.hovered_game_id = None;
                self.last_click = None;
                self.refresh_library_snapshot();
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
                let preview_item = self
                    .store
                    .get(&id)
                    .map(|game| self.game_item_view_for(game));
                self.drag_state = preview_item.map(|preview_item| DragState {
                    source_id: id,
                    preview_item,
                    hover_id: None,
                    started_at: Point::new(f32::NAN, f32::NAN),
                    is_active: false,
                });
                Task::none()
            }
            Message::GameRightPressed(id) => {
                self.selection.clear();
                self.selection.insert(id);
                self.drag_state = None;
                self.cursor_position = None;
                Task::none()
            }
            Message::DragEntered(id) => {
                let drag_active = self.drag_state.as_ref().is_some_and(|drag| drag.is_active);
                if drag_active {
                    if let Some(drag) = &mut self.drag_state {
                        if drag.hover_id.as_deref() != Some(id.as_str()) {
                            drag.hover_id = Some(id);
                        }
                    }
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
                if let Some(drag) = &mut self.drag_state {
                    let rounded = Point::new(position.x.round(), position.y.round());
                    let cursor_changed = self
                        .cursor_position
                        .is_none_or(|current| current.x != rounded.x || current.y != rounded.y);
                    if cursor_changed {
                        self.cursor_position = Some(rounded);
                    }
                    if drag.started_at.x.is_nan() || drag.started_at.y.is_nan() {
                        drag.started_at = position;
                        return Task::none();
                    }
                    if !drag.is_active {
                        let delta = position - drag.started_at;
                        if delta.x.abs() > 6.0 || delta.y.abs() > 6.0 {
                            drag.is_active = true;
                            drag.hover_id = Some(drag.source_id.clone());
                            self.hovered_game_id = None;
                            self.last_click = None;
                        }
                    }
                }
                Task::none()
            }
            Message::PointerReleased => {
                let had_drag_feedback = self
                    .drag_state
                    .as_ref()
                    .is_some_and(|drag| drag.is_active || drag.hover_id.is_some())
                    || self.hovered_game_id.is_some();
                let reordered = self.finish_drag_reorder();
                self.cursor_position = None;
                self.hovered_game_id = None;
                if reordered {
                    self.refresh_library_snapshot();
                } else if had_drag_feedback {
                    self.drag_state = None;
                }
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
                    self.refresh_library_snapshot();
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
                        Ok(()) => self.refresh_library_snapshot(),
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
                    } else {
                        self.refresh_library_snapshot();
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
                self.refresh_library_snapshot();
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
            Message::SearchChanged(query) => {
                self.search_query = query;
                self.refresh_library_snapshot();
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

    fn finish_drag_reorder(&mut self) -> bool {
        let Some(drag) = self.drag_state.take() else {
            return false;
        };

        if !drag.is_active {
            return false;
        }

        let Some(target_id) = drag.hover_id else {
            return false;
        };

        if target_id == drag.source_id {
            return false;
        }

        let Some(target_index) = self.game_index(&target_id) else {
            return false;
        };

        if let Err(error) = self.store.move_game_to(&drag.source_id, target_index) {
            self.modal = Modal::Alert(error.to_string());
            return false;
        }

        true
    }

    fn drag_source_item_view(&self) -> Option<&GameItemView> {
        let drag = self.drag_state.as_ref()?;
        if !drag.is_active {
            return None;
        }

        Some(&drag.preview_item)
    }

    fn visible_games(&self) -> Vec<&Game> {
        let query = self.search_query.trim();
        if query.is_empty() {
            return self.store.games.iter().collect();
        }

        let query = query.to_lowercase();
        self.store
            .games
            .iter()
            .filter(|game| {
                game.name.to_lowercase().contains(&query)
                    || game.path.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn icon_visual(&self, game: &Game) -> IconVisual {
        let config_parent = self.store.config_path.parent().unwrap_or(Path::new("."));
        let path = drop_resolve::normalize_icon_path_for_preview(&game.icon, config_parent);

        if game.icon.trim().is_empty() || !path.exists() {
            return IconVisual {
                key: format!("placeholder:{}", game.name),
                handle: None,
                placeholder: placeholder_letter(&game.name),
            };
        }

        let key = path.to_string_lossy().to_string();
        let handle = {
            let mut cache = self.icon_cache.borrow_mut();
            cache
                .entry(key.clone())
                .or_insert_with(|| load_icon_thumbnail(&path))
                .clone()
        };

        IconVisual {
            key,
            handle: Some(handle),
            placeholder: placeholder_letter(&game.name),
        }
    }

    fn game_item_view_for(&self, game: &Game) -> GameItemView {
        GameItemView {
            id: game.id.clone(),
            name: game.name.clone(),
            path: game.path.clone(),
            source_label: source_label(&game.path),
            source_badge: source_badge_label(&game.path),
            icon: self.icon_visual(game),
        }
    }

    fn library_items(&self) -> Vec<GameItemView> {
        self.visible_games()
            .into_iter()
            .map(|game| self.game_item_view_for(game))
            .collect()
    }

    fn refresh_library_snapshot(&mut self) {
        self.library_snapshot = Rc::new(self.library_items());
        self.library_revision = self.library_revision.wrapping_add(1);
    }

    fn library_interaction_state(&self) -> LibraryInteractionState {
        let mut selected_ids = self.selection.iter().cloned().collect::<Vec<_>>();
        selected_ids.sort();

        LibraryInteractionState {
            selected_ids,
            hovered_game_id: self.hovered_game_id.clone(),
            drag_source_id: self.drag_state.as_ref().map(|drag| drag.source_id.clone()),
            drag_hover_id: self
                .drag_state
                .as_ref()
                .and_then(|drag| drag.hover_id.clone()),
            drag_active: self.drag_state.as_ref().is_some_and(|drag| drag.is_active),
        }
    }

    fn empty_library_state(&self) -> Element<'_, Message> {
        let content = column![
            text("Библиотека пуста").size(26).style(|_| text::Style {
                color: Some(text_color()),
            }),
            text("Добавь игру вручную или перетащи сюда ярлык, exe или url.")
                .size(14)
                .style(|_| text::Style {
                    color: Some(muted_text_color()),
                }),
            toolbar_button(
                "Добавить игру",
                Some(Message::AddPressed),
                ButtonTone::Primary
            ),
        ]
        .spacing(14)
        .align_x(Center)
        .max_width(420);

        container(
            column![
                container(text("+").size(28).style(|_| text::Style {
                    color: Some(primary_color()),
                }),)
                .width(Length::Fixed(52.0))
                .height(Length::Fixed(52.0))
                .center_x(Fill)
                .center_y(Fill)
                .style(|_| soft_card_style(
                    surface_alt(),
                    blend(primary_color(), border_strong(), 0.35),
                    16.0
                )),
                content,
            ]
            .spacing(18)
            .align_x(Center),
        )
        .padding(32)
        .center_x(Fill)
        .center_y(Fill)
        .style(|_| panel_style(surface()))
        .into()
    }

    fn empty_search_state(&self) -> Element<'_, Message> {
        let content = column![
            text("Ничего не найдено").size(22).style(|_| text::Style {
                color: Some(text_color()),
            }),
            text("Попробуй другое название, путь или очисти поиск.")
                .size(14)
                .style(|_| text::Style {
                    color: Some(muted_text_color()),
                }),
            toolbar_button(
                "Сбросить поиск",
                Some(Message::SearchChanged(String::new())),
                ButtonTone::Secondary,
            ),
        ]
        .spacing(12)
        .align_x(Center)
        .max_width(360);

        container(
            column![
                container(text("?").size(24).style(|_| text::Style {
                    color: Some(accent_color()),
                }),)
                .width(Length::Fixed(52.0))
                .height(Length::Fixed(52.0))
                .center_x(Fill)
                .center_y(Fill)
                .style(|_| soft_card_style(
                    surface_alt(),
                    blend(accent_color(), border_strong(), 0.35),
                    16.0
                )),
                content,
            ]
            .spacing(18)
            .align_x(Center),
        )
        .padding(32)
        .center_x(Fill)
        .center_y(Fill)
        .style(|_| panel_style(surface()))
        .into()
    }

    fn modern_modal_overlay<'a>(
        &self,
        base: Element<'a, Message>,
        title: &'a str,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        stack![
            opaque(base),
            opaque(
                container("")
                    .width(Fill)
                    .height(Fill)
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba8(4, 6, 10, 0.78).into()),
                        ..Default::default()
                    })
            ),
            opaque(
                container(
                    column![
                        text(title).size(22).style(|_| text::Style {
                            color: Some(text_color()),
                        }),
                        content,
                    ]
                    .spacing(18)
                    .max_width(620),
                )
                .padding(26)
                .style(|_| soft_card_style(surface(), border_strong(), 18.0))
                .center_x(Fill)
                .center_y(Fill)
                .width(Length::Fill)
                .height(Length::Fill)
            )
        ]
        .into()
    }

    fn modern_view(&self) -> Element<'_, Message> {
        let gate_ok = is_gate_open(&self.app_dir);
        let can_launch = gate_ok && self.selection.len() == 1;
        let has_selection = !self.selection.is_empty();

        let header = row![
            column![
                text("Game Launcher").size(28).style(|_| text::Style {
                    color: Some(text_color()),
                }),
                row![
                    pill(
                        match self.mode {
                            AppMode::Debug => "Debug",
                            AppMode::Release => "Release",
                        },
                        BadgeTone::Neutral,
                    ),
                    pill(
                        if gate_ok {
                            "Задание выполнено"
                        } else {
                            "Сначала выполни задание"
                        },
                        if gate_ok {
                            BadgeTone::Success
                        } else {
                            BadgeTone::Warning
                        },
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(10),
            horizontal_space(),
            container(
                text_input("Поиск игр", &self.search_query)
                    .on_input(Message::SearchChanged)
                    .padding([10, 14])
                    .size(14)
                    .style(|_, status| input_style(status)),
            )
            .width(Length::Fixed(300.0))
            .style(|_| panel_style(surface())),
        ]
        .align_y(Alignment::Center);

        let toolbar = container(
            row![
                row![
                    toolbar_button(
                        "Запустить",
                        can_launch.then_some(Message::Launch),
                        ButtonTone::Primary
                    ),
                    toolbar_button("Добавить", Some(Message::AddPressed), ButtonTone::Secondary),
                    toolbar_button(
                        "Несколько",
                        Some(Message::AddPickMany),
                        ButtonTone::Secondary
                    ),
                    toolbar_button(
                        "Изменить",
                        Some(Message::EditPressed),
                        ButtonTone::Secondary
                    ),
                    toolbar_button(
                        "Удалить",
                        has_selection.then_some(Message::RemovePressed),
                        ButtonTone::Danger,
                    ),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                horizontal_space(),
            ]
            .align_y(Alignment::Center),
        )
        .padding(12)
        .style(|_| panel_style(surface()));

        let body: Element<'_, Message> = if self.store.games.is_empty() {
            self.empty_library_state()
        } else if self.library_snapshot.is_empty() {
            self.empty_search_state()
        } else {
            let interaction_state = self.library_interaction_state();
            let body_dependency = (
                self.view_mode,
                self.window_width.round() as i32,
                self.library_revision,
                interaction_state.clone(),
            );
            let body_snapshot = Rc::clone(&self.library_snapshot);
            let view_mode = self.view_mode;
            let window_width = self.window_width;
            let interactions = interaction_state;
            lazy(body_dependency, move |_| {
                render_library_body(
                    view_mode,
                    window_width,
                    Rc::clone(&body_snapshot),
                    interactions.clone(),
                )
            })
            .into()
        };

        let library_count = self.library_snapshot.len();
        let selected_count = self.selection.len();
        let library_subtitle = if selected_count == 0 {
            format!("{library_count} игр в библиотеке")
        } else {
            format!("{library_count} игр в библиотеке · выбрано: {selected_count}")
        };

        let library_header = row![
            column![
                text("Библиотека").size(22).style(|_| text::Style {
                    color: Some(text_color()),
                }),
                text(library_subtitle).size(12).style(|_| text::Style {
                    color: Some(muted_text_color()),
                }),
            ]
            .spacing(4),
            horizontal_space(),
            segmented_control(self.view_mode),
        ]
        .align_y(Alignment::Center);

        let library_panel = container(
            column![library_header, container(body).height(Fill)]
                .spacing(16)
                .height(Fill),
        )
        .height(Fill)
        .padding(18)
        .style(|_| panel_style(surface()));

        let main = container(
            column![
                header,
                toolbar,
                library_panel,
                container(
                    row![
                        text("Данные").size(11).style(|_| text::Style {
                            color: Some(blend(muted_text_color(), text_color(), 0.18)),
                        }),
                        text(self.store.config_path.display().to_string())
                            .size(11)
                            .style(|_| text::Style {
                                color: Some(muted_text_color()),
                            }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .height(Length::Fixed(30.0))
                .padding([4, 6])
                .style(|_| status_bar_style()),
            ]
            .spacing(16),
        )
        .width(Fill)
        .height(Fill)
        .padding(18)
        .style(|_| container::Style {
            background: Some(background_color().into()),
            ..Default::default()
        });

        let root = match &self.modal {
            Modal::None => opaque(main).into(),
            Modal::Alert(message) => self.modern_modal_overlay(
                main.into(),
                "Сообщение",
                column![
                    text(message).size(14).style(|_| text::Style {
                        color: Some(text_color()),
                    }),
                    row![
                        horizontal_space(),
                        toolbar_button("OK", Some(Message::DismissAlert), ButtonTone::Primary)
                    ]
                    .align_y(Alignment::Center),
                ]
                .spacing(16)
                .into(),
            ),
            Modal::ConfirmDelete { ids } => self.modern_modal_overlay(
                main.into(),
                "Удаление игр",
                column![
                    text(format!("Удалить {} игр(ы) из библиотеки?", ids.len()))
                        .size(14)
                        .style(|_| text::Style {
                            color: Some(text_color()),
                        }),
                    row![
                        horizontal_space(),
                        toolbar_button(
                            "Отмена",
                            Some(Message::ConfirmDeleteNo),
                            ButtonTone::Secondary
                        ),
                        toolbar_button(
                            "Удалить",
                            Some(Message::ConfirmDeleteYes),
                            ButtonTone::Danger
                        ),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                ]
                .spacing(18)
                .into(),
            ),
            Modal::AddManual { name, path, icon } => self.modern_modal_overlay(
                main.into(),
                "Добавить игру",
                column![
                    modal_label("Название"),
                    text_input("Название игры", name)
                        .on_input(Message::AddName)
                        .padding([10, 12])
                        .size(14)
                        .style(|_, status| input_style(status)),
                    modal_label("Путь"),
                    row![
                        text_input("Путь к .exe / .lnk / .url", path)
                            .on_input(Message::AddPath)
                            .padding([10, 12])
                            .size(14)
                            .style(|_, status| input_style(status))
                            .width(Fill),
                        toolbar_button("Обзор", Some(Message::AddPickExe), ButtonTone::Secondary),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                    modal_label("Иконка"),
                    row![
                        text_input("Путь к иконке", icon)
                            .on_input(Message::AddIcon)
                            .padding([10, 12])
                            .size(14)
                            .style(|_, status| input_style(status))
                            .width(Fill),
                        toolbar_button("Обзор", Some(Message::AddPickIcon), ButtonTone::Secondary),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                    row![
                        horizontal_space(),
                        toolbar_button("Отмена", Some(Message::AddCancel), ButtonTone::Secondary),
                        toolbar_button("Добавить", Some(Message::AddConfirm), ButtonTone::Primary),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                ]
                .spacing(12)
                .into(),
            ),
            Modal::Edit {
                id,
                name,
                icon_path,
            } => {
                let path_label = self
                    .store
                    .get(id.as_str())
                    .map(|game| game.path.as_str())
                    .unwrap_or("");
                self.modern_modal_overlay(
                    main.into(),
                    "Изменить игру",
                    column![
                        modal_label("Название"),
                        text_input("Название игры", name)
                            .on_input(Message::EditName)
                            .padding([10, 12])
                            .size(14)
                            .style(|_, status| input_style(status)),
                        modal_label("Путь"),
                        container(text(path_label).size(13).style(|_| text::Style {
                            color: Some(muted_text_color()),
                        }),)
                        .padding([10, 12])
                        .style(|_| container::Style {
                            background: Some(surface_alt().into()),
                            border: iced::Border {
                                radius: 12.0.into(),
                                color: border_color(),
                                width: 1.0,
                            },
                            ..Default::default()
                        }),
                        modal_label("Иконка"),
                        row![
                            text_input("Путь к иконке", icon_path)
                                .on_input(Message::EditIcon)
                                .padding([10, 12])
                                .size(14)
                                .style(|_, status| input_style(status))
                                .width(Fill),
                            toolbar_button(
                                "Обзор",
                                Some(Message::EditPickIcon),
                                ButtonTone::Secondary
                            ),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                        row![
                            toolbar_button(
                                "Сбросить иконку",
                                Some(Message::EditClearIcon),
                                ButtonTone::Secondary
                            ),
                            horizontal_space(),
                            toolbar_button(
                                "Отмена",
                                Some(Message::EditCancel),
                                ButtonTone::Secondary
                            ),
                            toolbar_button(
                                "Сохранить",
                                Some(Message::EditSave),
                                ButtonTone::Primary
                            ),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(12)
                    .into(),
                )
            }
            Modal::Batch { rows } => self.modern_modal_overlay(
                main.into(),
                "Добавить несколько игр",
                column![
                    text(format!("Подготовлено к добавлению: {}", rows.len()))
                        .size(14)
                        .style(|_| text::Style {
                            color: Some(muted_text_color()),
                        }),
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
                        .spacing(10),
                    )
                    .height(Length::Fixed(320.0)),
                    row![
                        horizontal_space(),
                        toolbar_button("Отмена", Some(Message::BatchCancel), ButtonTone::Secondary),
                        toolbar_button(
                            "Добавить выбранные",
                            Some(Message::BatchConfirm),
                            ButtonTone::Primary
                        ),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                ]
                .spacing(14)
                .into(),
            ),
        };

        let root: Element<'_, Message> = container(root).width(Fill).height(Fill).into();
        if let Some(drag_overlay) = self.drag_overlay() {
            stack![root, drag_overlay].into()
        } else {
            root
        }
    }

    fn drag_tile_overlay(&self, item: &GameItemView) -> Element<'static, Message> {
        let icon_frame = render_icon_frame(&item.icon, 84.0, 72);
        let label = column![
            pill("Перетаскивание", BadgeTone::Accent),
            text(truncate_tile_name(&item.name))
                .size(14)
                .align_x(Center)
                .style(|_| text::Style {
                    color: Some(text_color()),
                }),
            text(item.source_label.clone())
                .size(11)
                .align_x(Center)
                .style(|_| text::Style {
                    color: Some(muted_text_color()),
                }),
        ]
        .spacing(8)
        .align_x(Center);

        container(column![icon_frame, label].spacing(14).align_x(Center))
            .padding([12, 14])
            .width(Length::Fixed(136.0))
            .height(Length::Fixed(162.0))
            .style(|_| container::Style {
                text_color: Some(text_color()),
                background: Some(surface_alt().into()),
                border: iced::Border {
                    radius: 16.0.into(),
                    color: border_strong(),
                    width: 1.0,
                },
                ..Default::default()
            })
            .into()
    }

    fn drag_row_overlay(&self, item: &GameItemView) -> Element<'static, Message> {
        let icon_frame = render_icon_frame(&item.icon, 56.0, 48);
        let badge = pill("Перетаскивание", BadgeTone::Accent);
        let name = text(item.name.clone()).size(15).style(|_| text::Style {
            color: Some(text_color()),
        });
        let path = text(item.path.clone()).size(12).style(|_| text::Style {
            color: Some(muted_text_color()),
        });

        container(
            row![
                icon_frame,
                column![badge, name, path].spacing(2).width(Fill),
            ]
            .spacing(14)
            .align_y(Alignment::Center)
            .padding([14, 16]),
        )
        .width(Length::Fixed(420.0))
        .height(Length::Fixed(78.0))
        .style(|_| container::Style {
            text_color: Some(text_color()),
            background: Some(surface_alt().into()),
            border: iced::Border {
                radius: 16.0.into(),
                color: border_strong(),
                width: 1.0,
            },
            ..Default::default()
        })
        .into()
    }

    fn drag_overlay(&self) -> Option<Element<'static, Message>> {
        let cursor = self.cursor_position?;
        let item = self.drag_source_item_view()?;

        let (preview, width, height, offset_x, offset_y) = match self.view_mode {
            ViewMode::Tiles => (self.drag_tile_overlay(item), 136.0, 162.0, 68.0, 54.0),
            ViewMode::List => (self.drag_row_overlay(item), 420.0, 78.0, 42.0, 26.0),
        };

        let left = (cursor.x - offset_x).round().max(0.0);
        let top = (cursor.y - offset_y).round().max(0.0);

        Some(
            container(column![
                vertical_space().height(Length::Fixed(top)),
                row![
                    horizontal_space().width(Length::Fixed(left)),
                    container(preview)
                        .width(Length::Fixed(width))
                        .height(Length::Fixed(height)),
                    horizontal_space(),
                ],
                vertical_space(),
            ])
            .width(Fill)
            .height(Fill)
            .into(),
        )
    }

    fn view(&self) -> Element<'_, Message> {
        self.modern_view()
    }
}

fn render_library_body(
    view_mode: ViewMode,
    window_width: f32,
    items: Rc<Vec<GameItemView>>,
    interactions: LibraryInteractionState,
) -> Element<'static, Message> {
    match view_mode {
        ViewMode::List => render_list_body(items, interactions),
        ViewMode::Tiles => render_tiles_body(window_width, items, interactions),
    }
}

fn render_list_body(
    items: Rc<Vec<GameItemView>>,
    interactions: LibraryInteractionState,
) -> Element<'static, Message> {
    let source_item = drag_source_item(&items, &interactions).cloned();
    let rows = items
        .iter()
        .map(|item| render_game_row(item, &interactions, source_item.as_ref()))
        .collect::<Vec<Element<'static, Message>>>();

    scrollable(column(rows).spacing(12).padding([2, 4]))
        .height(Fill)
        .into()
}

fn render_tiles_body(
    window_width: f32,
    items: Rc<Vec<GameItemView>>,
    interactions: LibraryInteractionState,
) -> Element<'static, Message> {
    let tile_width = 136.0;
    let spacing = 14.0;
    let available_width = (window_width - 92.0).max(tile_width);
    let cols = (((available_width + spacing) / (tile_width + spacing)).floor() as usize).max(1);
    let source_item = drag_source_item(&items, &interactions).cloned();

    let rows = items
        .chunks(cols)
        .map(|chunk| {
            row(chunk
                .iter()
                .map(|item| render_game_tile(item, tile_width, &interactions, source_item.as_ref()))
                .collect::<Vec<Element<'static, Message>>>())
            .spacing(spacing)
            .width(Length::Shrink)
            .into()
        })
        .collect::<Vec<Element<'static, Message>>>();

    scrollable(
        container(
            column(rows)
                .spacing(spacing)
                .padding([4, 2])
                .width(Length::Shrink),
        )
        .width(Fill),
    )
    .height(Fill)
    .into()
}

fn drag_source_item<'a>(
    items: &'a [GameItemView],
    interactions: &LibraryInteractionState,
) -> Option<&'a GameItemView> {
    let source_id = interactions.drag_source_id.as_deref()?;
    items.iter().find(|item| item.id == source_id)
}

fn is_selected(interactions: &LibraryInteractionState, id: &str) -> bool {
    interactions
        .selected_ids
        .binary_search_by(|candidate| candidate.as_str().cmp(id))
        .is_ok()
}

fn item_drag_source(interactions: &LibraryInteractionState, id: &str) -> bool {
    interactions.drag_active && interactions.drag_source_id.as_deref() == Some(id)
}

fn item_drag_target(interactions: &LibraryInteractionState, id: &str) -> bool {
    interactions.drag_active
        && interactions.drag_source_id.as_deref() != Some(id)
        && interactions.drag_hover_id.as_deref() == Some(id)
}

fn display_item_for<'a>(
    item: &'a GameItemView,
    interactions: &LibraryInteractionState,
    source_item: Option<&'a GameItemView>,
) -> (&'a GameItemView, bool) {
    if item_drag_target(interactions, &item.id) {
        if let Some(source_item) = source_item {
            return (source_item, true);
        }
    }

    (item, false)
}

fn render_game_row(
    item: &GameItemView,
    interactions: &LibraryInteractionState,
    source_item: Option<&GameItemView>,
) -> Element<'static, Message> {
    let selected = is_selected(interactions, &item.id);
    let hovered = !interactions.drag_active
        && interactions.hovered_game_id.as_deref() == Some(item.id.as_str());
    let drag_source = item_drag_source(interactions, &item.id);
    let drag_target = item_drag_target(interactions, &item.id);
    let (display_item, is_preview) = display_item_for(item, interactions, source_item);

    let bg = if drag_target {
        blend(accent_color(), surface_alt(), 0.14)
    } else if drag_source {
        blend(background_color(), surface_alt(), 0.28)
    } else if selected {
        selected_surface()
    } else if hovered {
        surface_hover()
    } else {
        surface_alt()
    };

    let border = if drag_target {
        accent_color()
    } else if selected {
        primary_color()
    } else if hovered {
        blend(primary_color(), border_color(), 0.25)
    } else {
        border_color()
    };

    let title_block = column![
        row![
            text(display_item.name.clone())
                .size(16)
                .style(|_| text::Style {
                    color: Some(text_color()),
                }),
            horizontal_space(),
            pill(
                if is_preview {
                    "Preview".to_string()
                } else {
                    display_item.source_badge.clone()
                },
                if is_preview {
                    BadgeTone::Accent
                } else {
                    BadgeTone::Neutral
                },
            ),
        ]
        .align_y(Alignment::Center),
        row![text(display_item.path.clone())
            .size(12)
            .style(|_| text::Style {
                color: Some(muted_text_color()),
            }),]
        .align_y(Alignment::Center),
    ]
    .spacing(7)
    .width(Fill);

    let id = item.id.clone();
    let row = mouse_area(
        container(
            row![
                container(render_icon_frame(&display_item.icon, 56.0, 46)).padding([0, 2]),
                title_block,
            ]
            .spacing(14)
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .height(Length::Fixed(78.0))
        .padding([10, 16])
        .style(move |_| container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                radius: 16.0.into(),
                color: border,
                width: 1.0,
            },
            ..Default::default()
        }),
    )
    .on_press(Message::GamePressed(id.clone()))
    .on_right_press(Message::GameRightPressed(id.clone()));

    let row = if interactions.drag_active {
        row.on_enter(Message::DragEntered(id.clone()))
            .on_exit(Message::DragExited(id))
    } else {
        row
    };

    row.into()
}

fn render_game_tile(
    item: &GameItemView,
    width: f32,
    interactions: &LibraryInteractionState,
    source_item: Option<&GameItemView>,
) -> Element<'static, Message> {
    let selected = is_selected(interactions, &item.id);
    let hovered = !interactions.drag_active
        && interactions.hovered_game_id.as_deref() == Some(item.id.as_str());
    let drag_source = item_drag_source(interactions, &item.id);
    let drag_target = item_drag_target(interactions, &item.id);
    let (display_item, is_preview) = display_item_for(item, interactions, source_item);

    let bg = if drag_target {
        blend(accent_color(), surface_alt(), 0.14)
    } else if drag_source {
        blend(background_color(), surface_alt(), 0.3)
    } else if selected {
        selected_surface()
    } else if hovered {
        surface_hover()
    } else {
        surface_alt()
    };

    let border = if drag_target {
        accent_color()
    } else if selected {
        primary_color()
    } else if hovered {
        blend(primary_color(), border_color(), 0.25)
    } else {
        border_color()
    };

    let id = item.id.clone();
    let tile = mouse_area(
        container(
            column![
                row![
                    pill(
                        if is_preview {
                            "Preview".to_string()
                        } else {
                            display_item.source_badge.clone()
                        },
                        if is_preview {
                            BadgeTone::Accent
                        } else {
                            BadgeTone::Neutral
                        },
                    ),
                    horizontal_space(),
                ]
                .width(Fill)
                .align_y(Alignment::Center),
                container(render_icon_frame(&display_item.icon, 78.0, 68))
                    .width(Fill)
                    .height(Length::Fixed(82.0))
                    .center_x(Fill),
                container(
                    column![
                        text(truncate_tile_name(&display_item.name))
                            .size(14)
                            .align_x(Center)
                            .style(|_| text::Style {
                                color: Some(text_color()),
                            }),
                        text(display_item.source_label.clone())
                            .size(11)
                            .align_x(Center)
                            .style(|_| text::Style {
                                color: Some(muted_text_color()),
                            }),
                    ]
                    .spacing(4)
                    .align_x(Center)
                    .width(Fill),
                )
                .width(Fill)
                .height(Length::Fixed(44.0))
                .center_x(Fill),
            ]
            .spacing(10)
            .align_x(Center)
            .width(Fill),
        )
        .padding([12, 12])
        .width(Length::Fixed(width))
        .height(Length::Fixed(168.0))
        .style(move |_| container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                radius: 18.0.into(),
                color: border,
                width: 1.0,
            },
            ..Default::default()
        }),
    )
    .on_press(Message::GamePressed(id.clone()))
    .on_right_press(Message::GameRightPressed(id.clone()));

    let tile = if interactions.drag_active {
        tile.on_enter(Message::DragEntered(id.clone()))
            .on_exit(Message::DragExited(id))
    } else {
        tile
    };

    tile.into()
}

fn render_icon_widget(icon: &IconVisual, size: u16) -> Element<'static, Message> {
    if let Some(handle) = &icon.handle {
        image(handle.clone())
            .width(Length::Fixed(f32::from(size)))
            .height(Length::Fixed(f32::from(size)))
            .content_fit(ContentFit::Contain)
            .into()
    } else {
        container(
            text(icon.placeholder.clone())
                .size((size / 2).max(18))
                .style(|_| text::Style {
                    color: Some(text_color()),
                }),
        )
        .width(Length::Fixed(f32::from(size)))
        .height(Length::Fixed(f32::from(size)))
        .center_x(Fill)
        .center_y(Fill)
        .into()
    }
}

fn render_icon_frame(
    icon: &IconVisual,
    frame_size: f32,
    icon_size: u16,
) -> Element<'static, Message> {
    container(
        container(render_icon_widget(icon, icon_size))
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill),
    )
    .width(Length::Fixed(frame_size))
    .height(Length::Fixed(frame_size))
    .style(|_| container::Style {
        background: Some(blend(background_color(), surface(), 0.36).into()),
        border: iced::Border {
            radius: 14.0.into(),
            color: blend(primary_color(), border_color(), 0.08),
            width: 1.0,
        },
        ..Default::default()
    })
    .into()
}

fn load_icon_thumbnail(path: &Path) -> image::Handle {
    const THUMB_SIZE: u32 = 96;

    let decoded = ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.decode().ok());

    if let Some(decoded) = decoded {
        let thumbnail = decoded.resize(THUMB_SIZE, THUMB_SIZE, FilterType::Triangle);
        let rgba = thumbnail.to_rgba8();
        let (width, height) = rgba.dimensions();

        return image::Handle::from_rgba(width, height, rgba.into_raw());
    }

    image::Handle::from_path(path)
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
    container(
        column![
            row![
                checkbox("", r.enabled).on_toggle(BatchMsg::Toggle).size(18),
                container(text(format!("#{}", i + 1)).size(11).style(|_| text::Style {
                    color: Some(muted_text_color()),
                }),)
                .padding([4, 8])
                .style(|_| soft_card_style(surface(), border_color(), 999.0)),
                horizontal_space(),
            ]
            .align_y(Alignment::Center),
            text_input("Название", &r.name)
                .on_input(BatchMsg::Name)
                .padding([9, 10])
                .style(|_, status| input_style(status)),
            text(&r.target_path).size(11).style(|_| text::Style {
                color: Some(muted_text_color()),
            }),
            row![
                text_input("Иконка", &r.icon_source)
                    .on_input(BatchMsg::Icon)
                    .padding([9, 10])
                    .style(|_, status| input_style(status))
                    .width(Fill),
                button(text("Обзор").size(13))
                    .padding([9, 12])
                    .on_press(BatchMsg::PickIcon)
                    .style(|_, status| toolbar_button_style(ButtonTone::Secondary, status, true)),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(10),
    )
    .padding(12)
    .style(|_| soft_card_style(surface_alt(), border_color(), 14.0))
    .into()
}

fn placeholder_letter(name: &str) -> String {
    name.chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "#".to_string())
}

fn source_label(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with("steam://") {
        return "Steam".to_string();
    }
    if let Some((scheme, _)) = trimmed.split_once("://") {
        return scheme.to_uppercase();
    }

    let path = Path::new(trimmed);
    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
        let stem = stem.trim();
        if !stem.is_empty() {
            return stem.to_string();
        }
    }

    "Локальная игра".to_string()
}

fn source_badge_label(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with("steam://") {
        return "Steam".to_string();
    }
    if let Some((scheme, _)) = trimmed.split_once("://") {
        return scheme.to_uppercase();
    }
    "Local".to_string()
}

fn background_color() -> Color {
    Color::from_rgb8(0x0E, 0x0D, 0x0B)
}

fn surface() -> Color {
    Color::from_rgb8(0x17, 0x15, 0x12)
}

fn surface_alt() -> Color {
    Color::from_rgb8(0x20, 0x1D, 0x18)
}

fn surface_hover() -> Color {
    Color::from_rgb8(0x2A, 0x26, 0x20)
}

fn border_color() -> Color {
    Color::from_rgb8(0x3B, 0x35, 0x2B)
}

fn border_strong() -> Color {
    Color::from_rgb8(0x57, 0x4D, 0x3E)
}

fn primary_color() -> Color {
    Color::from_rgb8(0xC6, 0xB0, 0x88)
}

fn primary_hover_color() -> Color {
    Color::from_rgb8(0xD7, 0xC3, 0x9F)
}

fn accent_color() -> Color {
    Color::from_rgb8(0xA9, 0x8A, 0x62)
}

fn text_color() -> Color {
    Color::from_rgb8(0xF2, 0xEE, 0xE7)
}

fn muted_text_color() -> Color {
    Color::from_rgb8(0xA8, 0x9F, 0x91)
}

fn success_color() -> Color {
    Color::from_rgb8(0x8D, 0xA7, 0x74)
}

fn success_surface() -> Color {
    Color::from_rgb8(0x18, 0x22, 0x16)
}

fn danger_color() -> Color {
    Color::from_rgb8(0xD4, 0x72, 0x63)
}

fn danger_surface() -> Color {
    Color::from_rgb8(0x2B, 0x18, 0x15)
}

fn warning_color() -> Color {
    Color::from_rgb8(0xC7, 0x9A, 0x55)
}

fn selected_surface() -> Color {
    blend(primary_color(), surface_alt(), 0.16)
}

fn on_primary_text_color() -> Color {
    Color::from_rgb8(0x18, 0x15, 0x10)
}

fn blend(top: Color, bottom: Color, amount: f32) -> Color {
    let t = amount.clamp(0.0, 1.0);
    Color {
        r: bottom.r + (top.r - bottom.r) * t,
        g: bottom.g + (top.g - bottom.g) * t,
        b: bottom.b + (top.b - bottom.b) * t,
        a: 1.0,
    }
}

fn soft_card_style(background: Color, border: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(background.into()),
        border: iced::Border {
            radius: radius.into(),
            color: border,
            width: 1.0,
        },
        ..Default::default()
    }
}

fn panel_style(background: Color) -> container::Style {
    soft_card_style(background, border_color(), 18.0)
}

fn status_bar_style() -> container::Style {
    soft_card_style(
        blend(surface(), background_color(), 0.46),
        border_color(),
        12.0,
    )
}

#[derive(Clone, Copy)]
enum ButtonTone {
    Primary,
    Secondary,
    Danger,
}

#[derive(Clone, Copy)]
enum BadgeTone {
    Neutral,
    Success,
    Warning,
    Accent,
}

fn toolbar_button(
    label: &'static str,
    message: Option<Message>,
    tone: ButtonTone,
) -> Element<'static, Message> {
    let enabled = message.is_some();
    button(text(label).size(14))
        .padding([10, 15])
        .height(Length::Fixed(40.0))
        .on_press_maybe(message)
        .style(move |_, status| toolbar_button_style(tone, status, enabled))
        .into()
}

fn segmented_control(current: ViewMode) -> Element<'static, Message> {
    let segment = |label: &'static str, mode: ViewMode| {
        let active = current == mode;
        button(text(label).size(14))
            .padding([8, 14])
            .height(Length::Fixed(38.0))
            .on_press(Message::ViewMode(mode))
            .style(move |_, status| segmented_button_style(active, status))
    };

    container(
        row![
            segment("Список", ViewMode::List),
            segment("Плитки", ViewMode::Tiles)
        ]
        .spacing(6),
    )
    .padding(5)
    .style(|_| panel_style(surface()))
    .into()
}

fn pill(label: impl Into<String>, tone: BadgeTone) -> Element<'static, Message> {
    let label = label.into();
    let (background, border, text_col) = match tone {
        BadgeTone::Neutral => (
            blend(surface_alt(), surface(), 0.32),
            border_color(),
            muted_text_color(),
        ),
        BadgeTone::Success => (
            success_surface(),
            blend(success_color(), border_strong(), 0.36),
            success_color(),
        ),
        BadgeTone::Warning => (
            blend(warning_color(), surface(), 0.14),
            blend(warning_color(), border_strong(), 0.36),
            warning_color(),
        ),
        BadgeTone::Accent => (
            blend(accent_color(), surface_alt(), 0.14),
            blend(accent_color(), border_strong(), 0.42),
            text_color(),
        ),
    };

    container(text(label).size(12).style(move |_| text::Style {
        color: Some(text_col),
    }))
    .padding([5, 10])
    .style(move |_| container::Style {
        background: Some(background.into()),
        border: iced::Border {
            radius: 999.0.into(),
            color: border,
            width: 1.0,
        },
        ..Default::default()
    })
    .into()
}

fn modal_label(label: &'static str) -> Element<'static, Message> {
    text(label)
        .size(13)
        .style(|_| text::Style {
            color: Some(muted_text_color()),
        })
        .into()
}

fn toolbar_button_style(tone: ButtonTone, status: button::Status, enabled: bool) -> button::Style {
    let (base, hover, pressed, border, text_col) = match tone {
        ButtonTone::Primary => (
            primary_color(),
            primary_hover_color(),
            blend(primary_hover_color(), primary_color(), 0.72),
            blend(primary_color(), border_strong(), 0.62),
            on_primary_text_color(),
        ),
        ButtonTone::Secondary => (
            blend(surface_alt(), surface(), 0.18),
            surface_hover(),
            blend(background_color(), surface_alt(), 0.18),
            border_strong(),
            text_color(),
        ),
        ButtonTone::Danger => (
            danger_surface(),
            blend(danger_color(), danger_surface(), 0.18),
            blend(background_color(), danger_surface(), 0.28),
            blend(danger_color(), border_strong(), 0.45),
            text_color(),
        ),
    };

    let (background, border_color_value, text_color_value) = match status {
        button::Status::Disabled => (
            blend(surface(), background_color(), 0.38),
            blend(border_color(), background_color(), 0.28),
            blend(muted_text_color(), background_color(), 0.38),
        ),
        button::Status::Hovered => (hover, border, text_col),
        button::Status::Pressed => (pressed, border, text_col),
        button::Status::Active => (base, border, text_col),
    };

    button::Style {
        background: Some(background.into()),
        text_color: if enabled {
            text_color_value
        } else {
            blend(muted_text_color(), background_color(), 0.15)
        },
        border: iced::Border {
            radius: 12.0.into(),
            color: if enabled {
                border_color_value
            } else {
                blend(border_color_value, background_color(), 0.25)
            },
            width: 1.0,
        },
        ..Default::default()
    }
}

fn segmented_button_style(active: bool, status: button::Status) -> button::Style {
    let background = if active {
        match status {
            button::Status::Pressed => blend(primary_color(), surface_alt(), 0.2),
            button::Status::Hovered => blend(primary_color(), surface_alt(), 0.16),
            _ => blend(primary_color(), surface_alt(), 0.12),
        }
    } else {
        match status {
            button::Status::Hovered => surface_hover(),
            button::Status::Pressed => blend(background_color(), surface_alt(), 0.18),
            _ => Color::TRANSPARENT,
        }
    };

    button::Style {
        background: Some(background.into()),
        text_color: if active {
            text_color()
        } else {
            muted_text_color()
        },
        border: iced::Border {
            radius: 10.0.into(),
            color: if active {
                primary_color()
            } else {
                Color::TRANSPARENT
            },
            width: if active { 1.0 } else { 0.0 },
        },
        ..Default::default()
    }
}

fn input_style(status: text_input::Status) -> text_input::Style {
    let border = match status {
        text_input::Status::Focused => primary_color(),
        text_input::Status::Hovered => blend(primary_color(), border_color(), 0.28),
        text_input::Status::Disabled => blend(border_color(), background_color(), 0.25),
        _ => border_color(),
    };

    text_input::Style {
        background: surface_alt().into(),
        border: iced::Border {
            radius: 12.0.into(),
            color: border,
            width: 1.0,
        },
        icon: muted_text_color(),
        placeholder: muted_text_color(),
        value: text_color(),
        selection: blend(primary_color(), surface_hover(), 0.28),
    }
}
