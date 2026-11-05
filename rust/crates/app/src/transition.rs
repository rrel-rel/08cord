//! Transition effects: when the chat changes or a menu is dismissed, the old picture dissolves into the new one.
//! Only pixels that differ show the effect, and the new frame keeps drawing live underneath, so nothing waits for it.

use std::time::{Duration, Instant};

use egui::{Context, LayerId, Order, Rect};
use serde::{Deserialize, Serialize};
use swiftcord_core::Snowflake;

use crate::platform::d3d11::Snapshots;
use crate::platform::dissolve::Dissolve;

const CHAT_TIME: Duration = Duration::from_millis(450);
const MENU_TIME: Duration = Duration::from_millis(300);

/// As many cells across the longer side, unless that would make them smaller than `MIN_CELL` points.
const CELLS: f32 = 30.0;
const MIN_CELL: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Effect {
    #[default]
    Off,
    Diamonds,
    Blinds,
    Checkerboard,
}

impl Effect {
    pub const ALL: [Effect; 4] = [Effect::Off, Effect::Diamonds, Effect::Blinds, Effect::Checkerboard];

    pub fn name(self) -> &'static str {
        match self {
            Effect::Off => "Off",
            Effect::Diamonds => "Diamonds",
            Effect::Blinds => "Blinds",
            Effect::Checkerboard => "Checkerboard",
        }
    }
}

/// What to start: where, and for how long.
pub type Start = (Rect, Duration);

/// Both together, if both happened (leaving a channel closes its emoji picker, say).
fn merge(a: Option<Start>, b: Option<Start>) -> Option<Start> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a.0.union(b.0), a.1.max(b.1))),
        (a, b) => a.or(b),
    }
}

/// What the window should do this frame. It's run more than once if egui discards a pass, so what's asked for adds up.
#[derive(Debug, Default)]
pub struct Request {
    pub effect: Effect,
    start: Option<Start>,
}

impl Request {
    pub fn start(&mut self, start: Option<Start>) {
        self.start = merge(self.start, start);
    }
}

/// An open menu or window, as last drawn.
#[derive(Clone, Copy)]
struct Layer {
    id: LayerId,
    rect: Rect,
    /// It dims the whole window.
    modal: bool,
}

/// Notices what should start a transition, from one frame to the next.
#[derive(Default)]
pub struct Watcher {
    /// The server and channel showing.
    view: Option<(Option<Snowflake>, Option<Snowflake>)>,
    layers: Vec<Layer>,
}

impl Watcher {
    /// At the end of drawing a frame. `panes` are the channel list's and the chat's rectangles, if they're showing.
    pub fn update(&mut self, ctx: &Context, guild: Option<Snowflake>, channel: Option<Snowflake>, panes: Option<(Rect, Rect)>) -> Option<Start> {
        let window = ctx.content_rect();
        let view = Some((guild, channel));
        let chat = match (self.view, panes) {
            (Some(old), _) if Some(old) == view => None,
            (None, _) => None,
            // A different server changes its channel list too.
            (Some((old_guild, _)), Some((list, chat))) => Some((if old_guild == guild { chat } else { chat.union(list) }, CHAT_TIME)),
            (Some(_), None) => Some((window, CHAT_TIME)),
        };
        self.view = view;

        // Which layers have widgets says what was drawn in this very pass (egui's own list of visible areas has
        // the last frame's in it too, which would only say a menu had gone once its picture had gone as well).
        let modal = ctx.memory(|m| m.top_modal_layer());
        let layers: Vec<Layer> = ctx
            // (A layer stays listed after it's gone, with no widgets in it.)
            .viewport(|v| v.this_pass.widgets.layers().filter(|(_, widgets)| !widgets.is_empty()).map(|(id, _)| *id).collect::<Vec<_>>())
            .into_iter()
            // Not tooltips, which come and go with the pointer.
            .filter(|id| matches!(id.order, Order::Middle | Order::Foreground))
            .filter_map(|id| Some(Layer { id, rect: ctx.memory(|m| m.area_rect(id.id))?, modal: modal == Some(id) }))
            .collect();
        let mut menus = None;
        for gone in self.layers.iter().filter(|old| !layers.iter().any(|new| new.id == old.id)) {
            let rect = if gone.modal { window } else { gone.rect.expand(2.0) };
            menus = merge(menus, Some((rect, MENU_TIME)));
        }
        self.layers = layers;
        merge(chat, menus)
    }
}

