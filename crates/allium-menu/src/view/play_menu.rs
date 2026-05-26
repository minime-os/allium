use std::collections::VecDeque;

use anyhow::Result;
use async_trait::async_trait;
use common::display::Display;
use common::geom::{Alignment, Point, Rect};
use common::locale::Locale;
use common::platform::{DefaultPlatform, Key, KeyEvent, Platform};
use common::resources::Resources;
use common::retroarch::RetroArchCommand;
use common::stylesheet::Stylesheet;
use common::view::{ButtonHint, ButtonHints, Label, SettingsList, View};
use tokio::sync::mpsc::Sender;

/// Play settings overlay rendered inside the Allium in-game menu.
/// Structured like MinArch with a category selection list.
pub struct PlayMenu {
    rect: Rect,
    category_list: SettingsList,
    button_hints: ButtonHints<String>,
    child: Option<PlayMenuSub>,
    res: Resources,
}

enum PlayMenuSub {
    Frontend(FrontendMenu),
    Placeholder(PlaceholderMenu),
    SaveChanges(SaveChangesMenu),
}

impl PlayMenu {
    pub fn new(rect: Rect, res: Resources) -> Self {
        let locale = res.get::<Locale>();
        let styles = res.get::<Stylesheet>();

        let mut button_hints = ButtonHints::new(
            res.clone(),
            vec![ButtonHint::new(
                res.clone(),
                Point::zero(),
                Key::B,
                locale.t("button-back"),
                Alignment::Left,
            )],
            vec![ButtonHint::new(
                res.clone(),
                Point::zero(),
                Key::A,
                locale.t("button-select"),
                Alignment::Right,
            )],
        );

        let button_hints_rect = button_hints.bounding_box(&styles);
        let content_top =
            rect.y + styles.ui.margin_y + styles.ui.ui_font.size as i32 + styles.ui.margin_y / 2;
        let content_height = (button_hints_rect.y - content_top) as u32;

        let entry_labels = vec![
            "Frontend".to_string(),
            "Controls".to_string(),
            "Shortcuts".to_string(),
            "Core Options".to_string(),
            "Save Changes".to_string(),
        ];

        let right_views: Vec<Box<dyn View>> = entry_labels
            .iter()
            .map(|_| {
                Box::new(Label::new(
                    Point::zero(),
                    "".to_string(),
                    Alignment::Right,
                    None,
                )) as Box<dyn View>
            })
            .collect();

        let category_list = SettingsList::new(
            res.clone(),
            Rect::new(
                rect.x + styles.ui.margin_x,
                content_top,
                rect.w - styles.ui.margin_x as u32 * 2,
                content_height,
            ),
            entry_labels,
            right_views,
            styles.ui.ui_font.size + styles.ui.padding_y as u32,
        );

        drop(locale);
        drop(styles);

        Self {
            rect,
            category_list,
            button_hints,
            child: None,
            res,
        }
    }
}

#[async_trait(?Send)]
impl View for PlayMenu {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        if let Some(child) = &mut self.child {
            match child {
                PlayMenuSub::Frontend(m) => return m.draw(display, styles),
                PlayMenuSub::Placeholder(m) => return m.draw(display, styles),
                PlayMenuSub::SaveChanges(m) => return m.draw(display, styles),
            }
        }

