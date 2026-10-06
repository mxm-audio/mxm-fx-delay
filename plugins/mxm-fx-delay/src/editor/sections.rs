//! Five Effects cards in the approved signal/workflow order.
//!
//! Each card body is a `mxm_ui::tree` (plans/plan-layout-tree.md): [`card`] describes it once, the
//! paging renderer measures that one description for the card's floor and height, and [`paint`]
//! draws it leaf by leaf through the bindings, so the controls, their gestures and their names are
//! the bindings' own. Nothing is typed and nothing is drawn to learn a size.

use std::collections::HashMap;

use egui::{Ui, Vec2, vec2};
use mxm_ui::control::{Size, Wave};
use mxm_ui::space::SPACE_2;
use mxm_ui::theme::Tokens;
use mxm_ui::tree::{Height, Kind, Node, leaf, pad_all, row_gap, stack, switch_beside_knob};
use mxm_ui::visual::{AXIS_STROKE, CANVAS_RADIUS, EMPHASIS_STROKE, TALL_PLOT_HEIGHT, TRACE_STROKE};
use nice_plug::prelude::ParamSetter;

use super::binding::{Bound, segmented, segmented_waves, toggle_labelled};
use crate::params::{MxmFxDelayParams, RoutingChoice};

const KNOB: Size = Size::Standard;
/// The cards' titles, in paging order.
pub const TITLES: [&str; 5] = ["Time", "Routing", "Feedback", "Character", "Output"];

/// The timeline's stated size: it fills its card's width and is never narrower than this.
pub const TIMELINE_MIN: Vec2 = vec2(180.0, TALL_PLOT_HEIGHT);
/// The routing diagram's stated size; it fills its card's width.
pub const ROUTING_MIN: Vec2 = vec2(180.0, 72.0);
/// The level display's stated size; it fills its card's width.
pub const LEVELS_MIN: Vec2 = vec2(160.0, 72.0);

/// Drawn, not named (design system §7.3), each announcing its option's own name. *Random* glides
/// between its values, so it is the smooth-random picture rather than sample-and-hold steps.
const SHAPES: [Wave; 3] = [Wave::Sine, Wave::Triangle, Wave::SmoothRandom];

#[derive(Clone, Copy, Default)]
pub struct Display {
    pub levels: [f32; 3],
    pub times: [f32; 2],
    pub activity: f32,
    pub held: bool,
}

pub fn all_parameters(params: &MxmFxDelayParams) -> Vec<Bound<'_>> {
    vec![
        Bound::new(
            "time",
            &params.time,
            "The delay time; synced, a note length.",
        ),
        Bound::new("sync", &params.sync, super::binding::SYNC_DESCRIPTION),
        Bound::new(
            "change",
            &params.change,
            "What the echoes do when you change the time.",
        ),
        Bound::new("routing", &params.routing, "How the echoes sit in stereo."),
        Bound::new(
            "offset",
            &params.offset,
            "Spreads the left and right times apart in Dual.",
        )
        .bipolar(),
        Bound::new(
            "feedback",
            &params.feedback,
            "How many times the echoes repeat; at the top they build up on their own.",
        ),
        Bound::new(
            "lowcut",
            &params.low_cut,
            "Removes low end from every repeat.",
        )
        .law(mxm_preset::StepLaw::Hertz),
        Bound::new(
            "highcut",
            &params.high_cut,
            "Removes high end from every repeat.",
        )
        .law(mxm_preset::StepLaw::Hertz),
        Bound::new("drive", &params.drive, "Saturates every repeat."),
        Bound::new(
            "freeze",
            &params.freeze,
            "Holds the current echoes, repeating them forever, and lets no new sound in.",
        ),
        Bound::new("model", &params.model, "The delay's character."),
        Bound::new(
            "character",
            &params.character,
            "From clean and new toward dark and worn.",
        ),
        Bound::new(
            "motion",
            &params.motion,
            "How much the delay time wavers; zero is steady.",
        ),
        Bound::new("rate", &params.rate, "How fast it wavers."),
        Bound::new(
            "ratesync",
            &params.rate_sync,
            super::binding::SYNC_DESCRIPTION,
        ),
        Bound::new("shape", &params.shape, "The shape of the wavering."),
        Bound::new(
            "duck",
            &params.duck,
            "Lowers the echoes while you play, so they fill the gaps.",
        ),
        Bound::new(
            "mix",
            &params.mix,
            "The balance of dry sound and echoes; at zero the effect is off.",
        ),
    ]
}

