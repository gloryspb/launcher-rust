$path = "src/app.rs"
$content = Get-Content $path -Raw
$options = [System.Text.RegularExpressions.RegexOptions]::Singleline -bor [System.Text.RegularExpressions.RegexOptions]::Multiline

$content = [regex]::Replace($content, "(?ms)^    fn drag_row_overlay\(&self, game: &Game\) -> Element<'static, Message> \{.*?(?=^    fn drag_overlay\(&self\) -> Option<Element<'static, Message>> \{)", @'
    fn drag_row_overlay(&self, game: &Game) -> Element<'static, Message> {
        let icon_el = self.icon_widget(
            &game.icon,
            self.store.config_path.parent().unwrap_or(Path::new(".")),
            32,
        );
        let icon_frame = container(icon_el)
            .width(Length::Fixed(44.0))
            .height(Length::Fixed(44.0))
            .center_x(Fill)
            .center_y(Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgba8(17, 24, 39, 0.92).into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    color: Color::from_rgb8(0x33, 0x44, 0x66),
                    width: 1.0,
                },
                ..Default::default()
            });
        let badge = container(
            text("DRAGGING").size(10).style(|_| text::Style {
                color: Some(Color::from_rgb8(0xd7, 0xe1, 0xff)),
            }),
        )
        .padding([3, 8])
        .style(|_| container::Style {
            background: Some(Color::from_rgba8(69, 99, 182, 0.95).into()),
            border: iced::Border {
                radius: 8.0.into(),
                color: Color::from_rgb8(0x67, 0x8a, 0xda),
                width: 1.0,
            },
            ..Default::default()
        });
        let name = text(game.name.clone()).size(15);
        let path = text(game.path.clone()).size(11).style(|_| text::Style {
            color: Some(Color::from_rgb8(0x8c, 0x96, 0xb0)),
        });

        container(
            row![
                icon_frame,
                column![badge, name, path].spacing(2).width(Fill),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .padding(12),
        )
        .width(Length::Fixed(
            (self.window_width - 120.0).clamp(340.0, 760.0),
        ))
        .style(|_| container::Style {
            text_color: Some(Color::WHITE),
            background: Some(Color::from_rgba8(24, 30, 45, 0.94).into()),
            border: iced::Border {
                radius: 8.0.into(),
                color: Color::from_rgb8(0x67, 0x8a, 0xda),
                width: 1.0,
            },
            ..Default::default()
        })
        .into()
    }

'@, $options)