        let mut drawn = false;
        drawn |= self.category_list.draw(display, styles)?;
        if self.button_hints.should_draw() {
            display.load(self.button_hints.bounding_box(styles))?;
            drawn |= self.button_hints.draw(display, styles)?;
        }
        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        if let Some(child) = &self.child {
            match child {
                PlayMenuSub::Frontend(m) => return m.should_draw(),
                PlayMenuSub::Placeholder(m) => return m.should_draw(),
                PlayMenuSub::SaveChanges(m) => return m.should_draw(),
            }
        }
        self.category_list.should_draw() || self.button_hints.should_draw()
    }

    fn set_should_draw(&mut self) {
        if let Some(child) = &mut self.child {
            match child {
                PlayMenuSub::Frontend(m) => m.set_should_draw(),
                PlayMenuSub::Placeholder(m) => m.set_should_draw(),
                PlayMenuSub::SaveChanges(m) => m.set_should_draw(),
            }
            return;
        }
        self.category_list.set_should_draw();
        self.button_hints.set_should_draw();
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        commands: Sender<common::command::Command>,
        bubble: &mut VecDeque<common::command::Command>,
    ) -> Result<bool> {
        if let Some(child) = &mut self.child {
            let handled = match child {
                PlayMenuSub::Frontend(m) => {
                    m.handle_key_event(event, commands.clone(), bubble).await?
                }
                PlayMenuSub::Placeholder(m) => {
                    m.handle_key_event(event, commands.clone(), bubble).await?
                }
                PlayMenuSub::SaveChanges(m) => {
                    m.handle_key_event(event, commands.clone(), bubble).await?
                }
            };
            if handled {
                let mut close = false;
                bubble.retain(|cmd| match cmd {
                    common::command::Command::CloseView => {
                        close = true;
                        false
                    }
                    _ => true,
                });
                if close {
                    self.child = None;
                    self.set_should_draw();
                    commands.send(common::command::Command::Redraw).await.ok();
                }
                return Ok(true);
            }
            return Ok(false);
        }

        let selected = self.category_list.selected();

        match event {
            KeyEvent::Pressed(Key::B) => {
                bubble.push_back(common::command::Command::CloseView);
                return Ok(true);
            }
            KeyEvent::Pressed(Key::A) => match selected {
                0 => {
                    self.child = Some(PlayMenuSub::Frontend(FrontendMenu::new(
                        self.rect,
                        self.res.clone(),
                    )));
                    self.set_should_draw();
                    commands.send(common::command::Command::Redraw).await.ok();
                    return Ok(true);
                }
                1 => {
                    self.child = Some(PlayMenuSub::Placeholder(PlaceholderMenu::new(
                        self.rect,
                        self.res.clone(),
                        "Controls",
                    )));
                    self.set_should_draw();
                    commands.send(common::command::Command::Redraw).await.ok();
                    return Ok(true);
                }
                2 => {
                    self.child = Some(PlayMenuSub::Placeholder(PlaceholderMenu::new(
                        self.rect,
                        self.res.clone(),
                        "Shortcuts",
                    )));
                    self.set_should_draw();
                    commands.send(common::command::Command::Redraw).await.ok();
                    return Ok(true);
                }
                3 => {
                    self.child = Some(PlayMenuSub::Placeholder(PlaceholderMenu::new(
                        self.rect,
                        self.res.clone(),
                        "Core Options",
                    )));
                    self.set_should_draw();
                    commands.send(common::command::Command::Redraw).await.ok();
                    return Ok(true);
                }
                4 => {
                    self.child = Some(PlayMenuSub::SaveChanges(SaveChangesMenu::new(
                        self.rect,
                        self.res.clone(),
                    )));
                    self.set_should_draw();
                    commands.send(common::command::Command::Redraw).await.ok();
                    return Ok(true);
                }
                _ => {}
            },
            _ => {}
        }

        self.category_list
            .handle_key_event(event, commands, bubble)
            .await
    }

    fn children(&self) -> Vec<&dyn View> {
        if let Some(child) = &self.child {
            match child {
                PlayMenuSub::Frontend(m) => return m.children(),
                PlayMenuSub::Placeholder(m) => return m.children(),
                PlayMenuSub::SaveChanges(m) => return m.children(),
            }
        }
        vec![&self.category_list, &self.button_hints]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        if let Some(child) = &mut self.child {
            match child {
                PlayMenuSub::Frontend(m) => return m.children_mut(),
                PlayMenuSub::Placeholder(m) => return m.children_mut(),
                PlayMenuSub::SaveChanges(m) => return m.children_mut(),
            }
        }
        vec![&mut self.category_list, &mut self.button_hints]
    }

    fn bounding_box(&mut self, _styles: &Stylesheet) -> Rect {
        self.rect
    }

    fn set_position(&mut self, point: Point) {
        self.rect.x = point.x;
        self.rect.y = point.y;
    }
}

// ---- Frontend Settings sub-menu ----