struct Running {
    started: Instant,
    duration: Duration,
    rect: Rect,
}

/// The running transition, if any.
#[derive(Default)]
pub struct Player {
    running: Option<Running>,
}

impl Player {
    /// What to draw for the frame just asked for. `size` is the window's in pixels.
    pub fn frame(&mut self, request: Request, now: Instant, pixels_per_point: f32, size: [u32; 2]) -> Snapshots {
        if request.effect == Effect::Off {
            self.running = None;
            return Snapshots::Off;
        }
        let restart = request.start.is_some();
        if let Some((rect, duration)) = request.start {
            // What's still dissolving has to stay inside, or it would snap to the new frame.
            let rect = self.running.as_ref().map_or(rect, |r| r.rect.union(rect));
            self.running = Some(Running { started: now, duration, rect });
        }
        let Some(running) = &self.running else { return Snapshots::Keep };
        let progress = now.saturating_duration_since(running.started).as_secs_f32() / running.duration.as_secs_f32();
        if progress >= 1.0 {
            self.running = None;
            return Snapshots::Keep;
        }
        let [left, top, right, bottom] = pixels(running.rect, pixels_per_point, size);
        if right == left || bottom == top {
            self.running = None;
            return Snapshots::Keep;
        }
        let cell = ((right - left).max(bottom - top) as f32 / CELLS).max(MIN_CELL * pixels_per_point);
        Snapshots::Dissolve(Dissolve { effect: request.effect as u32, rect: [left, top, right, bottom], progress, cell, restart })
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }
}