$content = [regex]::Replace($content, "(?ms)^    fn view\(&self\) -> Element<'_, Message> \{.*?(?=^    fn view_list\(&self\) -> Element<'_, Message> \{)", @'
    fn view(&self) -> Element<'_, Message> {
        #[derive(Clone, Copy)]
        enum ActionTone {
            Primary,
            Secondary,
            Danger,
        }

        let gate_ok = is_gate_open(&self.app_dir);
        let can_launch = gate_ok && self.selection.len() == 1;
        let has_sel = !self.selection.is_empty();

        let mode_badge = container(
            text(match self.mode {
                AppMode::Debug => "Debug",
                AppMode::Release => "Release",
            })
            .size(12)
            .style(|_| text::Style {
                color: Some(Color::from_rgb8(0xc6, 0xd1, 0xf5)),
            }),
        )
        .padding([4, 10])
        .style(|_| container::Style {
            background: Some(Color::from_rgb8(0x17, 0x1d, 0x2d).into()),
            border: iced::Border {
                radius: 8.0.into(),
                color: Color::from_rgb8(0x2a, 0x35, 0x52),
                width: 1.0,
            },
            ..Default::default()
        });

        let status_badge = container(
            text(if gate_ok {
                "Задание выполнено"
            } else {
                "Сначала выполни задание"
            })
            .size(12)
            .style(move |_| text::Style {
                color: Some(if gate_ok {
                    Color::from_rgb8(0xb7, 0xf0, 0xdc)
                } else {
                    Color::from_rgb8(0xff, 0xd0, 0xd0)
                }),
            }),
        )
        .padding([4, 10])
        .style(move |_| container::Style {
            background: Some(
                if gate_ok {
                    Color::from_rgb8(0x13, 0x2a, 0x28)
                } else {
                    Color::from_rgb8(0x32, 0x1c, 0x22)
                }
                .into(),
            ),
            border: iced::Border {
                radius: 8.0.into(),
                color: if gate_ok {
                    Color::from_rgb8(0x2f, 0x5a, 0x4f)
                } else {
                    Color::from_rgb8(0x6a, 0x3a, 0x44)
                },
                width: 1.0,
            },
            ..Default::default()
        });

        let action_button = |label: &'static str, message: Option<Message>, tone: ActionTone| {
            let enabled = message.is_some();
            let (base, hover, pressed, border) = match tone {
                ActionTone::Primary => (
                    Color::from_rgb8(0x35, 0x5f, 0xdc),
                    Color::from_rgb8(0x3f, 0x6b, 0xeb),
                    Color::from_rgb8(0x2d, 0x52, 0xc0),
                    Color::from_rgb8(0x67, 0x8a, 0xda),
                ),
                ActionTone::Secondary => (
                    Color::from_rgb8(0x1a, 0x24, 0x38),
                    Color::from_rgb8(0x22, 0x2e, 0x45),
                    Color::from_rgb8(0x16, 0x1f, 0x32),
                    Color::from_rgb8(0x33, 0x40, 0x60),
                ),
                ActionTone::Danger => (
                    Color::from_rgb8(0x3a, 0x20, 0x28),
                    Color::from_rgb8(0x4b, 0x27, 0x31),
                    Color::from_rgb8(0x2f, 0x1a, 0x21),
                    Color::from_rgb8(0x63, 0x34, 0x40),
                ),
            };

            button(text(label).size(14))
                .padding([10, 16])
                .on_press_maybe(message)
                .style(move |_theme, status| {
                    let (background, text_color, border_color) = match status {
                        button::Status::Disabled => (
                            Color::from_rgb8(0x14, 0x1b, 0x2a),
                            Color::from_rgb8(0x74, 0x7d, 0x93),
                            Color::from_rgb8(0x24, 0x2b, 0x3f),
                        ),
                        button::Status::Hovered => (hover, Color::WHITE, border),
                        button::Status::Pressed => (pressed, Color::WHITE, border),
                        button::Status::Active => (base, Color::WHITE, border),
                    };

                    button::Style {
                        background: Some(background.into()),
                        text_color,
                        border: iced::Border {
                            radius: 8.0.into(),
                            color: if enabled { border_color } else { Color::from_rgb8(0x24, 0x2b, 0x3f) },
                            width: 1.0,
                        },
                        ..Default::default()
                    }
                })
        };

        let view_toggle = |label: &'static str, mode: ViewMode| {
            radio(label, mode, Some(self.view_mode), Message::ViewMode)
                .text_size(14)
                .size(16)
                .spacing(8)
                .style(|_theme, status| match status {
                    radio::Status::Active { is_selected }
                    | radio::Status::Hovered { is_selected } => radio::Style {
                        background: if is_selected {
                            Color::from_rgb8(0x1b, 0x28, 0x44).into()
                        } else {
                            Color::from_rgb8(0x11, 0x16, 0x25).into()
                        },
                        dot_color: if is_selected {
                            Color::from_rgb8(0x8d, 0xb0, 0xff)
                        } else {
                            Color::from_rgb8(0x42, 0x4d, 0x69)
                        },
                        border_width: 1.0,
                        border_color: if is_selected {
                            Color::from_rgb8(0x67, 0x8a, 0xda)
                        } else {
                            Color::from_rgb8(0x37, 0x41, 0x5d)
                        },
                        text_color: Some(Color::from_rgb8(0xdb, 0xe2, 0xf6)),
                    },
                })
        };

        let toolbar = row![
            action_button("Запустить", can_launch.then_some(Message::Launch), ActionTone::Primary),
            action_button("Добавить", Some(Message::AddPressed), ActionTone::Secondary),
            action_button("Несколько", Some(Message::AddPickMany), ActionTone::Secondary),
            action_button("Изменить", Some(Message::EditPressed), ActionTone::Secondary),
            action_button("Удалить", has_sel.then_some(Message::RemovePressed), ActionTone::Danger),
            horizontal_space(),
            container(
                row![
                    view_toggle("Список", ViewMode::List),
                    view_toggle("Плитки", ViewMode::Tiles)
                ]
                .spacing(14)
            )
            .padding([8, 12])
            .style(|_| container::Style {
                background: Some(Color::from_rgb8(0x12, 0x18, 0x28).into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    color: Color::from_rgb8(0x2b, 0x34, 0x4a),
                    width: 1.0,
                },
                ..Default::default()
            }),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let body: Element<'_, Message> = match self.view_mode {
            ViewMode::List => self.view_list(),
            ViewMode::Tiles => self.view_tiles(),
        };

        let body_container = container(body)
            .height(Fill)
            .padding(8)
            .style(|_| container::Style {
                background: Some(Color::from_rgb8(0x0f, 0x14, 0x22).into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    color: Color::from_rgb8(0x2b, 0x34, 0x4a),
                    width: 1.0,
                },
                ..Default::default()
            });

        let header = column![
            text("Game Launcher").size(28),
            row![mode_badge, status_badge]
                .spacing(10)
                .align_y(Alignment::Center),
        ]
        .spacing(10);

        let main = container(
            column![
                header,
                toolbar,
                body_container,
                text(format!("Данные: {}", self.store.config_path.display()))
                    .size(11)
                    .style(|_| text::Style {
                        color: Some(Color::from_rgb8(0x76, 0x81, 0x98)),
                    }),
            ]
            .spacing(12),
        )
        .width(Fill)
        .height(Fill)
        .padding(14)
        .style(|_| container::Style {
            background: Some(Color::from_rgb8(0x0b, 0x10, 0x1a).into()),
            ..Default::default()
        });

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
                                button("...").on_press(Message::AddPickExe),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            text("Иконка (необязательно):"),
                            row![
                                text_input("Путь к иконке", icon)
                                    .on_input(Message::AddIcon)
                                    .padding(6)
                                    .width(Fill),
                                button("...").on_press(Message::AddPickIcon),
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
                                    button("...").on_press(Message::EditPickIcon),
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

        let root: Element<'_, Message> = container(under).width(Fill).height(Fill).into();

        if let Some(drag_overlay) = self.drag_overlay() {
            stack![root, drag_overlay].into()
        } else {
            root
        }
    }

