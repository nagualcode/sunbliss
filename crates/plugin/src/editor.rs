//! Sunbliss editor (egui). Original design: flat dark device panels, one
//! orange accent, compact arc knobs and rectangular toggles, laid out
//! left-to-right in signal-flow order like a device chain.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};
use truce::prelude64::*;
use truce_core::editor::PluginContext;
use truce_egui::{EditorUi, EguiEditor};

use crate::{SunblissParams, SunblissParamsParamId as P};

const W: u32 = 960;
const H: u32 = 400;

mod col {
    use egui::Color32;
    pub const BG: Color32 = Color32::from_rgb(28, 28, 28);
    pub const PANEL: Color32 = Color32::from_rgb(44, 44, 44);
    pub const WELL: Color32 = Color32::from_rgb(22, 22, 22);
    pub const LINE: Color32 = Color32::from_rgb(68, 68, 68);
    pub const BTN: Color32 = Color32::from_rgb(62, 62, 62);
    pub const TEXT: Color32 = Color32::from_rgb(214, 214, 214);
    pub const DIM: Color32 = Color32::from_rgb(128, 128, 128);
    pub const ACCENT: Color32 = Color32::from_rgb(255, 150, 46);
    pub const ON_TEXT: Color32 = Color32::from_rgb(24, 24, 24);
    pub const BLUE: Color32 = Color32::from_rgb(108, 182, 255);
    pub const GREEN: Color32 = Color32::from_rgb(120, 214, 120);
    pub const RED: Color32 = Color32::from_rgb(242, 84, 74);
}

struct Info {
    default_norm: f64,
    steps: Option<u32>,
}

pub struct SunUi {
    infos: HashMap<u32, Info>,
    order: Vec<u32>,
    hist: VecDeque<[f32; 3]>,
}

pub fn create(params: Arc<SunblissParams>) -> Box<dyn Editor> {
    let ui = SunUi { infos: HashMap::new(), order: Vec::new(), hist: VecDeque::with_capacity(1024) };
    Box::new(EguiEditor::with_ui(params, (W, H), ui).with_visuals(visuals()))
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = col::BG;
    v.window_fill = col::BG;
    v.override_text_color = Some(col::TEXT);
    v
}

type Ctx = PluginContext<SunblissParams>;

impl SunUi {
    fn ensure_infos(&mut self, ctx: &Ctx) {
        if !self.infos.is_empty() {
            return;
        }
        for i in ctx.params().param_infos() {
            self.order.push(i.id);
            self.infos.insert(i.id, Info { default_norm: i.range.normalize(i.default_plain), steps: i.range.step_count().map(|s| s.get()) });
        }
    }

    fn set(&self, ctx: &Ctx, id: impl Into<u32>, norm: f64) {
        let id = id.into();
        ctx.begin_edit(id);
        ctx.set_param(id, norm.clamp(0.0, 1.0));
        ctx.end_edit(id);
    }

    fn on(&self, ctx: &Ctx, id: impl Into<u32>) -> bool {
        ctx.get_param(id.into()) > 0.5
    }

    fn idx(&self, ctx: &Ctx, id: impl Into<u32>) -> usize {
        let id = id.into();
        let n = self.infos.get(&id).and_then(|i| i.steps).unwrap_or(1) as f64;
        (ctx.get_param(id) * n).round() as usize
    }

    // ------------------------------------------------------------ widgets

