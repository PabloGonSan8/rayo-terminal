//! Window chrome: custom title bar pieces, hand-drawn icons, window controls
//! and edge resizing for the undecorated window.

use egui::{
    Color32, CursorIcon, Painter, Pos2, Rect, ResizeDirection, Response, Sense, Stroke, Ui, Vec2,
    ViewportCommand,
};

use crate::theme::Theme;

pub const BAR_H: f32 = 36.0;

#[derive(Clone, Copy)]
pub enum Icon {
    Plus,
    SplitRight,
    SplitDown,
    Gear,
    Agents,
    Minimize,
    Maximize,
    Restore,
    Close,
    SmallClose,
}

pub fn draw_icon(p: &Painter, icon: Icon, c: Pos2, color: Color32) {
    let s = Stroke::new(1.15, color);
    let r = |w: f32, h: f32| Rect::from_center_size(c, Vec2::new(w, h));
    match icon {
        Icon::Plus => {
            p.hline(c.x - 5.0..=c.x + 5.0, c.y, s);
            p.vline(c.x, c.y - 5.0..=c.y + 5.0, s);
        }
        Icon::SplitRight => {
            p.rect_stroke(r(13.0, 11.0), 2.0, s, egui::StrokeKind::Middle);
            p.vline(c.x, c.y - 5.5..=c.y + 5.5, s);
        }
        Icon::SplitDown => {
            p.rect_stroke(r(13.0, 11.0), 2.0, s, egui::StrokeKind::Middle);
            p.hline(c.x - 6.5..=c.x + 6.5, c.y, s);
        }
        Icon::Gear => {
            p.circle_stroke(c, 2.2, s);
            p.circle_stroke(c, 4.6, s);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let d = Vec2::angled(a);
                p.line_segment([c + d * 4.6, c + d * 6.6], Stroke::new(1.9, color));
            }
        }
        // Spark: four-point star.
        Icon::Agents => {
            let pts = (0..8)
                .map(|k| {
                    let a = k as f32 * std::f32::consts::TAU / 8.0 - std::f32::consts::FRAC_PI_2;
                    c + Vec2::angled(a) * if k % 2 == 0 { 6.5 } else { 1.8 }
                })
                .collect();
            p.add(egui::Shape::closed_line(pts, s));
        }
        Icon::Minimize => {
            p.hline(c.x - 5.0..=c.x + 5.0, c.y, Stroke::new(1.0, color));
        }
        Icon::Maximize => {
            p.rect_stroke(
                r(10.0, 10.0),
                1.5,
                Stroke::new(1.0, color),
                egui::StrokeKind::Middle,
            );
        }
        Icon::Restore => {
            let s1 = Stroke::new(1.0, color);
            p.rect_stroke(
                Rect::from_center_size(c + Vec2::new(-1.2, 1.2), Vec2::splat(8.0)),
                1.5,
                s1,
                egui::StrokeKind::Middle,
            );
            p.line_segment([c + Vec2::new(-2.0, -4.0), c + Vec2::new(4.0, -4.0)], s1);
            p.line_segment([c + Vec2::new(4.0, -4.0), c + Vec2::new(4.0, 2.0)], s1);
        }
        Icon::Close => {
            let s1 = Stroke::new(1.0, color);
            p.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], s1);
            p.line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], s1);
        }
        Icon::SmallClose => {
            p.line_segment([c + Vec2::new(-3.3, -3.3), c + Vec2::new(3.3, 3.3)], s);
            p.line_segment([c + Vec2::new(-3.3, 3.3), c + Vec2::new(3.3, -3.3)], s);
        }
    }
}

#[derive(Clone, Copy)]
pub enum ChipIcon {
    Folder,
    Branch,
}

/// Folder outline centred on `c` (drawn, so it never depends on the font having an emoji).
pub fn draw_folder(p: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.2, color);
    // Body with a tab on the top-left.
    p.rect_stroke(
        Rect::from_center_size(c + Vec2::new(0.0, 1.0), Vec2::new(13.0, 9.5)),
        1.5,
        s,
        egui::StrokeKind::Middle,
    );
    p.line_segment([c + Vec2::new(-6.5, -3.0), c + Vec2::new(-6.5, -5.0)], s);
    p.line_segment([c + Vec2::new(-6.5, -5.0), c + Vec2::new(-1.5, -5.0)], s);
    p.line_segment([c + Vec2::new(-1.5, -5.0), c + Vec2::new(0.0, -3.7)], s);
}