fn bound<'a>(id: &str, params: &'a MxmFxDelayParams) -> Bound<'a> {
    all_parameters(params)
        .into_iter()
        .find(|bound| bound.id == id)
        .expect("every drawn parameter is bound")
}

pub fn cards(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmFxDelayParams,
    setter: &ParamSetter<'_>,
    text: &mut HashMap<&'static str, Option<String>>,
    tempo: Option<f64>,
    display: Display,
) -> f32 {
    let items = page_items(ui, params);
    let text_editing = text.values().any(Option::is_some);
    let mut live = Live {
        params,
        setter,
        text,
        tempo,
        display,
    };
    let report = mxm_ui::paging::editor::show(
        ui,
        tokens,
        &items,
        &[],
        text_editing,
        &mut |ui, index| card(ui, index, params),
        &mut |ui, _, leaf, rect| paint(ui, tokens, leaf, rect, &mut live),
    );
    report
        .visible
        .iter()
        .map(|(_, rect)| rect.bottom())
        .fold(ui.min_rect().bottom(), f32::max)
}

/// Every paging item, each floor computed from its card's tree in `ui`'s fonts: the tree's
/// narrowest and the card's chrome. There is no usability minimum to add, and each card is as wide
/// as its floor (`plans/plan-editor-standard.md` A1).
pub fn page_items(ui: &Ui, params: &MxmFxDelayParams) -> Vec<mxm_ui::paging::Item<'static>> {
    use mxm_ui::paging::{Category, Item, Key};
    TITLES
        .iter()
        .enumerate()
        .map(|(index, title)| Item {
            key: Key(index as u64),
            card: {
                let floor = mxm_ui::tree::card_floor(ui, title, &card(ui, index, params));
                mxm_ui::flow::Card::new(title, floor).capped(floor)
            },
            category: Category::Effects,
            kind: title,
        })
        .collect()
}

/// The paging items as the editor computes them, from a context set up as an editor's is — three
/// passes in, so the weighted font cuts are bound — for tests, which have no editor `Ui` to hand.
#[cfg(test)]
pub(crate) fn test_items() -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    let params = MxmFxDelayParams::default();
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = page_items(ui, &params);
        });
        output.textures_delta.clear();
    }
    items
}

/// The cards' floors in paging order, as [`test_items`] computes them.
#[cfg(test)]
pub(crate) fn test_floors() -> Vec<f32> {
    test_items().iter().map(|item| item.card.floor).collect()
}