    fn knob(&self, ui: &mut egui::Ui, ctx: &Ctx, id: P, label: &str, d: f32, enabled: bool, value_text: Option<String>) {
        let id: u32 = id.into();
        let (rect, resp) = ui.allocate_exact_size(vec2(d + 14.0, d + 34.0), if enabled { Sense::click_and_drag() } else { Sense::hover() });
        let mut v = ctx.get_param(id);
        if enabled {
            if resp.drag_started() {
                ctx.begin_edit(id);
            }
            if resp.dragged() {
                let fine = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                let dv = -resp.drag_delta().y as f64 / if fine { 900.0 } else { 180.0 };
                v = (v + dv).clamp(0.0, 1.0);
                ctx.set_param(id, v);
            }
            if resp.drag_stopped() {
                ctx.end_edit(id);
            }
            if resp.double_clicked() {
                if let Some(i) = self.infos.get(&id) {
                    self.set(ctx, id, i.default_norm);
                }
            }
        }
        let p = ui.painter_at(rect.expand2(vec2(24.0, 2.0)));
        let c = pos2(rect.center().x, rect.top() + 14.0 + d * 0.5);
        let r = d * 0.5;
        p.text(pos2(rect.center().x, rect.top()), Align2::CENTER_TOP, label, FontId::proportional(10.5), col::DIM);
        let start = std::f32::consts::PI * 0.75;
        let sweep = std::f32::consts::PI * 1.5;
        let arc = |a0: f32, a1: f32, rad: f32| -> Vec<Pos2> {
            let n = 48;
            (0..=n).map(|k| {
                let a = a0 + (a1 - a0) * k as f32 / n as f32;
                c + vec2(a.cos(), a.sin()) * rad
            }).collect()
        };
        p.add(egui::Shape::line(arc(start, start + sweep, r), Stroke::new(3.0_f32, col::LINE)));
        let bipolar = self.infos.get(&id).map(|i| (i.default_norm - 0.5).abs() < 1e-6).unwrap_or(false);
        let a_from = if bipolar { start + sweep * 0.5 } else { start };
        let a_to = start + sweep * v as f32;
        let accent = if enabled { col::ACCENT } else { col::DIM };
        if (a_to - a_from).abs() > 0.01 {
            let (lo, hi) = if a_to > a_from { (a_from, a_to) } else { (a_to, a_from) };
            p.add(egui::Shape::line(arc(lo, hi, r), Stroke::new(3.0_f32, accent)));
        }
        p.circle_filled(c, r - 6.0, if resp.hovered() || resp.dragged() { col::BTN } else { col::PANEL });
        let tip = c + vec2(a_to.cos(), a_to.sin()) * (r - 7.0);
        p.line_segment([c + vec2(a_to.cos(), a_to.sin()) * (r * 0.25), tip], Stroke::new(2.0_f32, if enabled { col::TEXT } else { col::DIM }));
        let txt = value_text.unwrap_or_else(|| ctx.format_param(id));
        p.text(pos2(rect.center().x, rect.bottom() - 2.0), Align2::CENTER_BOTTOM, txt, FontId::monospace(10.5), if enabled { col::TEXT } else { col::DIM });
    }

