//! The dynamically paged five-card editor for `mxm-fx-delay`.

pub mod binding;
pub mod sections;

use std::collections::HashMap;
use std::sync::Arc;

use egui::Ui;
use mxm_ui::space::SPACE_5;
use mxm_ui::theme::Tokens;
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, create_egui_editor};

use crate::params::MxmFxDelayParams;
use crate::telemetry::Telemetry;

// The quarter-4K budget hugged to the five cards (`plans/plan-editor-standard.md` F1), held below.
const REFERENCE: (u32, u32) = (1287, 412);
// One card wide: the widest computed floor (Character's, its Model cells) and the gutters.
const MINIMUM: (u32, u32) = (392, 360);

pub type MxmFxDelayEditor = nice_plug_egui::EguiEditor<MxmFxDelayApp>;
pub use mxm_preset::PresetUi;

pub fn create(
    params: Arc<MxmFxDelayParams>,
    telemetry: Arc<Telemetry>,
) -> Option<MxmFxDelayEditor> {
    let state = EguiEditorState::from_size(
        nice_plug::editor::dpi::LogicalSize::new(REFERENCE.0, REFERENCE.1),
        1.0,
    );
    create_egui_editor(
        state,
        nice_plug_egui::RepaintNotifier::new(),
        nice_plug_egui::EguiNiceSettings {
            title: crate::NAME.to_owned(),
            resize_hint: ResizeHint {
                size_constraints: nice_plug::editor::SizeConstraints::min_logical_size(
                    nice_plug::editor::dpi::LogicalSize::new(MINIMUM.0 as f32, MINIMUM.1 as f32),
                ),
                ..ResizeHint::RESIZABLE
            },
            ..Default::default()
        },
        MxmFxDelayApp::new(params, telemetry),
    )
}

pub struct MxmFxDelayApp {
    params: Arc<MxmFxDelayParams>,
    telemetry: Arc<Telemetry>,
    gui_context: Option<GuiContext>,
    text_entry: HashMap<&'static str, Option<String>>,
    presets: PresetUi,
    nav: mxm_ui::navigation::State,
}

impl MxmFxDelayApp {
    pub fn new(params: Arc<MxmFxDelayParams>, telemetry: Arc<Telemetry>) -> Self {
        let presets = PresetUi::new(params.as_ref());
        Self {
            params,
            telemetry,
            gui_context: None,
            text_entry: HashMap::new(),
            presets,
            nav: mxm_ui::navigation::State::default(),
        }
    }
}

impl NiceEguiApp for MxmFxDelayApp {
    fn build(
        &mut self,
        context: egui::Context,
        gui_context: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        mxm_ui::theme::apply(&context);
        mxm_ui::typography::apply(&context);
        context.set_theme(mxm_ui::theme::preference());
        self.gui_context = Some(gui_context);
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(context) = self.gui_context.clone() else {
            return;
        };
        panel(
            ui,
            &self.params,
            &self.telemetry,
            &context.param_setter(),
            &mut self.text_entry,
            &mut self.presets,
            &mut self.nav,
        );
    }

    fn editor_closed(&mut self) {
        self.gui_context = None;
    }
}

pub fn panel(
    ui: &mut Ui,
    params: &MxmFxDelayParams,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    presets: &mut PresetUi,
    nav: &mut mxm_ui::navigation::State,
) {
    let tokens = tokens_for(ui);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));
    let levels = telemetry.take_levels();
    let display = sections::Display {
        levels,
        times: telemetry.times(),
        activity: telemetry.activity(),
        held: telemetry.held(),
    };
    let busy = presets.holds_the_keyboard() || text_entry.values().any(Option::is_some);
    mxm_ui::paging::editor::hold(ui.ctx(), busy);
    mxm_ui::navigation::paged(ui.ctx(), nav, busy);

    mxm_ui::AppBar::new(crate::NAME).show_with(
        ui,
        &tokens,
        |ui| mxm_preset::ui::preset_row(ui, &tokens, params, setter, presets),
        |ui| {
            if mxm_ui::shell::level_meter(ui, &tokens, levels[2], telemetry.clipped()) {
                telemetry.clear_clip();
            }
            mxm_ui::shell::zoom_control(ui);
            mxm_ui::shell::editor_theme_control(ui);
        },
    );
    mxm_preset::ui::overlays(ui, &tokens, params, setter, presets);
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tokens.canvas)
                .inner_margin(egui::Margin::same(SPACE_5 as i8)),
        )
        .show(ui, |ui| {
            sections::cards(
                ui,
                &tokens,
                params,
                setter,
                text_entry,
                telemetry.tempo(),
                display,
            );
        });
}