/// What a leaf of this editor's cards draws. Hashed by what it names, which keeps its widget ids
/// stable wherever the tree places it.
#[derive(Clone, Copy, Debug, Hash)]
pub enum Leaf {
    Knob(&'static str),
    Slider(&'static str),
    Segmented(&'static str),
    /// An on/off parameter's toggle, labelled with its name.
    Toggle(&'static str),
    /// A control's tempo sync, the quarter note beside it.
    Picture(&'static str),
    Waves(&'static str),
    Timeline,
    Routing,
    Levels,
}

/// A stepped parameter's painted cells: every option, as its own formatted text (B4), so a host's
/// list and the switch cannot disagree.
fn options(params: &MxmFxDelayParams, id: &str) -> Vec<String> {
    let param = bound(id, params).param;
    let last = param
        .steps()
        .unwrap_or_else(|| panic!("{id} is not a segmented control here"));
    (0..=last)
        .map(|option| param.format(option as f32 / last as f32))
        .collect()
}

/// The ladder of the control `id`, if it has a tempo sync.
fn ladder_of(id: &str) -> Option<mxm_tempo::Ladder> {
    match id {
        "time" => Some(crate::params::TIME_SYNC),
        "rate" => Some(crate::params::RATE_SYNC),
        _ => None,
    }
}

/// **The division a synced knob reads**, or `None` for its free value: sync off, or no tempo
/// (`plans/plan-tempo-sync-controls.md`).
fn shown_division(
    params: &MxmFxDelayParams,
    id: &str,
    tempo: Option<f64>,
) -> Option<mxm_tempo::Division> {
    use nice_plug::prelude::Param as _;
    let (on, param): (bool, &nice_plug::prelude::FloatParam) = match id {
        "time" => (params.sync.value(), &params.time),
        "rate" => (params.rate_sync.value(), &params.rate),
        _ => return None,
    };
    if !on {
        return None;
    }
    let ladder = ladder_of(id)?;
    let (lo, hi) = (
        f64::from(param.preview_plain(0.0)),
        f64::from(param.preview_plain(1.0)),
    );
    ladder.shown(param.unmodulated_normalized_value(), tempo, lo, hi)
}

/// A control's tempo sync, the quarter note on its knob's grid.
fn sync_beside(id: &'static str) -> Node<Leaf> {
    switch_beside_knob(KNOB, leaf(Leaf::Picture(id), Kind::SyncToggle))
}

/// The collection's knob row (`mxm_ui::tree::knob_row`): equal columns at the one knob column. A
/// syncable control's column holds its free readings and its divisions.
fn knobs(ui: &Ui, params: &MxmFxDelayParams, ids: &[&'static str]) -> Node<Leaf> {
    mxm_ui::tree::knob_row(
        ui,
        ids.iter()
            .map(|id| {
                let bound = bound(id, params);
                let param = bound.param;
                let widest = match ladder_of(id) {
                    Some(ladder) => super::binding::synced_widest(param, ladder.span),
                    None => mxm_ui::control::widest_value(|n| param.format(n as f32)),
                };
                let knob = leaf(
                    Leaf::Knob(id),
                    Kind::Knob {
                        name: bound.painted().to_owned(),
                        widest,
                        size: KNOB,
                        // In the collection's knob row, which sizes the columns.
                        column: 0.0,
                    },
                );
                (KNOB, knob)
            })
            .collect(),
    )
}

fn slider(params: &MxmFxDelayParams, id: &'static str) -> Node<Leaf> {
    let bound = bound(id, params);
    let param = bound.param;
    leaf(
        Leaf::Slider(id),
        Kind::Slider {
            label: bound.painted().to_owned(),
            quiet: false,
            widest: mxm_ui::control::widest_value(|n| param.format(n as f32)),
        },
    )
}

/// What each option of a stepped control does, one sentence per cell in the parameter's own order
/// (design system §7.3; the owner, 2026-09-27: the cells of a row do not share one sentence).
fn details_of(id: &str) -> &'static [&'static str] {
    match id {
        "change" => &[
            "Changing the time bends the echoes' pitch, like tape.",
            "Changing the time crossfades smoothly to the new one.",
            "Changing the time jumps straight there, without a click.",
        ],
        "routing" => &[
            "One delay that keeps the source's stereo picture.",
            "Left and right get their own times; Offset spreads them.",
            "Echoes bounce between left and right.",
        ],
        "model" => &[
            "Clear, full-range echoes.",
            "The grainy, darker sound of early digital delays.",
            "Warm, softened echoes that waver like tape.",
        ],
        "shape" => &[
            "A smooth, even wobble.",
            "Sweeps evenly up and down.",
            "Wanders smoothly and unpredictably.",
        ],
        _ => &[],
    }
}

fn segmented_leaf(params: &MxmFxDelayParams, id: &'static str) -> Node<Leaf> {
    leaf(
        Leaf::Segmented(id),
        Kind::Segmented {
            label: bound(id, params).painted().to_owned(),
            options: options(params, id),
            beside: None,
        },
    )
}

/// An on/off parameter's toggle, labelled with its name: the collection draws every on/off this way
/// (design system §7.2), and its word is the parameter's.
fn toggle_leaf(params: &MxmFxDelayParams, id: &'static str) -> Node<Leaf> {
    leaf(
        Leaf::Toggle(id),
        Kind::Toggle {
            label: bound(id, params).painted().to_owned(),
        },
    )
}

/// A display that fills its card's width, at its stated size.
fn display(key: Leaf, min: Vec2) -> Node<Leaf> {
    leaf(
        key,
        Kind::Custom {
            min_width: min.x,
            height: Height::Fixed(min.y),
            fills: true,
        },
    )
}

/// Card `index`'s body, as a tree. Nothing in it follows telemetry: the displays state their
/// sizes, and only their painting reads the live times and levels.
pub fn card(ui: &Ui, index: usize, params: &MxmFxDelayParams) -> Node<Leaf> {
    let gap = ui.spacing().item_spacing.x;
    match index {
        0 => stack(vec![
            // The timeline keeps `SPACE_2` of its own below it, beyond the body's rhythm.
            pad_all(0.0, 0.0, SPACE_2, display(Leaf::Timeline, TIMELINE_MIN)),
            // Time with its tempo sync beside it, then how a commanded change is taken.
            row_gap(gap, vec![knobs(ui, params, &["time"]), sync_beside("sync")]),
            segmented_leaf(params, "change"),
        ]),
        1 => stack(vec![
            display(Leaf::Routing, ROUTING_MIN),
            segmented_leaf(params, "routing"),
            slider(params, "offset"),
        ]),
        2 => stack(vec![
            knobs(ui, params, &["feedback", "drive"]),
            slider(params, "lowcut"),
            slider(params, "highcut"),
            toggle_leaf(params, "freeze"),
        ]),
        3 => stack(vec![
            segmented_leaf(params, "model"),
            // Rate with its tempo sync beside it.
            row_gap(
                gap,
                vec![
                    knobs(ui, params, &["character", "motion", "rate"]),
                    sync_beside("ratesync"),
                ],
            ),
            leaf(
                Leaf::Waves("shape"),
                Kind::Waves {
                    label: Some(bound("shape", params).painted().to_owned()),
                    count: SHAPES.len(),
                    marks: Vec::new(),
                    beside: None,
                },
            ),
        ]),
        _ => stack(vec![
            display(Leaf::Levels, LEVELS_MIN),
            knobs(ui, params, &["duck", "mix"]),
        ]),
    }
}

/// Everything a leaf draws with: the parameters and their host, the text-entry buffers, and the
/// telemetry snapshot taken once before the frame.
pub struct Live<'a, 'b> {
    pub params: &'a MxmFxDelayParams,
    pub setter: &'a ParamSetter<'b>,
    pub text: &'a mut HashMap<&'static str, Option<String>>,
    pub tempo: Option<f64>,
    pub display: Display,
}

/// Draws one leaf, in the `Ui` the tree bounded to its rectangle, through the bindings — so the
/// controls, their gestures and their names are exactly what they were. Every leaf here takes its
/// width from that `Ui`, so the rectangle itself is not needed.
pub fn paint(ui: &mut Ui, tokens: &Tokens, leaf: &Leaf, rect: egui::Rect, live: &mut Live<'_, '_>) {
    let params = live.params;
    match *leaf {
        // At the knob row's column (`tree::knob_row`). Synced, a knob reads the division in force;
        // the host still reads its value.
        Leaf::Knob(id) => {
            let bound = bound(id, params);
            match shown_division(params, id, live.tempo) {
                Some(division) => bound.knob_with_reading(
                    ui,
                    tokens,
                    live.setter,
                    KNOB,
                    rect.width(),
                    live.text,
                    division.label(),
                ),
                None => bound.knob(ui, tokens, live.setter, KNOB, rect.width(), live.text),
            }
        }
        Leaf::Picture(id) => {
            super::binding::sync_picture(ui, tokens, id, bound(id, params).param, live.setter);
        }
        Leaf::Slider(id) => bound(id, params).slider(ui, tokens, live.setter, live.text),
        Leaf::Segmented(id) => {
            let bound = bound(id, params);
            let labels = options(params, id);
            let options: Vec<&str> = labels.iter().map(String::as_str).collect();
            segmented(
                ui,
                tokens,
                id,
                bound.param,
                &options,
                None,
                details_of(id),
                live.setter,
            );
        }
        Leaf::Toggle(id) => {
            let bound = bound(id, params);
            toggle_labelled(
                ui,
                tokens,
                id,
                bound.param,
                bound.painted(),
                bound.description,
                live.setter,
                0.0,
            );
        }
        Leaf::Waves(id) => {
            let bound = bound(id, params);
            let labels = options(params, id);
            let shapes: Vec<(Wave, &str)> = SHAPES
                .into_iter()
                .zip(labels.iter().map(String::as_str))
                .collect();
            segmented_waves(
                ui,
                tokens,
                id,
                bound.param,
                &shapes,
                None,
                details_of(id),
                live.setter,
            );
        }
        Leaf::Timeline => timeline(ui, tokens, params, live.tempo, live.display),
        Leaf::Routing => routing_diagram(ui, tokens, params.routing.value(), live.display.times),
        Leaf::Levels => level_display(ui, tokens, live.display),
    }
}

fn timeline(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmFxDelayParams,
    tempo: Option<f64>,
    display: Display,
) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(TIMELINE_MIN.x), TIMELINE_MIN.y),
        egui::Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Other,
            true,
            "Effective left and right repeat times",
        )
    });
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CANVAS_RADIUS, tokens.surface_2);
    painter.rect_stroke(
        rect,
        CANVAS_RADIUS,
        egui::Stroke::new(AXIS_STROKE, tokens.border),
        egui::StrokeKind::Inside,
    );
    let baseline = rect.center().y;
    painter.line_segment(
        [
            egui::pos2(rect.left() + 12.0, baseline),
            egui::pos2(rect.right() - 12.0, baseline),
        ],
        egui::Stroke::new(AXIS_STROKE, tokens.border_strong),
    );
    for (index, time) in display.times.iter().enumerate() {
        let position = ((time.max(0.005).log10() - 0.005f32.log10())
            / (8.0f32.log10() - 0.005f32.log10()))
        .clamp(0.0, 1.0);
        let x = egui::lerp((rect.left() + 12.0)..=(rect.right() - 12.0), position);
        let offset = if index == 0 { -8.0 } else { 8.0 };
        painter.line_segment(
            [
                egui::pos2(x, baseline - 12.0 + offset),
                egui::pos2(x, baseline + 12.0 + offset),
            ],
            egui::Stroke::new(
                EMPHASIS_STROKE,
                if index == 0 {
                    tokens.accent
                } else {
                    tokens.focus
                },
            ),
        );
    }
    let free = params.time.value();
    let reading = match (params.sync.value(), tempo) {
        (true, Some(bpm)) => format!(
            "{} · {}",
            params.division_at(bpm).label(),
            time_text(params.target_time(Some(bpm)))
        ),
        (true, None) => format!("No tempo · {}", time_text(free)),
        _ => format!("Free · {}", time_text(free)),
    };
    painter.text(
        rect.left_top() + egui::vec2(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        reading,
        mxm_ui::typography::caption_style(ui.style()).resolve(ui.style()),
        tokens.text_secondary,
    );
    if display.held {
        painter.text(
            rect.right_top() + egui::vec2(-8.0, 6.0),
            egui::Align2::RIGHT_TOP,
            "Held",
            mxm_ui::typography::caption_style(ui.style()).resolve(ui.style()),
            tokens.accent,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RouteDiagram {
    destinations: [usize; 2],
    caption: &'static str,
    separate_times: bool,
}

fn route_diagram(routing: RoutingChoice) -> RouteDiagram {
    match routing {
        RoutingChoice::Standard => RouteDiagram {
            destinations: [0, 1],
            caption: "Linked",
            separate_times: false,
        },
        RoutingChoice::Dual => RouteDiagram {
            destinations: [0, 1],
            caption: "Separate",
            separate_times: true,
        },
        RoutingChoice::PingPong => RouteDiagram {
            destinations: [1, 0],
            caption: "Cross-feedback",
            separate_times: false,
        },
    }
}

fn routing_diagram(ui: &mut Ui, tokens: &Tokens, routing: RoutingChoice, times: [f32; 2]) {
    let diagram = route_diagram(routing);
    let time = |seconds: f32| {
        if seconds.is_finite() && seconds > 0.0 {
            time_text(seconds)
        } else {
            "—".to_owned()
        }
    };
    let caption = if diagram.separate_times {
        format!(
            "{} · L {} / R {}",
            diagram.caption,
            time(times[0]),
            time(times[1])
        )
    } else {
        format!("{} · {}", diagram.caption, time(times[0]))
    };
    let accessibility = match routing {
        RoutingChoice::Standard => {
            format!("Standard routing: left stays left and right stays right; {caption}")
        }
        RoutingChoice::Dual => {
            format!("Dual routing: independent left and right delay times; {caption}")
        }
        RoutingChoice::PingPong => {
            format!("Ping-pong routing: feedback alternates left and right; {caption}")
        }
    };

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(ROUTING_MIN.x), ROUTING_MIN.y),
        egui::Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, accessibility.clone())
    });
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CANVAS_RADIUS, tokens.surface_2);
    painter.rect_stroke(
        rect,
        CANVAS_RADIUS,
        egui::Stroke::new(AXIS_STROKE, tokens.border),
        egui::StrokeKind::Inside,
    );

    let font = mxm_ui::typography::caption_style(ui.style()).resolve(ui.style());
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 11.0),
        egui::Align2::CENTER_CENTER,
        caption,
        font.clone(),
        tokens.text_secondary,
    );

    let rows = [rect.top() + 35.0, rect.top() + 56.0];
    let line_start = rect.left() + 42.0;
    let line_end = rect.right() - 58.0;
    for (source, destination) in diagram.destinations.into_iter().enumerate() {
        let color = if source == 0 {
            tokens.accent
        } else {
            tokens.focus
        };
        painter.text(
            egui::pos2(rect.left() + 8.0, rows[source]),
            egui::Align2::LEFT_CENTER,
            if source == 0 { "L in" } else { "R in" },
            font.clone(),
            tokens.text_primary,
        );
        draw_arrow(
            &painter,
            egui::pos2(line_start, rows[source]),
            egui::pos2(line_end, rows[destination]),
            egui::Stroke::new(TRACE_STROKE, color),
        );
    }
    for (channel, y) in rows.into_iter().enumerate() {
        painter.text(
            egui::pos2(rect.right() - 8.0, y),
            egui::Align2::RIGHT_CENTER,
            if channel == 0 { "L repeat" } else { "R repeat" },
            font.clone(),
            tokens.text_primary,
        );
    }
}