    fn button(&self, ui: &mut egui::Ui, text: &str, w: f32, on: bool, accent: Color32) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 18.0), Sense::click());
        let p = ui.painter();
        let fill = if on { accent } else if resp.hovered() { Color32::from_rgb(74, 74, 74) } else { col::BTN };
        p.rect_filled(rect, CornerRadius::same(2), fill);
        p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(10.5), if on { col::ON_TEXT } else { col::TEXT });
        resp
    }

    fn toggle(&self, ui: &mut egui::Ui, ctx: &Ctx, id: P, text: &str, w: f32) {
        let on = self.on(ctx, id);
        if self.button(ui, text, w, on, col::ACCENT).clicked() {
            self.set(ctx, id, if on { 0.0 } else { 1.0 });
        }
    }

    fn seg(&self, ui: &mut egui::Ui, ctx: &Ctx, id: P, labels: &[&str], w: f32, enabled: bool) {
        let cur = self.idx(ctx, id);
        let n = labels.len();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            for (k, l) in labels.iter().enumerate() {
                let accent = if enabled { col::ACCENT } else { col::DIM };
                if self.button(ui, l, w, k == cur, accent).clicked() {
                    self.set(ctx, id, k as f64 / (n - 1).max(1) as f64);
                }
            }
        });
    }

    fn section<R>(&self, ui: &mut egui::Ui, title: &str, w: f32, h: f32, power: Option<(&Ctx, P)>, body: impl FnOnce(&mut egui::Ui) -> R) {
        let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(3), col::PANEL);
        let mut title_rect = rect;
        title_rect.set_height(20.0);
        ui.painter().text(title_rect.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, title, FontId::proportional(11.0), col::TEXT);
        if let Some((ctx, id)) = power {
            let on = self.on(ctx, id);
            let b = Rect::from_center_size(title_rect.right_center() - vec2(14.0, 0.0), vec2(14.0, 12.0));
            let resp = ui.interact(b, ui.id().with((title, "pwr")), Sense::click());
            ui.painter().rect_filled(b, CornerRadius::same(2), if on { col::ACCENT } else { col::BTN });
            if resp.clicked() {
                self.set(ctx, id, if on { 0.0 } else { 1.0 });
            }
        }
        let inner = Rect::from_min_max(rect.min + vec2(6.0, 22.0), rect.max - vec2(6.0, 6.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Center)));
        child.spacing_mut().item_spacing = vec2(4.0, 5.0);
        body(&mut child);
    }

    // ------------------------------------------------------------ A/B

    fn snapshot(&self, ctx: &Ctx) -> Vec<f64> {
        self.order.iter().map(|&id| ctx.get_param(id)).collect()
    }

    fn recall(&self, ctx: &Ctx, snap: &[f64]) {
        if snap.len() != self.order.len() {
            return;
        }
        for (&id, &v) in self.order.iter().zip(snap) {
            if (ctx.get_param(id) - v).abs() > 1e-9 {
                self.set(ctx, id, v);
            }
        }
    }

    fn ab_bar(&self, ui: &mut egui::Ui, ctx: &Ctx) {
        let active = ctx.params().ab.read().map(|s| s.active).unwrap_or(0);
        let alt = ui.input(|i| i.modifiers.alt);
        for (k, name) in [(1u8, "A"), (2u8, "B")] {
            let lit = active == 0 || active == k;
            let r = self.button(ui, name, 22.0, lit, col::BLUE).on_hover_text("Click: switch / unlink.  Alt-click: copy current to the other slot.");
            if r.clicked() {
                let cur = self.snapshot(ctx);
                let mut st = ctx.params().ab.write().unwrap();
                if alt {
                    if k == 1 { st.b = cur } else { st.a = cur }
                    continue;
                }
                match (st.active, k) {
                    (0, _) => {
                        st.a = cur.clone();
                        st.b = cur;
                        st.active = k;
                    }
                    (a, k) if a == k => {}
                    (_, 1) => {
                        st.b = cur;
                        st.active = 1;
                        let s = st.a.clone();
                        drop(st);
                        self.recall(ctx, &s);
                    }
                    (_, _) => {
                        st.a = cur;
                        st.active = 2;
                        let s = st.b.clone();
                        drop(st);
                        self.recall(ctx, &s);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ display

    fn meter(&self, ui: &mut egui::Ui, ctx: &Ctx, ids: [P; 2], h: f32, ceiling_db: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(22.0, h), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(2), col::WELL);
        let to_y = |db: f32| rect.bottom() - (((db + 48.0) / 48.0).clamp(0.0, 1.0)) * rect.height();
        for (k, id) in ids.iter().enumerate() {
            let v = ctx.get_meter(*id).max(1e-6);
            let db = 20.0 * v.log10();
            let x0 = rect.left() + 2.0 + k as f32 * 10.0;
            let bar = Rect::from_min_max(pos2(x0, to_y(db)), pos2(x0 + 8.0, rect.bottom()));
            p.rect_filled(bar, CornerRadius::ZERO, if db > -0.01 { col::RED } else { col::GREEN });
        }
        let y = to_y(ceiling_db);
        p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, col::ACCENT));
    }

    fn waveform(&mut self, ui: &mut egui::Ui, ctx: &Ctx, w: f32, h: f32, ceiling_db: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, CornerRadius::same(2), col::WELL);
        let pin = ctx.get_meter(P::MInL).max(ctx.get_meter(P::MInR));
        let pout = ctx.get_meter(P::MOutL).max(ctx.get_meter(P::MOutR));
        let clip = ctx.get_meter(P::MClip);
        self.hist.push_back([pin, pout, clip]);
        while self.hist.len() > w as usize / 2 {
            self.hist.pop_front();
        }
        // scale: 0..-24 dB mapped from centre line to edge (log display)
        let mid = rect.center().y;
        let half = h * 0.5 - 4.0;
        let amp = |v: f32| (((20.0 * v.max(1e-6).log10()) + 24.0) / 24.0).clamp(0.0, 1.0) * half;
        let n = self.hist.len();
        for (k, s) in self.hist.iter().enumerate() {
            let x = rect.right() - (n - k) as f32 * 2.0;
            let ai = amp(s[0] * 10f32.powf(s[2] / 20.0).max(1.0));
            let ao = amp(s[1]);
            p.line_segment([pos2(x, mid - ai), pos2(x, mid + ai)], Stroke::new(2.0_f32, Color32::from_rgb(80, 80, 80)));
            p.line_segment([pos2(x, mid - ao), pos2(x, mid + ao)], Stroke::new(2.0_f32, if s[2] > 0.0 { col::ACCENT } else { col::BLUE }));
        }
        let yc = amp(10f32.powf(ceiling_db / 20.0));
        for y in [mid - yc, mid + yc] {
            p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, col::ACCENT.gamma_multiply(0.6)));
        }
        p.text(rect.left_top() + vec2(6.0, 4.0), Align2::LEFT_TOP, "grey: input   blue: output   orange: clipping", FontId::proportional(9.5), col::DIM);
    }
}