struct FrontendMenu {
    rect: Rect,
    settings_list: SettingsList,
    button_hints: ButtonHints<String>,
    values: Vec<String>,
}

impl FrontendMenu {
    pub fn new(rect: Rect, res: Resources) -> Self {
        let locale = res.get::<Locale>();
        let styles = res.get::<Stylesheet>();

        let mut button_hints = ButtonHints::new(
            res.clone(),
            vec![ButtonHint::new(
                res.clone(),
                Point::zero(),
                Key::B,
                locale.t("button-back"),
                Alignment::Left,
            )],
            vec![
                ButtonHint::new(
                    res.clone(),
                    Point::zero(),
                    Key::A,
                    locale.t("button-select"),
                    Alignment::Right,
                ),
                ButtonHint::new(
                    res.clone(),
                    Point::zero(),
                    Key::Left,
                    locale.t("button-left-right"),
                    Alignment::Right,
                ),
            ],
        );

        let button_hints_rect = button_hints.bounding_box(&styles);
        let content_top =
            rect.y + styles.ui.margin_y + styles.ui.ui_font.size as i32 + styles.ui.margin_y / 2;
        let content_height = (button_hints_rect.y - content_top) as u32;

        let entry_labels = vec![
            "Screen Scaling".to_string(),
            "Screen Effect".to_string(),
            "Screen Sharpness".to_string(),
            "Prevent Tearing".to_string(),
            "CPU Speed".to_string(),
            "Thread Video".to_string(),
            "Debug HUD".to_string(),
            "Max FF Speed".to_string(),
        ];

        let initial_values = vec![
            "Aspect".to_string(),
            "None".to_string(),
            "Soft".to_string(),
            "Lenient".to_string(),
            "Normal".to_string(),
            "Off".to_string(),
            "Off".to_string(),
            "4x".to_string(),
        ];

        let right_views: Vec<Box<dyn View>> = initial_values
            .iter()
            .map(|v| {
                Box::new(Label::new(Point::zero(), v.clone(), Alignment::Right, None))
                    as Box<dyn View>
            })
            .collect();

        let settings_list = SettingsList::new(
            res.clone(),
            Rect::new(
                rect.x + styles.ui.margin_x,
                content_top,
                rect.w - styles.ui.margin_x as u32 * 2,
                content_height,
            ),
            entry_labels,
            right_views,
            styles.ui.ui_font.size + styles.ui.padding_y as u32,
        );

        drop(locale);
        drop(styles);

        Self {
            rect,
            settings_list,
            button_hints,
            values: initial_values,
        }
    }

    fn cycle_value(&mut self, index: usize, forward: bool) {
        let options: Vec<&str> = match index {
            0 => vec!["Native", "Aspect", "Cropped", "Fullscreen"],
            1 => vec!["None", "Grid", "Line"],
            2 => vec!["Sharp", "Crisp", "Soft"],
            3 => vec!["Off", "Lenient", "Strict"],
            4 => vec!["Powersave", "Normal", "Performance"],
            5 => vec!["Off", "On"],
            6 => vec!["Off", "On"],
            7 => vec!["1x", "2x", "3x", "4x", "5x", "6x", "7x", "8x"],
            _ => return,
        };

        let current = &self.values[index];
        let pos = options
            .iter()
            .position(|&v| v.eq_ignore_ascii_case(current))
            .unwrap_or(0);
        let next = if forward {
            (pos + 1) % options.len()
        } else {
            pos.checked_sub(1).unwrap_or(options.len() - 1)
        };
        let new_value = options[next].to_string();

        self.values[index] = new_value.clone();
        self.settings_list.set_right(
            index,
            Box::new(Label::new(
                Point::zero(),
                new_value.clone(),
                Alignment::Right,
                None,
            )),
        );
    }