fn draw_arrow(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, stroke: egui::Stroke) {
    painter.line_segment([from, to], stroke);
    painter.circle_filled(from, 2.5, stroke.color);
    let delta = to - from;
    let direction = delta / delta.length().max(1.0);
    let normal = egui::vec2(-direction.y, direction.x);
    let back = to - direction * 7.0;
    painter.line_segment([back + normal * 3.5, to], stroke);
    painter.line_segment([back - normal * 3.5, to], stroke);
}

fn level_display(ui: &mut Ui, tokens: &Tokens, display: Display) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(LEVELS_MIN.x), LEVELS_MIN.y),
        egui::Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Other,
            true,
            "Input, wet and output levels",
        )
    });
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CANVAS_RADIUS, tokens.surface_2);
    for (index, (label, level)) in ["In", "Wet", "Out"]
        .into_iter()
        .zip(display.levels)
        .enumerate()
    {
        let y = rect.top() + 12.0 + index as f32 * 19.0;
        painter.text(
            egui::pos2(rect.left() + 8.0, y),
            egui::Align2::LEFT_CENTER,
            label,
            mxm_ui::typography::caption_style(ui.style()).resolve(ui.style()),
            tokens.text_secondary,
        );
        let left = rect.left() + 40.0;
        let right = egui::lerp(left..=(rect.right() - 8.0), level.clamp(0.0, 1.0));
        painter.line_segment(
            [egui::pos2(left, y), egui::pos2(rect.right() - 8.0, y)],
            egui::Stroke::new(AXIS_STROKE, tokens.border),
        );
        painter.line_segment(
            [egui::pos2(left, y), egui::pos2(right, y)],
            egui::Stroke::new(EMPHASIS_STROKE, tokens.accent),
        );
    }
}