fn mmss(s: f32) -> String {
    if s < 0.0 {
        return "--:--".into();
    }
    let t = s as u32;
    format!("{:02}:{:02}", t / 60, t % 60)
}

impl EditorUi<SunblissParams> for SunUi {
    fn opened(&mut self, ctx: &Ctx) {
        self.ensure_infos(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &Ctx) {
        self.ensure_infos(ctx);
        ui.ctx().request_repaint();
        let full = ui.max_rect();
        ui.painter().rect_filled(full, CornerRadius::ZERO, col::BG);
        let ceiling = ctx.params().ceiling.value() as f32;

        // ---------------------------------------------------- header
        let header = Rect::from_min_size(full.min, vec2(full.width(), 30.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(header.shrink2(vec2(10.0, 6.0))), |ui| {
            ui.horizontal_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 6.0, col::ACCENT);
                for k in 0..8 {
                    let a = k as f32 * std::f32::consts::PI / 4.0;
                    ui.painter().line_segment([r.center() + vec2(a.cos(), a.sin()) * 7.5, r.center() + vec2(a.cos(), a.sin()) * 9.5], Stroke::new(1.2_f32, col::ACCENT));
                }
                ui.label(egui::RichText::new("SUNBLISS").strong().size(13.0).color(col::TEXT));
                ui.add_space(20.0);
                self.ab_bar(ui, ctx);
                ui.add_space(14.0);
                self.toggle(ui, ctx, P::Bypass, "Bypass", 52.0);
                ui.add_space(14.0);
                ui.label(egui::RichText::new("Quality").size(10.5).color(col::DIM));
                self.seg(ui, ctx, P::Quality, &["High", "Pristine", "Extra"], 52.0, true);
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Render").size(10.5).color(col::DIM));
                self.seg(ui, ctx, P::RenderQuality, &["Same", "High", "Prist.", "Extra"], 40.0, true);
            });
        });