/// Warp-style context chip: rounded pill with an icon and a label. Returns its width.
pub fn chip(
    p: &Painter,
    left_center: Pos2,
    icon: ChipIcon,
    text: &str,
    font: egui::FontId,
    color: Color32,
    t: &Theme,
) -> Rect {
    let galley = p.layout_no_wrap(text.to_string(), font, color);
    let h = (galley.size().y + 8.0).max(22.0);
    let w = 8.0 + 14.0 + 6.0 + galley.size().x + 9.0;
    let r = Rect::from_min_size(
        Pos2::new(left_center.x, left_center.y - h / 2.0),
        Vec2::new(w, h),
    );
    p.rect_filled(r, 6.0, t.bg3());
    p.rect_stroke(
        r,
        6.0,
        Stroke::new(1.0, t.border()),
        egui::StrokeKind::Inside,
    );
    let c = Pos2::new(r.min.x + 15.0, r.center().y);
    let s = Stroke::new(1.2, color);
    match icon {
        ChipIcon::Folder => draw_folder(p, c, color),
        ChipIcon::Branch => {
            let (a, b, d) = (
                c + Vec2::new(-3.0, -5.0),
                c + Vec2::new(-3.0, 5.0),
                c + Vec2::new(3.5, -3.0),
            );
            for q in [a, b, d] {
                p.circle_stroke(q, 1.7, s);
            }
            p.line_segment([a + Vec2::new(0.0, 1.7), b - Vec2::new(0.0, 1.7)], s);
            // Merge curve from the side branch down into the trunk.
            let pts = [
                d + Vec2::new(0.0, 1.7),
                d + Vec2::new(0.0, 3.5),
                c + Vec2::new(0.5, 2.8),
                b + Vec2::new(0.6, -2.6),
            ];
            for w2 in pts.windows(2) {
                p.line_segment([w2[0], w2[1]], s);
            }
        }
    }
    p.galley(
        Pos2::new(r.min.x + 28.0, r.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    r
}

/// Transparent icon button with a quick hover fade and a tooltip.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, size: Vec2, t: &Theme) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_icon_button(ui, &resp, rect, icon, t, false);
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    }
}

pub fn paint_icon_button(
    ui: &Ui,
    resp: &Response,
    rect: Rect,
    icon: Icon,
    t: &Theme,
    danger: bool,
) {
    let h = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.08);
    let p = ui.painter();
    if h > 0.0 {
        let fill = if danger {
            Color32::from_rgb(196, 43, 28)
        } else {
            t.bg3()
        };
        let radius = if danger { 0.0 } else { 6.0 };
        p.rect_filled(rect, radius, fill.gamma_multiply(h));
    }
    let base = t.muted();
    let hot = if danger { Color32::WHITE } else { t.fg };
    let color = if resp.is_pointer_button_down_on() {
        t.fg
    } else {
        base.lerp_to_gamma(hot, h)
    };
    draw_icon(p, icon, rect.center(), color);
}

/// Minimize / maximize / close, Windows-style (flush right, full bar height).
pub fn window_controls(ui: &mut Ui, t: &Theme) {
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let size = Vec2::new(46.0, BAR_H);
    let buttons = [
        (Icon::Close, "Cerrar", true),
        (
            if maximized {
                Icon::Restore
            } else {
                Icon::Maximize
            },
            if maximized { "Restaurar" } else { "Maximizar" },
            false,
        ),
        (Icon::Minimize, "Minimizar", false),
    ];
    for (icon, tip, danger) in buttons {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        paint_icon_button(ui, &resp, rect, icon, t, danger);
        let resp = resp.on_hover_text(crate::i18n::t(tip));
        if resp.clicked() {
            ctx.send_viewport_cmd(match icon {
                Icon::Close => ViewportCommand::Close,
                Icon::Minimize => ViewportCommand::Minimized(true),
                _ => ViewportCommand::Maximized(!maximized),
            });
        }
    }
}

/// Move the window with the mouse. A maximized window is restored first, as
/// Windows does with native title bars (it can't be moved while maximized).
pub fn start_window_drag(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
    }
    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
}

/// Empty title bar space: drag to move, double click to maximize.
pub fn drag_area(ui: &mut Ui, rect: Rect) {
    let resp = ui.interact(rect, ui.scope_id().with("drag"), Sense::click_and_drag());
    if resp.drag_started() {
        start_window_drag(ui.ctx());
    }
    if resp.double_clicked() {
        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!max));
    }
}

/// Resize from the window edges (the window has no native frame).
/// Returns true when the pointer is on an edge, so other widgets can ignore the press.
pub fn edge_resize(ctx: &egui::Context) -> bool {
    let (max, screen, pos, pressed) = ctx.input(|i| {
        (
            i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false),
            i.content_rect(),
            i.pointer.hover_pos(),
            i.pointer.primary_pressed(),
        )
    });
    let Some(p) = pos.filter(|_| !max) else {
        return false;
    };
    const M: f32 = 5.0;
    let (l, r, t, b) = (
        p.x - screen.min.x < M,
        screen.max.x - p.x < M,
        p.y - screen.min.y < M,
        screen.max.y - p.y < M,
    );
    let dir = match (l, r, t, b) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (_, true, true, _) => ResizeDirection::NorthEast,
        (true, _, _, true) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::West,
        (_, true, ..) => ResizeDirection::East,
        (_, _, true, _) => ResizeDirection::North,
        (.., true) => ResizeDirection::South,
        _ => return false,
    };
    ctx.set_cursor_icon(match dir {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        _ => CursorIcon::ResizeNeSw,
    });
    if pressed {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
    }
    true
}

/// Ask DWM for Windows 11 rounded corners on the undecorated window.
pub fn round_corners(handle: &impl raw_window_handle::HasWindowHandle) {
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    let Ok(h) = handle.window_handle() else {
        return;
    };
    if let RawWindowHandle::Win32(w) = h.as_raw() {
        let pref = DWMWCP_ROUND;
        // SAFETY: valid HWND from winit, pointer/size describe a live u32.
        unsafe {
            DwmSetWindowAttribute(
                w.hwnd.get() as _,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                &pref as *const _ as *const _,
                std::mem::size_of_val(&pref) as u32,
            );
        }
    }
}