'@, $options)

$content = [regex]::Replace($content, "(?ms)^    fn view_list\(&self\) -> Element<'_, Message> \{.*?(?=^    fn game_row_list<'a>\(&'a self, g: &'a Game\) -> Element<'a, Message> \{)", @'
    fn view_list(&self) -> Element<'_, Message> {
        if self.store.games.is_empty() {
            container(
                text("Нет игр. Добавь игры кнопкой выше или перетащи сюда .lnk / .exe")
                    .size(14)
                    .style(|_| text::Style {
                        color: Some(Color::from_rgb8(0x92, 0x9b, 0xb1)),
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

            scrollable(column(list_column).spacing(10).padding(6).width(Fill))
                .height(Fill)
                .into()
        }
    }

'@, $options)

$content = [regex]::Replace($content, "(?ms)^    fn game_row_list<'a>\(&'a self, g: &'a Game\) -> Element<'a, Message> \{.*?(?=^    fn view_tiles\(&self\) -> Element<'_, Message> \{)", @'
    fn game_row_list<'a>(&'a self, g: &'a Game) -> Element<'a, Message> {
        let selected = self.selection.contains(&g.id);
        let drag_source = self.is_drag_source(&g.id);
        let drag_target = self.is_drag_target(&g.id);
        let preview_game = self.drag_preview_game(&g.id);
        let display_game = preview_game.unwrap_or(g);
        let is_preview = preview_game.is_some();

        let icon_el: Element<'static, Message> = self.icon_widget(
            &display_game.icon,
            self.store.config_path.parent().unwrap_or(Path::new(".")),
            30,
        );
        let icon_frame = container(icon_el)
            .width(Length::Fixed(46.0))
            .height(Length::Fixed(46.0))
            .center_x(Fill)
            .center_y(Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgb8(0x10, 0x16, 0x26).into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    color: Color::from_rgb8(0x2d, 0x3a, 0x58),
                    width: 1.0,
                },
                ..Default::default()
            });
        let name_el = text(&display_game.name).size(15);
        let path_el = text(&display_game.path).size(11).style(|_| text::Style {
            color: Some(Color::from_rgb8(0x8c, 0x96, 0xb0)),
        });
        let preview_badge: Option<Element<'_, Message>> = if is_preview {
            Some(
                container(text("PREVIEW").size(10).style(|_| text::Style {
                    color: Some(Color::from_rgb8(0xd7, 0xe1, 0xff)),
                }))
                .padding([2, 8])
                .style(|_| container::Style {
                    background: Some(Color::from_rgba8(69, 99, 182, 0.95).into()),
                    border: iced::Border {
                        radius: 8.0.into(),
                        color: Color::from_rgb8(0x67, 0x8a, 0xda),
                        width: 1.0,
                    },
                    ..Default::default()
                })
                .into(),
            )
        } else {
            None
        };
        let meta_col = if let Some(preview_badge) = preview_badge {
            column![preview_badge, name_el, path_el]
        } else {
            column![name_el, path_el]
        }
        .spacing(3)
        .width(Fill);

        let row_content = row![icon_frame, meta_col]
            .spacing(14)
            .align_y(Alignment::Center)
            .padding([12, 14]);

        let bg = if drag_target {
            Color::from_rgb8(0x16, 0x23, 0x39)
        } else if drag_source {
            Color::from_rgb8(0x10, 0x15, 0x22)
        } else if selected {
            Color::from_rgb8(0x1b, 0x26, 0x3c)
        } else {
            Color::from_rgb8(0x13, 0x19, 0x28)
        };
        let border_color = if drag_target {
            Color::from_rgb8(0x67, 0x8a, 0xda)
        } else if drag_source {
            Color::from_rgb8(0x2b, 0x35, 0x50)
        } else if selected {
            Color::from_rgb8(0x4c, 0x66, 0x99)
        } else {
            Color::from_rgb8(0x27, 0x31, 0x48)
        };
        let id = g.id.clone();

        mouse_area(
            container(row_content)
                .width(Fill)
                .height(Length::Fixed(76.0))
                .style(move |_| container::Style {
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

'@, $options)

$content = [regex]::Replace($content, "(?ms)^    fn view_tiles\(&self\) -> Element<'_, Message> \{.*?(?=^    fn game_tile<'a>\(&'a self, g: &'a Game, width: f32\) -> Element<'a, Message> \{)", @'
    fn view_tiles(&self) -> Element<'_, Message> {
        let games = &self.store.games;
        if games.is_empty() {
            container(
                text("Нет игр. Добавь игры кнопкой выше или перетащи сюда .lnk / .exe")
                    .size(14)
                    .style(|_| text::Style {
                        color: Some(Color::from_rgb8(0x92, 0x9b, 0xb1)),
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
            let tile_width: f32 = 112.0;
            let spacing: f32 = 12.0;
            let available_width = (self.window_width - 48.0).max(tile_width);
            let cols =
                (((available_width + spacing) / (tile_width + spacing)).floor() as usize).max(1);

            let row_count = total.div_ceil(cols);
            let mut rows_vec: Vec<Element<'_, Message>> = Vec::with_capacity(row_count);

            for row_idx in 0..row_count {
                let tiles: Vec<Element<'_, Message>> = games
                    .iter()
                    .skip(row_idx * cols)
                    .take(cols)
                    .map(|g| self.game_tile(g, tile_width))
                    .collect();
                rows_vec.push(row(tiles).spacing(spacing).into());
            }

            let content = column(rows_vec).spacing(spacing).padding(6);

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

'@, $options)

$content = [regex]::Replace($content, "(?ms)^    fn game_tile<'a>\(&'a self, g: &'a Game, width: f32\) -> Element<'a, Message> \{.*?(?=^}\s*$\s*^fn truncate_tile_name)", @'
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
            60,
        );
        let icon_frame = container(icon_el)
            .width(Length::Fixed(72.0))
            .height(Length::Fixed(72.0))
            .center_x(Fill)
            .center_y(Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgb8(0x10, 0x16, 0x26).into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    color: Color::from_rgb8(0x2d, 0x3a, 0x58),
                    width: 1.0,
                },
                ..Default::default()
            });

        let preview_badge = container(
            text(if is_preview { "PREVIEW" } else { " " })
                .size(10)
                .style(move |_| text::Style {
                    color: Some(if is_preview {
                        Color::from_rgb8(0xd7, 0xe1, 0xff)
                    } else {
                        Color::TRANSPARENT
                    }),
                }),
        )
        .width(Fill)
        .height(Length::Fixed(18.0))
        .center_x(Fill)
        .style(move |_| container::Style {
            background: Some(
                if is_preview {
                    Color::from_rgba8(69, 99, 182, 0.95)
                } else {
                    Color::TRANSPARENT
                }
                .into(),
            ),
            border: iced::Border {
                radius: 8.0.into(),
                color: if is_preview {
                    Color::from_rgb8(0x67, 0x8a, 0xda)
                } else {
                    Color::TRANSPARENT
                },
                width: if is_preview { 1.0 } else { 0.0 },
            },
            ..Default::default()
        });

        let title = container(text(truncate_tile_name(&display_game.name)).size(12).align_x(Center))
            .width(Fill)
            .height(Length::Fixed(36.0))
            .center_x(Fill)
            .center_y(Fill);

        let inner = column![preview_badge, icon_frame, title]
            .spacing(8)
            .align_x(Center)
            .width(Fill);

        let bg = if drag_target {
            Color::from_rgb8(0x16, 0x23, 0x39)
        } else if drag_source {
            Color::from_rgb8(0x10, 0x15, 0x22)
        } else if selected {
            Color::from_rgb8(0x1b, 0x26, 0x3c)
        } else {
            Color::from_rgb8(0x13, 0x19, 0x28)
        };
        let border_color = if drag_target {
            Color::from_rgb8(0x67, 0x8a, 0xda)
        } else if drag_source {
            Color::from_rgb8(0x2b, 0x35, 0x50)
        } else if selected {
            Color::from_rgb8(0x4c, 0x66, 0x99)
        } else {
            Color::from_rgb8(0x27, 0x31, 0x48)
        };
        let id = g.id.clone();

        mouse_area(
            container(inner)
                .padding([10, 10])
                .width(Length::Fixed(width))
                .height(Length::Fixed(152.0))
                .style(move |_| container::Style {
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

'@, $options)

Set-Content -Path $path -Value $content -Encoding UTF8