        // ---------------------------------------------------- display
        let disp = Rect::from_min_size(full.min + vec2(10.0, 36.0), vec2(full.width() - 20.0, 118.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(disp), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                self.meter(ui, ctx, [P::MInL, P::MInR], disp.height(), ceiling);
                let w = disp.width() - 2.0 * 22.0 - 16.0 - 150.0 - 8.0;
                self.waveform(ui, ctx, w, disp.height(), ceiling);
                // readouts
                let (r, resp) = ui.allocate_exact_size(vec2(150.0, disp.height()), Sense::click());
                let p = ui.painter_at(r);
                p.rect_filled(r, CornerRadius::same(2), col::WELL);
                let line = |p: &egui::Painter, k: usize, a: &str, b: String, c: Color32| {
                    let y = r.top() + 10.0 + k as f32 * 21.0;
                    p.text(pos2(r.left() + 8.0, y), Align2::LEFT_TOP, a, FontId::proportional(10.0), col::DIM);
                    p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_TOP, b, FontId::monospace(11.5), c);
                };
                let cmax = ctx.get_meter(P::MClipMax);
                line(&p, 0, "MAX CLIP", format!("{:.2} dB", cmax), if cmax > 0.0 { col::ACCENT } else { col::TEXT });
                line(&p, 1, "AT", mmss(ctx.get_meter(P::MClipPos)), col::TEXT);
                line(&p, 2, "IN  LUFS-S", format!("{:.1}", ctx.get_meter(P::MLufsIn)), col::TEXT);
                line(&p, 3, "OUT LUFS-S", format!("{:.1}", ctx.get_meter(P::MLufsOut)), col::TEXT);
                line(&p, 4, "GUARD", format!("{:.2} dB", ctx.get_meter(P::MGuard)), col::TEXT);
                if resp.double_clicked() {
                    ctx.params().clip_reset.fetch_add(1);
                }
                resp.on_hover_text("Double-click to reset the clip meter");
                self.meter(ui, ctx, [P::MOutL, P::MOutR], disp.height(), ceiling);
            });
        });

        // ---------------------------------------------------- device chain
        let chain = Rect::from_min_max(full.min + vec2(10.0, 162.0), full.max - vec2(10.0, 10.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(chain), |ui| {
            let ch = chain.height();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let linked = self.on(ctx, P::Link);
                self.section(ui, "Input", 112.0, ch, None, |ui| {
                    self.knob(ui, ctx, P::Input, "Trim", 44.0, true, None);
                    self.toggle(ui, ctx, P::Link, "Link I/O", 90.0);
                    self.knob(ui, ctx, P::LinkOffset, "Offset", 30.0, linked, None);
                });
                self.section(ui, "Tone", 82.0, ch, None, |ui| {
                    ui.add_space(6.0);
                    for (k, l) in ["Flat", "A", "B"].iter().enumerate() {
                        let on = self.idx(ctx, P::Tone) == k;
                        if self.button(ui, l, 64.0, on, col::ACCENT).clicked() {
                            self.set(ctx, P::Tone, k as f64 / 2.0);
                        }
                    }
                });
                self.section(ui, "Clipper", 214.0, ch, Some((ctx, P::ClipOn)), |ui| {
                    let en = self.on(ctx, P::ClipOn);
                    self.seg(ui, ctx, P::ClipMode, &["Conv A", "Conv B", "Hard"], 64.0, en);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        self.knob(ui, ctx, P::Ceiling, "Ceiling", 50.0, en, None);
                        ui.vertical(|ui| {
                            ui.add_space(14.0);
                            self.toggle(ui, ctx, P::ClipFilter, "Filter", 86.0);
                            self.toggle(ui, ctx, P::Guard, "TP Guard", 86.0);
                        });
                    });
                });
                self.section(ui, "Saturation", 136.0, ch, Some((ctx, P::SatOn)), |ui| {
                    let en = self.on(ctx, P::SatOn);
                    self.knob(ui, ctx, P::Sat, "Amount", 56.0, en, None);
                    self.seg(ui, ctx, P::Curve, &["A", "B"], 56.0, en);
                    self.toggle(ui, ctx, P::Unity, "Unity", 113.0);
                });
                self.section(ui, "Contour", 92.0, ch, Some((ctx, P::ContourOn)), |ui| {
                    let en = self.on(ctx, P::ContourOn);
                    self.knob(ui, ctx, P::Contour, "Amount", 48.0, en, None);
                });
                let mlink = self.on(ctx, P::MixLink);
                self.section(ui, "Mix", 160.0, ch, None, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        self.knob(ui, ctx, P::Wet, "Wet", 40.0, true, None);
                        let dry_txt = if mlink { Some(format!("{:.0} %", 100.0 * (1.0 - ctx.params().wet.value()))) } else { None };
                        self.knob(ui, ctx, P::Dry, "Dry", 40.0, !mlink, dry_txt);
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        self.toggle(ui, ctx, P::MixLink, "Link", 44.0);
                        self.toggle(ui, ctx, P::WetMute, "M Wet", 44.0);
                        self.toggle(ui, ctx, P::DryMute, "M Dry", 44.0);
                    });
                    self.toggle(ui, ctx, P::Delta, "Delta", 138.0);
                });
                let rest = ui.available_width();
                self.section(ui, "Output", rest, ch, None, |ui| {
                    let txt = if linked {
                        let v = -ctx.params().input.value() + ctx.params().link_offset.value();
                        Some(format!("{v:+.2} dB"))
                    } else {
                        None
                    };
                    self.knob(ui, ctx, P::Output, if linked { "Trim (linked)" } else { "Trim" }, 44.0, !linked, txt);
                });
            });
        });
    }
}

