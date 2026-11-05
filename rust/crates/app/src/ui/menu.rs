//! Menus laid out as Source's options windows (VGUI's PropertyDialog): a window around everything with a small
//! title and a close button, tabs along the top of a bordered page of controls, the open tab joined to the page
//! with no line between them, and the window's own buttons below the page on the right.

use egui::{self, pos2, vec2, Rect, Sense, Ui, Vec2};

use crate::theme;
use crate::ui::widgets;

const TAB_HEIGHT: f32 = 24.0;
/// How much shorter a tab is while it isn't the open one.
const TAB_DROP: f32 = 3.0;
const TAB_PADDING: f32 = 11.0;
/// The size of the buttons below the page, wider if their labels need it.
const BUTTON: Vec2 = vec2(72.0, 24.0);

/// The window's frame, for windows and modals alike.
pub fn frame() -> egui::Frame {
    egui::Frame::new().fill(theme::SIDEBAR()).stroke(theme::field_stroke()).inner_margin(egui::Margin { left: 8, right: 8, top: 5, bottom: 8 })
}

/// A window for a menu: it starts with [`title`] rather than egui's title bar, and is as big as its contents.
pub fn window<'a>(id: impl Into<egui::WidgetText>) -> egui::Window<'a> {
    egui::Window::new(id).title_bar(false).collapsible(false).resizable(false).frame(frame())
}

/// The top row: the title in small capitals, maybe what it's about, and a close button. True when that's clicked.
pub fn title(ui: &mut Ui, text: &str, detail: Option<&str>) -> bool {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    let close = Rect::from_center_size(pos2(row.right() - 8.0, row.center().y), Vec2::splat(16.0));
    let r = ui.interact(close, ui.scope_id().with("menu-close"), Sense::click()).on_hover_text("Close");
    widgets::plain_cursor(&r);
    theme::register_button(ui.ctx(), close);
    let p = ui.painter();
    p.text(close.center(), egui::Align2::CENTER_CENTER, theme::icon::CLOSE, theme::icon_font(9.0), if r.hovered() { theme::HEADER() } else { theme::TEXT() });
    let name = p.layout_no_wrap(text.to_uppercase(), theme::font_bold(12.0), theme::HEADER());
    let name_width = name.size().x;
    p.galley(pos2(row.left(), row.center().y - name.size().y / 2.0), name, theme::HEADER());
    if let Some(detail) = detail {
        let left = row.left() + name_width + 8.0;
        let mut job = egui::text::LayoutJob::simple_singleline(detail.to_owned(), theme::font(12.5), theme::MUTED());
        job.wrap = egui::text::TextWrapping::truncate_at_width((close.left() - 8.0 - left).max(0.0));
        let detail = p.layout_job(job);
        p.galley(pos2(left, row.center().y - detail.size().y / 2.0), detail, theme::MUTED());
    }
    r.clicked()
}

/// The tabs over a page, to be given to [`page`].
pub struct Tabs {
    /// The open tab's inside, where the page's top line leaves off.
    gap: Option<egui::Rangef>,
    /// A tab was clicked that wasn't open.
    pub changed: bool,
}

/// Whole pixels and how thick a line is in them, at whole multiples as the scrollbar's, so the edges stay crisp.
fn pixels(ui: &Ui) -> (impl Fn(f32) -> f32 + use<>, f32) {
    let ppp = ui.ctx().pixels_per_point();
    (move |v: f32| (v * ppp).round() / ppp, ppp.floor().max(1.0) / ppp)
}

/// The menu's lines, as a mesh: egui's are feathered, which would blur them where tab meets page.
fn lines(ui: &Ui, rects: &[Rect]) {
    let mut mesh = egui::Mesh::default();
    for r in rects {
        mesh.add_colored_rect(*r, theme::BORDER());
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// A row of tabs: clicking one makes it `selected`. The next thing laid out is to be its [`page`], right below.
pub fn tabs<T: Copy + PartialEq, S: AsRef<str>>(ui: &mut Ui, selected: &mut T, tabs: &[(T, S)]) -> Tabs {
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), TAB_HEIGHT), Sense::hover());
    let (snap, thick) = pixels(ui);
    let font = theme::font(13.5);
    let (top, bottom) = (snap(strip.top()), snap(strip.bottom()));

    // Where they go and what's clicked first, so a click shows on this frame's tabs and page alike.
    let mut x = snap(strip.left());
    let mut placed = Vec::with_capacity(tabs.len());
    let mut changed = false;
    for (i, (value, label)) in tabs.iter().enumerate() {
        let width = snap(ui.painter().layout_no_wrap(label.as_ref().to_owned(), font.clone(), theme::TEXT()).size().x + 2.0 * TAB_PADDING);
        let rect = Rect::from_min_max(pos2(x, top), pos2(x + width, bottom));
        let r = ui.interact(rect, ui.scope_id().with(("menu-tab", i)), Sense::click());
        widgets::plain_cursor(&r);
        theme::register_button(ui.ctx(), rect);
        if r.clicked() && *value != *selected {
            *selected = *value;
            changed = true;
        }
        placed.push(r);
        x += width + thick;
    }

    let mut gap = None;
    let mut edges = Vec::new();
    for ((value, label), r) in tabs.iter().zip(&placed) {
        let open = *value == *selected;
        let rect = r.rect;
        r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, label.as_ref()));
        // The open tab is taller, and its sides run down through the page's top line to join it.
        let y = if open { top } else { snap(top + TAB_DROP) };
        let end = if open { bottom + thick } else { bottom };
        edges.push(Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right(), y + thick)));
        edges.push(Rect::from_min_max(pos2(rect.left(), y), pos2(rect.left() + thick, end)));
        edges.push(Rect::from_min_max(pos2(rect.right() - thick, y), pos2(rect.right(), end)));
        if open {
            gap = Some(egui::Rangef::new(rect.left() + thick, rect.right() - thick));
        }
        let color = if open {
            theme::HEADER()
        } else if r.hovered() {
            theme::TEXT()
        } else {
            theme::MUTED()
        };
        let body = Rect::from_min_max(pos2(rect.left(), y), rect.max);
        ui.painter().text(body.center(), egui::Align2::CENTER_CENTER, label.as_ref(), font.clone(), color);
        if r.has_focus() {
            ui.painter().rect_stroke(body.shrink(3.0), 0, egui::Stroke::new(1.0, theme::TEXT()), egui::StrokeKind::Inside);
        }
    }
    lines(ui, &edges);
    // The page starts where the tabs end.
    ui.add_space(-ui.spacing().item_spacing.y);
    Tabs { gap, changed }
}