    async fn send_udp_command(index: usize, value: &str) -> Result<()> {
        match index {
            0 => {
                RetroArchCommand::SetScale(value.to_lowercase())
                    .send()
                    .await?
            }
            1 => {
                RetroArchCommand::SetEffect(value.to_lowercase())
                    .send()
                    .await?
            }
            2 => {
                RetroArchCommand::SetSharpness(value.to_lowercase())
                    .send()
                    .await?
            }
            3 => {
                RetroArchCommand::SetTearing(value.to_lowercase())
                    .send()
                    .await?
            }
            4 => {
                RetroArchCommand::SetOverclock(value.to_lowercase())
                    .send()
                    .await?
            }
            5 => {
                RetroArchCommand::SetThreadVideo(value.eq_ignore_ascii_case("On"))
                    .send()
                    .await?
            }
            6 => {
                RetroArchCommand::SetDebugHUD(value.eq_ignore_ascii_case("On"))
                    .send()
                    .await?
            }
            7 => {
                let speed = value
                    .trim_end_matches('x')
                    .parse::<u8>()
                    .unwrap_or(1)
                    .clamp(1, 8);
                RetroArchCommand::SetMaxFF(speed).send().await?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl View for FrontendMenu {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        let mut drawn = false;
        drawn |= self.settings_list.draw(display, styles)?;
        if self.button_hints.should_draw() {
            display.load(self.button_hints.bounding_box(styles))?;
            drawn |= self.button_hints.draw(display, styles)?;
        }
        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        self.settings_list.should_draw() || self.button_hints.should_draw()
    }

    fn set_should_draw(&mut self) {
        self.settings_list.set_should_draw();
        self.button_hints.set_should_draw();
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        commands: Sender<common::command::Command>,
        bubble: &mut VecDeque<common::command::Command>,
    ) -> Result<bool> {
        let selected = self.settings_list.selected();

        match event {
            KeyEvent::Pressed(Key::B) => {
                bubble.push_back(common::command::Command::CloseView);
                return Ok(true);
            }
            KeyEvent::Pressed(Key::Left) | KeyEvent::Autorepeat(Key::Left) if selected < 8 => {
                self.cycle_value(selected, false);
                Self::send_udp_command(selected, &self.values[selected].clone()).await?;
                return Ok(true);
            }
            KeyEvent::Pressed(Key::Right) | KeyEvent::Autorepeat(Key::Right) if selected < 8 => {
                self.cycle_value(selected, true);
                Self::send_udp_command(selected, &self.values[selected].clone()).await?;
                return Ok(true);
            }
            _ => {}
        }

        self.settings_list
            .handle_key_event(event, commands, bubble)
            .await
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&self.settings_list, &self.button_hints]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut self.settings_list, &mut self.button_hints]
    }

    fn bounding_box(&mut self, _styles: &Stylesheet) -> Rect {
        self.rect
    }

    fn set_position(&mut self, point: Point) {
        self.rect.x = point.x;
        self.rect.y = point.y;
    }
}

// ---- Placeholder sub-menu for deferred options ----

struct PlaceholderMenu {
    rect: Rect,
    msg: Label<String>,
    button_hints: ButtonHints<String>,
}

impl PlaceholderMenu {
    pub fn new(rect: Rect, res: Resources, _sub_name: &str) -> Self {
        let locale = res.get::<Locale>();
        let styles = res.get::<Stylesheet>();

        let msg = Label::new(
            Point::new(rect.x + rect.w as i32 / 2, rect.y + rect.h as i32 / 2),
            "Option deferred to next iteration".to_string(),
            Alignment::Center,
            None,
        );

        let button_hints = ButtonHints::new(
            res.clone(),
            vec![ButtonHint::new(
                res.clone(),
                Point::zero(),
                Key::B,
                locale.t("button-back"),
                Alignment::Left,
            )],
            vec![],
        );

        drop(locale);
        drop(styles);

        Self {
            rect,
            msg,
            button_hints,
        }
    }
}

#[async_trait(?Send)]
impl View for PlaceholderMenu {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        let mut drawn = false;
        drawn |= self.msg.draw(display, styles)?;
        if self.button_hints.should_draw() {
            display.load(self.button_hints.bounding_box(styles))?;
            drawn |= self.button_hints.draw(display, styles)?;
        }
        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        self.msg.should_draw() || self.button_hints.should_draw()
    }