/// A rectangle in points, as whole pixels inside the window.
fn pixels(rect: Rect, pixels_per_point: f32, [width, height]: [u32; 2]) -> [u32; 4] {
    let x = |v: f32| ((v * pixels_per_point).round().max(0.0) as u32).min(width);
    let y = |v: f32| ((v * pixels_per_point).round().max(0.0) as u32).min(height);
    let (left, top) = (x(rect.min.x), y(rect.min.y));
    [left, top, x(rect.max.x).max(left), y(rect.max.y).max(top)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2, Id};

    const WINDOW: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(800.0, 600.0) };
    const LIST: Rect = Rect { min: pos2(72.0, 0.0), max: pos2(312.0, 600.0) };
    const CHAT: Rect = Rect { min: pos2(312.0, 0.0), max: pos2(800.0, 600.0) };
    const GUILD: Option<Snowflake> = Some(Snowflake(1));
    const CHANNEL: Option<Snowflake> = Some(Snowflake(2));
    const OTHER: Option<Snowflake> = Some(Snowflake(3));

    /// Runs a frame the way the app does, with the watcher looking at the end of it.
    fn frame(ctx: &Context, watcher: &mut Watcher, view: (Option<Snowflake>, Option<Snowflake>), mut add: impl FnMut(&mut egui::Ui)) -> Option<Start> {
        let input = egui::RawInput { screen_rect: Some(WINDOW), ..Default::default() };
        let mut found = None;
        let mut out = ctx.run_ui(input, |ui| {
            add(ui);
            found = merge(found, watcher.update(ui.ctx(), view.0, view.1, Some((LIST, CHAT))));
        });
        out.textures_delta.clear();
        found
    }

    fn menu(ui: &mut egui::Ui, order: Order) {
        egui::Area::new(Id::unique("menu")).order(order).fixed_pos(pos2(100.0, 100.0)).show(ui.ctx(), |ui| {
            ui.set_min_size(vec2(50.0, 50.0));
            let _ = ui.button("An item");
        });
    }

    #[test]
    fn opening_the_first_view_does_nothing() {
        let (ctx, mut w) = (Context::default(), Watcher::default());
        assert_eq!(frame(&ctx, &mut w, (GUILD, CHANNEL), |_| {}), None);
        assert_eq!(frame(&ctx, &mut w, (GUILD, CHANNEL), |_| {}), None);
    }

    #[test]
    fn another_channel_dissolves_the_chat() {
        let (ctx, mut w) = (Context::default(), Watcher::default());
        frame(&ctx, &mut w, (GUILD, CHANNEL), |_| {});
        assert_eq!(frame(&ctx, &mut w, (GUILD, OTHER), |_| {}), Some((CHAT, CHAT_TIME)));
    }

    #[test]
    fn another_server_dissolves_its_channel_list_too() {
        let (ctx, mut w) = (Context::default(), Watcher::default());
        frame(&ctx, &mut w, (GUILD, CHANNEL), |_| {});
        assert_eq!(frame(&ctx, &mut w, (None, CHANNEL), |_| {}), Some((CHAT.union(LIST), CHAT_TIME)));
    }

    #[test]
    fn a_dismissed_menu_dissolves_on_the_first_frame_without_it() {
        let (ctx, mut w) = (Context::default(), Watcher::default());
        let view = (GUILD, CHANNEL);
        // Twice, as an area is sized on the frame it first appears.
        for _ in 0..2 {
            assert_eq!(frame(&ctx, &mut w, view, |ui| menu(ui, Order::Foreground)), None);
        }
        let (rect, time) = frame(&ctx, &mut w, view, |_| {}).expect("the menu's gone");
        assert_eq!(time, MENU_TIME);
        assert_eq!(rect.min, pos2(98.0, 98.0));
        assert!(rect.width() >= 54.0, "{rect:?}");
        assert_eq!(frame(&ctx, &mut w, view, |_| {}), None);
    }

    #[test]
    fn a_closed_window_counts_as_a_menu() {
        let (ctx, mut w) = (Context::default(), Watcher::default());
        let view = (GUILD, CHANNEL);
        for _ in 0..2 {
            frame(&ctx, &mut w, view, |ui| {
                egui::Window::new("A window").show(ui.ctx(), |ui| ui.label("In it"));
            });
        }
        assert!(frame(&ctx, &mut w, view, |_| {}).is_some());
    }

    #[test]
    fn a_tooltip_going_away_does_not() {
        let (ctx, mut w) = (Context::default(), Watcher::default());
        let view = (GUILD, CHANNEL);
        for _ in 0..2 {
            frame(&ctx, &mut w, view, |ui| menu(ui, Order::Tooltip));
        }
        assert_eq!(frame(&ctx, &mut w, view, |_| {}), None);
    }

    #[test]
    fn a_closed_modal_dissolves_the_whole_window_for_its_backdrop() {
        let (ctx, mut w) = (Context::default(), Watcher::default());
        let view = (GUILD, CHANNEL);
        for _ in 0..3 {
            frame(&ctx, &mut w, view, |ui| {
                egui::Modal::new(Id::unique("modal")).show(ui.ctx(), |ui| ui.label("Sure?"));
            });
        }
        assert_eq!(frame(&ctx, &mut w, view, |_| {}), Some((WINDOW, MENU_TIME)));
    }

    #[test]
    fn a_context_menu_closing_is_seen() {
        let (ctx, mut w) = (Context::default(), Watcher::default());
        let view = (GUILD, CHANNEL);
        let popup = |ui: &mut egui::Ui, open: bool| {
            if open {
                let r = ui.button("Anchor");
                egui::Popup::menu(&r).open(true).show(|ui| {
                    let _ = ui.button("An item");
                });
            }
        };
        for _ in 0..3 {
            frame(&ctx, &mut w, view, |ui| popup(ui, true));
        }
        assert!(frame(&ctx, &mut w, view, |ui| popup(ui, false)).is_some());
    }

    fn request(effect: Effect, start: Option<Start>) -> Request {
        Request { effect, start }
    }

    #[test]
    fn it_runs_from_start_to_finish() {
        let mut p = Player::default();
        let t = Instant::now();
        let start = Some((WINDOW, Duration::from_millis(100)));
        let at = |ms| t + Duration::from_millis(ms);
        let Snapshots::Dissolve(d) = p.frame(request(Effect::Blinds, start), t, 1.0, [800, 600]) else { panic!("should start") };
        assert_eq!((d.effect, d.rect, d.progress, d.restart), (2, [0, 0, 800, 600], 0.0, true));
        let Snapshots::Dissolve(d) = p.frame(request(Effect::Blinds, None), at(50), 1.0, [800, 600]) else { panic!("should be running") };
        assert!(!d.restart && (d.progress - 0.5).abs() < 0.01);
        assert!(p.is_running());
        assert!(matches!(p.frame(request(Effect::Blinds, None), at(100), 1.0, [800, 600]), Snapshots::Keep));
        assert!(!p.is_running());
    }

    #[test]
    fn off_keeps_nothing_and_stops_what_was_running() {
        let mut p = Player::default();
        let t = Instant::now();
        assert!(matches!(p.frame(request(Effect::Off, Some((WINDOW, CHAT_TIME))), t, 1.0, [800, 600]), Snapshots::Off));
        assert!(!p.is_running());
        p.frame(request(Effect::Diamonds, Some((WINDOW, CHAT_TIME))), t, 1.0, [800, 600]);
        assert!(matches!(p.frame(request(Effect::Off, None), t, 1.0, [800, 600]), Snapshots::Off));
        assert!(!p.is_running());
    }

    #[test]
    fn starting_again_keeps_what_was_still_dissolving_inside() {
        let mut p = Player::default();
        let t = Instant::now();
        p.frame(request(Effect::Diamonds, Some((CHAT, CHAT_TIME))), t, 1.0, [800, 600]);
        let Snapshots::Dissolve(d) = p.frame(request(Effect::Diamonds, Some((LIST, MENU_TIME))), t, 1.0, [800, 600]) else { panic!() };
        assert_eq!((d.rect, d.restart), ([72, 0, 800, 600], true));
    }

    #[test]
    fn rectangles_become_pixels_inside_the_window() {
        assert_eq!(pixels(Rect::from_min_max(pos2(10.0, 20.0), pos2(30.0, 40.0)), 1.5, [800, 600]), [15, 30, 45, 60]);
        assert_eq!(pixels(Rect::from_min_max(pos2(-5.0, -5.0), pos2(900.0, 900.0)), 1.0, [800, 600]), [0, 0, 800, 600]);
    }

    #[test]
    fn nothing_visible_means_no_transition() {
        let mut p = Player::default();
        let off_screen = Rect::from_min_max(pos2(900.0, 900.0), pos2(950.0, 950.0));
        assert!(matches!(p.frame(request(Effect::Diamonds, Some((off_screen, CHAT_TIME))), Instant::now(), 1.0, [800, 600]), Snapshots::Keep));
        assert!(!p.is_running());
    }

    #[test]
    fn cells_are_square_with_a_smallest_size() {
        let mut p = Player::default();
        let t = Instant::now();
        let Snapshots::Dissolve(d) = p.frame(request(Effect::Diamonds, Some((WINDOW, CHAT_TIME))), t, 1.0, [800, 600]) else { panic!() };
        assert!((d.cell - 800.0 / 30.0).abs() < 0.01);
        let small = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let mut p = Player::default();
        let Snapshots::Dissolve(d) = p.frame(request(Effect::Diamonds, Some((small, CHAT_TIME))), t, 2.0, [800, 600]) else { panic!() };
        assert_eq!(d.cell, 20.0);
    }

    #[test]
    fn the_shader_numbers_its_patterns_as_the_settings_do() {
        assert_eq!((Effect::Diamonds as u32, Effect::Blinds as u32, Effect::Checkerboard as u32), (1, 2, 3));
    }
}