fn tokens_for(ui: &Ui) -> Tokens {
    if ui.visuals().dark_mode {
        mxm_ui::DARK
    } else {
        mxm_ui::LIGHT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_plugin_test::keyboard_checks;
    use mxm_plugin_test::{opening_size, paging_checks};
    const REVEAL: fn(&egui::Context) = |_| {};

    /// Every painted label with the box it was laid out in.
    ///
    /// Card rectangles are not enough on their own: a panel one row wide and entirely off screen
    /// passes a fit test, and so does a label squeezed to seven points and wrapped into a column of
    /// letters. Both shipped in `mxm-bucket-delay` and both were invisible to its fit test, which
    /// is why `plugins/AGENTS.md` requires reading egui's own shape list instead.
    fn painted_boxes(width: f32, height: f32) -> Vec<(String, egui::Rect)> {
        let context = egui::Context::default();
        mxm_ui::theme::apply(&context);
        mxm_ui::typography::apply(&context);
        context.set_theme(egui::ThemePreference::Light);
        context.all_styles_mut(|style| style.animation_time = 0.0);

        let params = MxmFxDelayParams::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };

        let mut boxes = Vec::new();
        for pass in 0..4 {
            let mut output = context.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            if pass == 3 {
                for clipped in &output.shapes {
                    collect_text(&clipped.shape, &mut boxes);
                }
            }
            output.textures_delta.clear();
        }
        boxes
    }

    fn collect_text(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push((
                text.galley.text().to_owned(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, out);
                }
            }
            _ => {}
        }
    }

    /// **No label is squeezed into a column of letters.** A control given too little width does not
    /// disappear — egui wraps its name one character per line and the row still reports a sensible
    /// height, so nothing downstream notices.
    #[test]
    fn no_painted_label_is_wrapped_into_a_column_of_letters() {
        for (text, rect) in painted_boxes(REFERENCE.0 as f32, REFERENCE.1 as f32) {
            if text.chars().count() < 2 {
                continue;
            }
            assert!(
                rect.height() <= 3.0 * rect.width().max(1.0),
                "{text:?} was painted {:.1} wide and {:.1} tall: it has been squeezed into a \
                 column of letters",
                rect.width(),
                rect.height()
            );
        }
    }

    /// Nothing is painted outside the window it opens at. The fit tests measure the cards; this is
    /// what a person actually sees.
    #[test]
    fn nothing_is_painted_outside_the_reference_window() {
        let (width, height) = (REFERENCE.0 as f32, REFERENCE.1 as f32);
        for (text, rect) in painted_boxes(width, height) {
            assert!(
                rect.right() <= width && rect.bottom() <= height && rect.left() >= 0.0,
                "{text:?} was painted at {rect:?}, outside the {width} x {height} window"
            );
        }
    }

    #[test]
    fn the_keyboard_cursor_reaches_and_operates_every_parameter() {
        let params = MxmFxDelayParams::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let ids: Vec<&str> = sections::all_parameters(&params)
            .iter()
            .map(|bound| bound.id)
            .collect();
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        keyboard_checks::the_cursor_reaches_and_operates(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &sections::test_items(),
            keyboard_checks::Coverage::Exactly(&ids),
            &REVEAL,
            &host,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    #[test]
    fn the_window_contract_fits_the_quarter_4k_budget_and_widest_card() {
        assert!(REFERENCE.0 <= 1920 && REFERENCE.1 <= 1080);
        let cards: Vec<_> = sections::test_items()
            .into_iter()
            .map(|item| item.card)
            .collect();
        assert!(MINIMUM.0 as f32 >= mxm_ui::flow::minimum_width(&cards) + 2.0 * SPACE_5);
    }

    #[test]
    fn the_opening_size_is_the_budget_hugged() {
        let params = MxmFxDelayParams::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        opening_size::is_the_budget_hugged(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &REVEAL,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// **The app bar holds in the narrowest window**: its `…` menu whole and nothing drawn over
    /// anything else, from `MINIMUM` up (`opening_size::bar_holds_from_the_minimum`).
    #[test]
    fn the_app_bar_holds_in_the_minimum_window() {
        let params = MxmFxDelayParams::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        opening_size::bar_holds_from_the_minimum(
            egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    #[test]
    fn opening_page_shows_the_complete_five_card_workflow() {
        let params = MxmFxDelayParams::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        let context = egui::Context::default();
        mxm_ui::theme::apply(&context);
        mxm_ui::typography::apply(&context);
        context.all_styles_mut(|style| style.animation_time = 0.0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            )),
            ..Default::default()
        };
        for _ in 0..4 {
            let mut output = context.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            output.textures_delta.clear();
        }
        assert_eq!(paging_checks::all_rects(&context, 5).len(), 5);
    }

    use mxm_plugin_test::tree_checks;

    /// Every page at the opening size, light and dark, for the owner's review of the layout-tree
    /// conversion (plans/plan-layout-tree.md §4.3): `target/layout-tree/mxm-fx-delay/<tag>/`, where
    /// `MXM_PICTURES` names the tag — `before` on the unconverted editor, `after` on the tree.
    ///
    /// `MXM_PICTURES=after cargo test -p mxm-fx-delay --lib tree_pictures -- --ignored`
    #[test]
    #[ignore = "renders through wgpu; run by hand"]
    fn tree_pictures() {
        let tag = std::env::var("MXM_PICTURES").unwrap_or_else(|_| "after".to_owned());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/layout-tree/mxm-fx-delay")
            .join(tag);
        let params = MxmFxDelayParams::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        tree_checks::pictures(
            &|_| {},
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &dir,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// Every card, in every state that changes what it holds or paints, passes the layout tree's
    /// checks (plans/plan-layout-tree.md §4.3, `tree_checks::card`): its computed floor holds its
    /// content with nothing painted outside the card, the floor is exact, the height its tree
    /// states is the height it draws, and every leaf stays in the room it was given.
    ///
    /// The structural-state matrix: nothing here opens, closes or appears, so the trees are the
    /// same in every state, and what varies is what the displays paint over their stated sizes —
    /// the init patch with idle telemetry, and the longest readings the displays can print: Dual's
    /// two eight-second times, both syncs on with no tempo, a held Freeze and full levels; both syncs
    /// against a slow host tempo; and both at 120 bpm, where each synced knob reads its division.
    #[test]
    fn every_card_passes_the_tree_checks_in_every_state() {
        use nice_plug::prelude::Params;
        let floors = sections::test_floors();
        let longest = [
            ("time", 1.0),
            ("sync", 1.0),
            ("ratesync", 1.0),
            ("routing", 0.5),
            ("offset", 0.0),
            ("freeze", 1.0),
            ("model", 1.0),
            ("shape", 1.0),
        ];
        let full = sections::Display {
            levels: [1.0; 3],
            times: [8.0, 8.0],
            activity: 1.0,
            held: true,
        };
        /// A state's name, the parameter values it sets, the host tempo and the telemetry.
        type State<'a> = (
            &'a str,
            &'a [(&'a str, f32)],
            Option<f64>,
            sections::Display,
        );
        let both = [("sync", 1.0), ("ratesync", 1.0)];
        let states: [State<'_>; 4] = [
            ("init", &[], None, sections::Display::default()),
            (
                "Dual at eight seconds, held, no tempo",
                &longest,
                None,
                full,
            ),
            ("Sync at a slow tempo", &both, Some(30.0), full),
            ("Sync at 120 bpm", &both, Some(120.0), full),
        ];
        for (state, values, tempo, display) in states {
            let params = MxmFxDelayParams::default();
            for (id, param, _) in params.param_map() {
                if let Some((_, value)) = values.iter().find(|(wanted, _)| *wanted == id) {
                    // SAFETY: the parameters are this test's own and nothing else reads them.
                    unsafe {
                        let _ = param._internal_set_normalized_value(*value);
                    }
                }
            }
            let host = keyboard_checks::Recorder::default();
            let setter = ParamSetter::new(&host);
            for (index, floor) in floors.iter().enumerate() {
                let mut text = HashMap::new();
                let mut live = sections::Live {
                    params: &params,
                    setter: &setter,
                    text: &mut text,
                    tempo,
                    display,
                };
                tree_checks::card(
                    &|_| {},
                    state,
                    sections::TITLES[index],
                    *floor,
                    &|ui| sections::card(ui, index, &params),
                    &mut |ui, leaf, rect| {
                        sections::paint(ui, &mxm_ui::LIGHT, leaf, rect, &mut live);
                    },
                );
            }
            assert_eq!(host.sets(), 0, "{state}: drawing a card edited a parameter");
        }
    }

    #[test]
    fn every_dynamic_page_fits_and_every_card_is_reachable() {
        let params = MxmFxDelayParams::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        paging_checks::verify(
            &sections::test_items(),
            &[
                egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
                egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            ],
            |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }
}