    fn set_should_draw(&mut self) {
        self.msg.set_should_draw();
        self.button_hints.set_should_draw();
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        _commands: Sender<common::command::Command>,
        bubble: &mut VecDeque<common::command::Command>,
    ) -> Result<bool> {
        if event == KeyEvent::Pressed(Key::B) {
            bubble.push_back(common::command::Command::CloseView);
            return Ok(true);
        }
        Ok(false)
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&self.msg, &self.button_hints]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut self.msg, &mut self.button_hints]
    }

    fn bounding_box(&mut self, _styles: &Stylesheet) -> Rect {
        self.rect
    }

    fn set_position(&mut self, point: Point) {
        self.rect.x = point.x;
        self.rect.y = point.y;
    }
}

// ---- Save Changes sub-menu ----

struct SaveChangesMenu {
    rect: Rect,
    settings_list: SettingsList,
    button_hints: ButtonHints<String>,
}

impl SaveChangesMenu {
    pub fn new(rect: Rect, res: Resources) -> Self {
        let locale = res.get::<Locale>();
        let styles = res.get::<Stylesheet>();
        let mut button_hints = ButtonHints::new(
            res.clone(),
            vec![ButtonHint::new(
                res.clone(),
                Point::zero(),
                Key::B,
                locale.t("button-back"),
                Alignment::Left,
            )],
            vec![ButtonHint::new(
                res.clone(),
                Point::zero(),
                Key::A,
                locale.t("button-select"),
                Alignment::Right,
            )],
        );
        let content_top =
            rect.y + styles.ui.margin_y + styles.ui.ui_font.size as i32 + styles.ui.margin_y / 2;
        let content_height = (button_hints.bounding_box(&styles).y - content_top) as u32;
        let entry_labels = vec![
            "Save for console".to_string(),
            "Save for game".to_string(),
            "Restore defaults".to_string(),
        ];
        let right_views = (0..3)
            .map(|_| {
                Box::new(Label::new(
                    Point::zero(),
                    "".to_string(),
                    Alignment::Right,
                    None,
                )) as Box<dyn View>
            })
            .collect();
        let settings_list = SettingsList::new(
            res.clone(),
            Rect::new(
                rect.x + styles.ui.margin_x,
                content_top,
                rect.w - styles.ui.margin_x as u32 * 2,
                content_height,
            ),
            entry_labels,
            right_views,
            styles.ui.ui_font.size + styles.ui.padding_y as u32,
        );
        drop(locale);
        drop(styles);
        Self {
            rect,
            settings_list,
            button_hints,
        }
    }
}

#[async_trait(?Send)]
impl View for SaveChangesMenu {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        let mut drawn = false;
        drawn |= self.settings_list.draw(display, styles)?;
        if self.button_hints.should_draw() {
            display.load(self.button_hints.bounding_box(styles))?;
            drawn |= self.button_hints.draw(display, styles)?;
        }
        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        self.settings_list.should_draw() || self.button_hints.should_draw()
    }

    fn set_should_draw(&mut self) {
        self.settings_list.set_should_draw();
        self.button_hints.set_should_draw();
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        commands: Sender<common::command::Command>,
        bubble: &mut VecDeque<common::command::Command>,
    ) -> Result<bool> {
        let selected = self.settings_list.selected();
        match event {
            KeyEvent::Pressed(Key::B) => {
                bubble.push_back(common::command::Command::CloseView);
                Ok(true)
            }
            KeyEvent::Pressed(Key::A) => {
                match selected {
                    0 => RetroArchCommand::SaveConfigConsole.send().await?,
                    1 => RetroArchCommand::SaveConfigGame.send().await?,
                    2 => RetroArchCommand::RestoreDefaults.send().await?,
                    _ => {}
                }
                bubble.push_back(common::command::Command::CloseView);
                Ok(true)
            }
            _ => {
                self.settings_list
                    .handle_key_event(event, commands, bubble)
                    .await
            }
        }
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&self.settings_list, &self.button_hints]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut self.settings_list, &mut self.button_hints]
    }

    fn bounding_box(&mut self, _styles: &Stylesheet) -> Rect {
        self.rect
    }

    fn set_position(&mut self, point: Point) {
        self.rect.x = point.x;
        self.rect.y = point.y;
    }
}
