//! The egui GUI: a group dropdown, an instrument dropdown, and a status line.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use nice_plug::context::gui::GuiContext;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug_egui::NiceEguiApp;

use crate::library::{Library, lib_root};
use crate::params::OgziParams;

/// The window size of the GUI, in logical pixels.
pub(crate) const WINDOW_SIZE: LogicalSize<f32> = LogicalSize::new(360.0, 130.0);

/// State that only exists while the editor window is open.
struct OpenEditorState {
    nice_gui_ctx: GuiContext,
}

/// The egui app. This persists across editor openings.
pub struct OgziEditor {
    open: Option<OpenEditorState>,
    params: Arc<OgziParams>,
    lib: Arc<Library>,
    loading: Arc<std::sync::atomic::AtomicBool>,
}

impl OgziEditor {
    pub(crate) fn new(
        params: Arc<OgziParams>,
        lib: Arc<Library>,
        loading: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            open: None,
            params,
            lib,
            loading,
        }
    }
}

impl NiceEguiApp for OgziEditor {
    fn build(
        &mut self,
        _egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        self.open = Some(OpenEditorState { nice_gui_ctx });
        Ok(())
    }

    fn editor_closed(&mut self) {
        self.open = None;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(state) = self.open.as_ref() else {
            return;
        };
        let setter = state.nice_gui_ctx.param_setter();

        if self.lib.groups.is_empty() {
            ui.label(format!(
                "Ogzi library not found at {}",
                lib_root().display()
            ));
            return;
        }

        let params = &self.params;
        let mut group_idx = (params.group.value() as usize).min(self.lib.groups.len() - 1);

        ui.horizontal(|ui| {
            ui.label("Group");
            egui::ComboBox::from_id_salt("ogzi-group")
                .selected_text(self.lib.groups[group_idx].name.clone())
                .show_ui(ui, |ui| {
                    for (i, group) in self.lib.groups.iter().enumerate() {
                        ui.selectable_value(&mut group_idx, i, group.name.clone());
                    }
                });
        });

        if group_idx as i32 != params.group.value() {
            setter.begin_set_parameter(&params.group);
            setter.set_parameter(&params.group, group_idx as i32);
            setter.end_set_parameter(&params.group);
        }

        let instruments = &self.lib.groups[group_idx].instruments;
        if instruments.is_empty() {
            ui.label("This group has no instruments");
            return;
        }
        let mut inst_idx = (params.instrument.value() as usize).min(instruments.len() - 1);

        ui.horizontal(|ui| {
            ui.label("Instrument");
            egui::ComboBox::from_id_salt("ogzi-instrument")
                .selected_text(instruments[inst_idx].name.clone())
                .show_ui(ui, |ui| {
                    for (i, inst) in instruments.iter().enumerate() {
                        ui.selectable_value(&mut inst_idx, i, inst.name.clone());
                    }
                });
        });

        // Keep the instrument parameter valid for the newly selected group.
        if params.instrument.value() as usize >= instruments.len() {
            setter.begin_set_parameter(&params.instrument);
            setter.set_parameter(&params.instrument, 0);
            setter.end_set_parameter(&params.instrument);
            inst_idx = 0;
        }

        if inst_idx as i32 != params.instrument.value() && inst_idx < instruments.len() {
            setter.begin_set_parameter(&params.instrument);
            setter.set_parameter(&params.instrument, inst_idx as i32);
            setter.end_set_parameter(&params.instrument);
        }

        ui.add_space(8.0);
        if self.loading.load(Ordering::Relaxed) {
            ui.label(format!("Loading {}…", instruments[inst_idx].name));
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        } else {
            ui.strong(format!(
                "{} — {}",
                self.lib.groups[group_idx].name, instruments[inst_idx].name
            ));
        }
    }
}