fn time_text(seconds: f32) -> String {
    if (seconds * 1000.0).round() >= 1000.0 {
        format!("{seconds:.2} s")
    } else {
        format!("{:.0} ms", seconds * 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::prelude::Params;

    #[test]
    fn every_parameter_is_bound_once() {
        let params = MxmFxDelayParams::default();
        let ids: Vec<_> = all_parameters(&params)
            .iter()
            .map(|bound| bound.id)
            .collect();
        assert_eq!(ids.len(), params.param_map().len());
        for (id, _, _) in params.param_map() {
            assert_eq!(ids.iter().filter(|bound| **bound == id).count(), 1, "{id}");
        }
    }

    #[test]
    fn every_routing_has_an_unambiguous_diagram() {
        let standard = route_diagram(RoutingChoice::Standard);
        let dual = route_diagram(RoutingChoice::Dual);
        let ping_pong = route_diagram(RoutingChoice::PingPong);

        assert_eq!(standard.destinations, [0, 1]);
        assert_eq!(dual.destinations, [0, 1]);
        assert_eq!(ping_pong.destinations, [1, 0]);
        assert!(!standard.separate_times);
        assert!(dual.separate_times);
        assert_ne!(standard.caption, dual.caption);
        assert_ne!(standard.caption, ping_pong.caption);
        assert_ne!(dual.caption, ping_pong.caption);
    }
}