/// The bordered page holding a menu's controls, under its `tabs` if it has any.
pub fn page<R>(ui: &mut Ui, tabs: Option<&Tabs>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let out = egui::Frame::new().inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Slider tracks and check boxes, sunk into the page as fields are: in the button's shade they'd barely show.
        ui.visuals_mut().widgets.inactive.bg_fill = theme::INPUT();
        add(ui)
    });
    let (snap, thick) = pixels(ui);
    let rect = out.response.rect;
    let (x0, x1, y0, y1) = (snap(rect.left()), snap(rect.right()), snap(rect.top()), snap(rect.bottom()));
    let top = |from: f32, to: f32| Rect::from_min_max(pos2(from, y0), pos2(to, y0 + thick));
    let mut edges = vec![
        Rect::from_min_max(pos2(x0, y0), pos2(x0 + thick, y1)),
        Rect::from_min_max(pos2(x1 - thick, y0), pos2(x1, y1)),
        Rect::from_min_max(pos2(x0, y1 - thick), pos2(x1, y1)),
    ];
    match tabs.and_then(|t| t.gap) {
        Some(gap) => {
            edges.push(top(x0, gap.min));
            edges.push(top(gap.max, x1));
        }
        None => edges.push(top(x0, x1)),
    }
    lines(ui, &edges);
    out.inner
}

/// The row of buttons below the page, added from the right (so the last added is the leftmost). Its height is
/// fixed: a layout centring in all the height left would hold the window at its tallest, and grow it whenever its
/// contents do.
pub fn buttons<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add_space(4.0);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), BUTTON.y), egui::Layout::right_to_left(egui::Align::Center), add).inner
}

/// A button for that row.
pub fn button<'a>(label: impl egui::IntoAtoms<'a>) -> egui::Button<'a> {
    egui::Button::new(label).min_size(BUTTON)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(pos: egui::Pos2, events: Vec<egui::Event>) -> egui::RawInput {
        let mut all = vec![egui::Event::PointerMoved(pos)];
        all.extend(events);
        egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0))), events: all, ..Default::default() }
    }

    fn button_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
    }

    /// The window: two tabs over a page. Gives back where the page's top line leaves off, and the tabs' row.
    fn frame_with(ctx: &egui::Context, input: egui::RawInput, selected: &mut u8) -> (Option<egui::Rangef>, bool, Rect) {
        let mut out = (None, false, Rect::NOTHING);
        let mut shown = ctx.run_ui(input, |ui| {
            let t = tabs(ui, selected, &[(1, "One"), (2, "Two")]);
            page(ui, Some(&t), |ui| ui.label("Page"));
            out = (t.gap, t.changed, ui.min_rect());
        });
        shown.textures_delta.clear();
        out
    }

    #[test]
    fn clicking_a_tab_opens_it_and_joins_it_to_the_page() {
        let ctx = egui::Context::default();
        let mut selected = 1;
        let (first, changed, _) = frame_with(&ctx, at(pos2(1.0, 1.0), vec![]), &mut selected);
        let first = first.expect("the open tab");
        assert!(!changed);
        // The page's line starts again after the first tab, so a point past it is on the second.
        let second = pos2(first.max + 20.0, TAB_HEIGHT / 2.0 + 8.0);
        frame_with(&ctx, at(second, vec![]), &mut selected);
        frame_with(&ctx, at(second, vec![button_event(second, true)]), &mut selected);
        let (gap, changed, _) = frame_with(&ctx, at(second, vec![button_event(second, false)]), &mut selected);
        assert_eq!(selected, 2);
        assert!(changed);
        // Already on this frame, the gap is under the tab that's now open.
        assert!(gap.expect("the open tab").min > first.max);
    }

    #[test]
    fn clicking_the_open_tab_changes_nothing() {
        let ctx = egui::Context::default();
        let mut selected = 1;
        let (first, ..) = frame_with(&ctx, at(pos2(1.0, 1.0), vec![]), &mut selected);
        let on_it = pos2(first.unwrap().center(), TAB_HEIGHT / 2.0 + 8.0);
        frame_with(&ctx, at(on_it, vec![]), &mut selected);
        frame_with(&ctx, at(on_it, vec![button_event(on_it, true)]), &mut selected);
        let (_, changed, _) = frame_with(&ctx, at(on_it, vec![button_event(on_it, false)]), &mut selected);
        assert_eq!(selected, 1);
        assert!(!changed);
    }
}
