#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod model;
mod render;

use eframe::egui::{self, Color32, Pos2, Vec2, ViewportCommand, ViewportId};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use model::Drawing;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Cursor,
    sync::mpsc,
    time::{Duration, Instant},
};
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};
include!(concat!(env!("OUT_DIR"), "/assets.rs"));

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    hand: usize,
    size: f32,
    fade: f32,
    sound: usize,
    muted: bool,
    ink: [u8; 3],
    width: f32,
    shadow: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            hand: HANDS.iter().position(|(n, _)| *n == "lavender").unwrap(),
            size: 0.13,
            fade: 3.0,
            sound: 4,
            muted: false,
            ink: [135, 84, 242],
            width: 5.0,
            shadow: true,
        }
    }
}
impl Preferences {
    fn validate(&mut self) {
        self.hand = self.hand.min(HANDS.len());
        self.sound = self.sound.min(SOUNDS.len() - 1);
        self.size = finite_clamp(self.size, 0.08, 0.20, 0.13);
        self.fade = finite_clamp(self.fade, 0.0, 30.0, 3.0);
        self.width = finite_clamp(self.width, 1.0, 20.0, 5.0);
    }
    fn color(&self) -> Color32 {
        Color32::from_rgb(self.ink[0], self.ink[1], self.ink[2])
    }
    fn hand_name(&self) -> &str {
        HANDS.get(self.hand).map_or("Laser", |(name, _)| name)
    }
}
fn finite_clamp(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

struct TinyHand {
    prefs: Preferences,
    active: bool,
    quitting: bool,
    displays: Vec<display_info::DisplayInfo>,
    drawings: HashMap<u32, Drawing>,
    textures: HashMap<usize, render::Hand>,
    status: String,
    _tray: TrayIcon,
    _hotkeys: GlobalHotKeyManager,
    toggle_key: u32,
    clear_key: u32,
    toggle_menu: MenuItem,
    settings_menu: MenuItem,
    quit_menu: MenuItem,
    hotkey_events: mpsc::Receiver<GlobalHotKeyEvent>,
    menu_events: mpsc::Receiver<MenuEvent>,
    _audio_stream: Option<rodio::OutputStream>,
    audio: Option<rodio::OutputStreamHandle>,
}

impl TinyHand {
    fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(target_os = "linux")]
        gtk::init()?;
        let mut prefs: Preferences = cc
            .storage
            .and_then(|s| eframe::get_value(s, "preferences"))
            .unwrap_or_default();
        prefs.validate();
        // X11 may deliver hotkeys on its event thread as soon as they are registered.
        let (hotkey_tx, hotkey_events) = mpsc::channel();
        let wake = cc.egui_ctx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event| {
            let _ = hotkey_tx.send(event);
            wake.request_repaint_of(ViewportId::ROOT);
        }));
        let (menu_tx, menu_events) = mpsc::channel();
        let wake = cc.egui_ctx.clone();
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = menu_tx.send(event);
            wake.request_repaint_of(ViewportId::ROOT);
        }));
        let menu = Menu::new();
        let toggle_menu = MenuItem::new("Toggle pointer (Ctrl+Alt+H)", true, None);
        let settings_menu = MenuItem::new("Settings", true, None);
        let quit_menu = MenuItem::new("Quit TinyHand", true, None);
        menu.append_items(&[&toggle_menu, &settings_menu, &quit_menu])?;
        let icon = image::load_from_memory(include_bytes!("../../.github/logo.png"))?
            .resize(32, 32, image::imageops::FilterType::Triangle)
            .into_rgba8();
        let (w, h) = icon.dimensions();
        let tray = TrayIconBuilder::new()
            .with_tooltip("TinyHand — Ctrl+Alt+H")
            .with_icon(tray_icon::Icon::from_rgba(icon.into_raw(), w, h)?)
            .with_menu(Box::new(menu))
            .build()?;
        let hotkeys = GlobalHotKeyManager::new()?;
        let toggle = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyH);
        let clear = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyX);
        // Shortcut conflicts must be visible at startup, never leave an unreachable overlay.
        hotkeys.register(toggle)?;
        hotkeys.register(clear)?;
        let displays = display_info::DisplayInfo::all()?;
        if displays.is_empty() {
            return Err("No displays found".into());
        }
        let (stream, audio, status) = match rodio::OutputStream::try_default() {
            Ok((s, h)) => (Some(s), Some(h), String::new()),
            Err(e) => (None, None, format!("Audio unavailable: {e}")),
        };
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Ok(Self {
            prefs,
            active: false,
            quitting: false,
            displays,
            drawings: HashMap::new(),
            textures: HashMap::new(),
            status,
            _tray: tray,
            _hotkeys: hotkeys,
            toggle_key: toggle.id(),
            clear_key: clear.id(),
            toggle_menu,
            settings_menu,
            quit_menu,
            hotkey_events,
            menu_events,
            _audio_stream: stream,
            audio,
        })
    }
    fn clear(&mut self) {
        for d in self.drawings.values_mut() {
            d.clear();
        }
    }
    fn set_active(&mut self, ctx: &egui::Context, active: bool) {
        if active {
            match display_info::DisplayInfo::all() {
                Ok(displays) if !displays.is_empty() => self.displays = displays,
                result => {
                    self.status = match result {
                        Ok(_) => "No displays found".into(),
                        Err(e) => format!("Display discovery failed: {e}"),
                    };
                    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(true));
                    return;
                }
            }
        }
        self.active = active;
        self.clear();
        for d in &self.displays {
            let id = ViewportId::from_hash_of(d.id);
            if !active {
                ctx.send_viewport_cmd_to(id, ViewportCommand::Close);
            }
        }
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(!active));
        if !active {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
        }
    }
    fn tap(&mut self) {
        if self.prefs.muted {
            return;
        }
        if let Some(audio) = &self.audio {
            match audio.play_once(Cursor::new(SOUNDS[self.prefs.sound].1)) {
                Ok(sink) => sink.detach(),
                Err(e) => self.status = format!("Could not play tap: {e}"),
            }
        }
    }
    fn controls(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("hand")
            .selected_text(self.prefs.hand_name())
            .show_ui(ui, |ui| {
                for (i, (name, _)) in HANDS.iter().enumerate() {
                    ui.selectable_value(&mut self.prefs.hand, i, *name);
                }
                ui.selectable_value(&mut self.prefs.hand, HANDS.len(), "Laser");
            });
        ui.checkbox(&mut self.prefs.muted, "Mute");
        let ink_label = ui.label("Ink");
        ui.color_edit_button_srgb(&mut self.prefs.ink)
            .labelled_by(ink_label.id);
        if ui.button("Clear").clicked() {
            self.clear();
        }
    }
    fn overlay(&mut self, ctx: &egui::Context, display: display_info::DisplayInfo) {
        // Use the actual window DPI, not the display library's legacy Windows DPI estimate.
        let scale = if cfg!(target_os = "macos") {
            1.0
        } else {
            ctx.pixels_per_point()
        };
        let pos = Pos2::new(display.x as f32 / scale, display.y as f32 / scale);
        let size = Vec2::new(display.width as f32 / scale, display.height as f32 / scale);
        let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
        if outer.is_none_or(|r| r.min.distance(pos) > 0.5) {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
        }
        if inner.is_none_or(|r| (r.size() - size).length() > 0.5) {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        }
        let screen = ctx.content_rect();
        let mut stop = false;
        let toolbar = egui::Area::new(egui::Id::new("toolbar"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(14)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            self.controls(ui);
                            if ui.button("Stop · Esc").clicked() {
                                stop = true;
                            }
                            if ui.button("Quit").clicked() {
                                self.quitting = true;
                            }
                        });
                        if !self.status.is_empty() {
                            ui.colored_label(Color32::YELLOW, &self.status);
                        }
                    });
            })
            .response
            .rect;
        let now = Instant::now();
        let mut drawing = self.drawings.remove(&display.id).unwrap_or_default();
        // Events preserve press/move/release ordering, including a complete tap in one frame.
        for event in ctx.input(|i| i.events.clone()) {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if !toolbar.contains(pos) && !ctx.is_pointer_over_area() => {
                    drawing.begin(pos, self.prefs.color(), self.prefs.width);
                    self.tap();
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => {
                    drawing.push(pos);
                    drawing.end();
                }
                egui::Event::PointerMoved(pos) => drawing.push(pos),
                egui::Event::PointerGone => drawing.end(),
                _ => {}
            }
        }
        drawing.expire(now, self.prefs.fade);
        let painter = ctx.layer_painter(egui::LayerId::background());
        for s in &drawing.strokes {
            let color = s
                .color
                .gamma_multiply(model::opacity(s.ended, now, self.prefs.fade));
            if s.points.len() == 1 {
                painter.circle_filled(s.points[0], s.width / 2.0, color);
            } else {
                painter.add(egui::Shape::line(
                    s.points.clone(),
                    egui::Stroke::new(s.width, color),
                ));
            }
        }
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos())
            && !toolbar.contains(pos)
            && !ctx.is_pointer_over_area()
        {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            if self.prefs.hand == HANDS.len() {
                let r = screen.height() * self.prefs.size * 0.035;
                for n in (1..=4).rev() {
                    painter.circle_filled(
                        pos,
                        r * n as f32,
                        Color32::from_rgba_premultiplied(40, 0, 0, 20),
                    );
                }
                painter.circle_filled(pos, r, Color32::RED);
                painter.circle_filled(pos, r * 0.4, Color32::WHITE);
            } else {
                let index = self.prefs.hand;
                if let std::collections::hash_map::Entry::Vacant(e) = self.textures.entry(index) {
                    match render::Hand::load(ctx, HANDS[index].0, HANDS[index].1) {
                        Ok(hand) => {
                            e.insert(hand);
                        }
                        Err(err) => self.status = err,
                    }
                }
                if let Some(hand) = self.textures.get(&index) {
                    hand.paint(
                        &painter,
                        pos,
                        screen,
                        self.prefs.size,
                        drawing.drawing,
                        self.prefs.shadow,
                    );
                }
            }
        }
        self.drawings.insert(display.id, drawing);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            || ctx.input(|i| i.viewport().close_requested())
        {
            stop = true;
        }
        if stop {
            self.set_active(ctx, false);
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl eframe::App for TinyHand {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "preferences", &self.prefs);
    }
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        while let Ok(event) = self.hotkey_events.try_recv() {
            if event.state == HotKeyState::Pressed {
                if event.id == self.toggle_key {
                    self.set_active(ctx, !self.active);
                }
                if event.id == self.clear_key {
                    self.clear();
                }
            }
        }
        while let Ok(event) = self.menu_events.try_recv() {
            if event.id == *self.toggle_menu.id() {
                self.set_active(ctx, !self.active);
            }
            if event.id == *self.settings_menu.id() {
                self.set_active(ctx, false);
            }
            if event.id == *self.quit_menu.id() {
                self.quitting = true;
            }
        }
        if self.quitting {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("TinyHand");
            ui.label("A tiny hand on a stick. Point, tap, and draw over your desktop.");
            ui.add_space(12.0);
            ui.horizontal(|ui| self.controls(ui));
            ui.add(egui::Slider::new(&mut self.prefs.size, 0.08..=0.20).text("Hand size"));
            ui.add(egui::Slider::new(&mut self.prefs.width, 1.0..=20.0).text("Ink width"));
            ui.add(
                egui::Slider::new(&mut self.prefs.fade, 0.0..=30.0).text("Ink lifetime (0 = keep)"),
            );
            ui.checkbox(&mut self.prefs.shadow, "Hand shadow");
            egui::ComboBox::from_label("Tap sound")
                .selected_text(SOUNDS[self.prefs.sound].0)
                .show_ui(ui, |ui| {
                    for (i, (name, _)) in SOUNDS.iter().enumerate() {
                        ui.selectable_value(&mut self.prefs.sound, i, *name);
                    }
                });
            ui.separator();
            ui.label("Ctrl+Alt+H: toggle   ·   Ctrl+Alt+X: clear   ·   Esc: stop");
            ui.label("While active, clicks and drags draw instead of reaching other apps.");
            ui.horizontal(|ui| {
                if ui.button("Start pointer").clicked() {
                    self.set_active(ctx, true);
                }
                if ui.button("Reset settings").clicked() {
                    self.prefs = Preferences::default();
                }
                if ui.button("Quit").clicked() {
                    self.quitting = true;
                }
            });
            if !self.status.is_empty() {
                ui.colored_label(Color32::YELLOW, &self.status);
            }
        });
        if self.active {
            for display in self.displays.clone() {
                if !self.active {
                    break;
                }
                // CoreGraphics reports display bounds in points; the other backends use pixels.
                let scale = if cfg!(target_os = "macos") {
                    1.0
                } else {
                    display.scale_factor
                };
                let builder = egui::ViewportBuilder::default()
                    .with_title("TinyHand Overlay")
                    .with_transparent(true)
                    .with_decorations(false)
                    .with_has_shadow(false)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_resizable(false)
                    .with_position(Pos2::new(
                        display.x as f32 / scale,
                        display.y as f32 / scale,
                    ))
                    .with_inner_size(Vec2::new(
                        display.width as f32 / scale,
                        display.height as f32 / scale,
                    ));
                ctx.show_viewport_immediate(
                    ViewportId::from_hash_of(display.id),
                    builder,
                    |ctx, _| self.overlay(ctx, display),
                );
            }
        }
        if self.quitting {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(if self.active { 16 } else { 100 }));
    }
}
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TinyHand · Settings")
            .with_transparent(true)
            .with_inner_size([560.0, 360.0])
            .with_min_inner_size([480.0, 340.0]),
        ..Default::default()
    };
    eframe::run_native(
        "TinyHand",
        options,
        Box::new(|cc| Ok(Box::new(TinyHand::new(cc)?))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_saved_preferences_are_bounded() {
        let mut p = Preferences {
            hand: usize::MAX,
            sound: usize::MAX,
            size: f32::NAN,
            fade: f32::INFINITY,
            width: -1.0,
            ..Preferences::default()
        };
        p.validate();
        assert_eq!(p.hand, HANDS.len());
        assert_eq!(p.sound, 5);
        assert_eq!(p.size, 0.13);
        assert_eq!(p.fade, 3.0);
        assert_eq!(p.width, 1.0);
    }
    #[test]
    fn all_embedded_hands_and_sounds_decode() {
        assert_eq!(HANDS.len(), 36);
        for (name, bytes) in HANDS {
            let img = image::load_from_memory(bytes).unwrap().into_rgba8();
            let g = model::HandGeometry::measure(&img).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(g.top < g.wrist && g.wrist < 1.0, "{name}");
        }
        for (_, bytes) in SOUNDS {
            rodio::Decoder::new(Cursor::new(*bytes)).unwrap();
        }
    }
}
