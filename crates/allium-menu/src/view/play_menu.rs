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
use common::view::{
    ButtonHint, ButtonHints, Label, SettingsList, View,
};
use tokio::sync::mpsc::Sender;

/// Play settings overlay rendered inside the Allium in-game menu.
/// Currently shows the Frontend settings tab only (Controls, Shortcuts, and
/// Core tabs are deferred to follow-up iterations).
///
/// Controls:
///   Up/Down     – navigate settings
///   Left/Right  – cycle the selected setting’s value
///   A           – activate save/restore actions
///   B           – close PlayMenu and return to IngameMenu
pub struct PlayMenu {
    rect: Rect,
    game_name: Label<String>,
    settings_list: SettingsList,
    button_hints: ButtonHints<String>,
    values: Vec<String>,
}

impl PlayMenu {
    pub fn new(rect: Rect, res: Resources) -> Self {
        let game_info = res.get::<common::game_info::GameInfo>();
        let locale = res.get::<Locale>();
        let styles = res.get::<Stylesheet>();

        let name = Label::new(
            Point::new(rect.x + styles.ui.margin_x, rect.y + styles.ui.margin_y),
            game_info.name.clone(),
            Alignment::Left,
            None,
        );
        drop(game_info);

        let mut button_hints = ButtonHints::new(
            res.clone(),
            vec![ButtonHint::new(
                res.clone(),
                Point::zero(),
                Key::B,
                locale.t("ingame-menu-continue"),
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
        let content_top = rect.y
            + styles.ui.margin_y
            + styles.ui.ui_font.size as i32
            + styles.ui.margin_y / 2;
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
            "Save as Default".to_string(),
            "Restore Defaults".to_string(),
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
            "".to_string(),
            "".to_string(),
        ];

        let right_views: Vec<Box<dyn View>> = initial_values
            .iter()
            .map(|v| {
                Box::new(Label::new(
                    Point::zero(),
                    v.clone(),
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
            game_name: name,
            settings_list,
            button_hints,
            values: initial_values,
        }
    }

    pub fn set_value(&mut self, index: usize, value: &str) {
        if index < self.values.len() {
            self.values[index] = value.to_string();
            self.settings_list
                .set_right(index, Box::new(Label::new(Point::zero(), value.to_string(), Alignment::Right, None)));
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
            Box::new(Label::new(Point::zero(), new_value.clone(), Alignment::Right, None)),
        );
    }

    async fn send_udp_command(index: usize, value: &str) -> Result<()> {
        match index {
            0 => RetroArchCommand::SetScale(value.to_lowercase()).send().await?,
            1 => RetroArchCommand::SetEffect(value.to_lowercase()).send().await?,
            2 => RetroArchCommand::SetSharpness(value.to_lowercase()).send().await?,
            3 => RetroArchCommand::SetTearing(value.to_lowercase()).send().await?,
            4 => RetroArchCommand::SetOverclock(value.to_lowercase()).send().await?,
            5 => RetroArchCommand::SetThreadVideo(value.eq_ignore_ascii_case("On")).send().await?,
            6 => RetroArchCommand::SetDebugHUD(value.eq_ignore_ascii_case("On")).send().await?,
            7 => {
                let speed = value
                    .trim_end_matches('x')
                    .parse::<u8>()
                    .unwrap_or(1)
                    .min(8)
                    .max(1);
                RetroArchCommand::SetMaxFF(speed).send().await?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl View for PlayMenu {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        let mut drawn = false;
        drawn |= self.game_name.draw(display, styles)?;
        drawn |= self.settings_list.draw(display, styles)?;
        if self.button_hints.should_draw() {
            display.load(self.button_hints.bounding_box(styles))?;
            drawn |= self.button_hints.draw(display, styles)?;
        }
        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        self.game_name.should_draw()
            || self.settings_list.should_draw()
            || self.button_hints.should_draw()
    }

    fn set_should_draw(&mut self) {
        self.game_name.set_should_draw();
        self.settings_list.set_should_draw();
        self.button_hints.set_should_draw();
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        _commands: Sender<common::command::Command>,
        bubble: &mut VecDeque<common::command::Command>,
    ) -> Result<bool> {
        let selected = self.settings_list.selected();

        match event {
            KeyEvent::Pressed(Key::B) => {
                bubble.push_back(common::command::Command::CloseView);
                return Ok(true);
            }
            KeyEvent::Pressed(Key::Left) | KeyEvent::Autorepeat(Key::Left)
                if selected < 8 =>
            {
                self.cycle_value(selected, false);
                Self::send_udp_command(selected, &self.values[selected].clone()).await?;
                return Ok(true);
            }
            KeyEvent::Pressed(Key::Right) | KeyEvent::Autorepeat(Key::Right)
                if selected < 8 =>
            {
                self.cycle_value(selected, true);
                Self::send_udp_command(selected, &self.values[selected].clone()).await?;
                return Ok(true);
            }
            KeyEvent::Pressed(Key::A) if selected == 8 => {
                RetroArchCommand::ReloadConfig.send().await?;
                return Ok(true);
            }
            KeyEvent::Pressed(Key::A) if selected == 9 => {
                RetroArchCommand::ReloadConfig.send().await?;
                return Ok(true);
            }
            _ => {}
        }

        self.settings_list
            .handle_key_event(event, _commands, bubble)
            .await
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&self.game_name, &self.settings_list, &self.button_hints]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut self.game_name, &mut self.settings_list, &mut self.button_hints]
    }

    fn bounding_box(&mut self, _styles: &Stylesheet) -> Rect {
        self.rect
    }

    fn set_position(&mut self, point: Point) {
        self.rect.x = point.x;
        self.rect.y = point.y;
    }
}
