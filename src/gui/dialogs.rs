//! Arrastrar y soltar (zona para soltar mientras no hay carpeta y aviso al
//! arrastrar sobre la ventana) y el diálogo que aparece al volver a generar.

use super::palette::Palette;
use super::util::{dropped_path_to_input_dir, truncate_path_display};
use super::widgets::panel_title;
use super::App;
use crate::runner::ReportFormat;
use eframe::egui;

impl App {
    /// Arrastrar una carpeta a la ventana rellena "Carpeta a escanear"
    /// (el gesto más natural para esta app). Se ignora mientras hay un
    /// escaneo en curso, igual que el resto del formulario.
    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        if self.is_busy() {
            return;
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.iter().map(|f| f.path().to_path_buf()).next() {
            self.input_path = dropped_path_to_input_dir(path).display().to_string();
        }
    }

    /// Overlay mientras se arrastra un archivo/carpeta sobre la ventana, para
    /// que quede claro que soltarlo ahí lo usa como carpeta a escanear.
    pub(super) fn render_drop_hint(&self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering || self.is_busy() {
            return;
        }
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("drop_overlay"),
        ));
        painter.rect_filled(
            screen,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
        );
        painter.text(
            screen.center(),
            egui::Align2::CENTER_CENTER,
            "Suelta aquí la carpeta para escanearla",
            egui::FontId::proportional(24.0),
            egui::Color32::WHITE,
        );
    }

    /// Zona para soltar, bajo el formulario, mientras no hay carpeta elegida:
    /// hace visible que se puede arrastrar una carpeta, y un clic abre el
    /// selector como "Examinar…".
    pub(super) fn render_drop_zone(&mut self, ui: &mut egui::Ui) {
        if !self.input_path.trim().is_empty() || self.is_busy() {
            return;
        }
        let palette = Palette::of(ui);
        ui.add_space(8.0);
        let size = egui::vec2(ui.available_width(), 150.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let stroke_color = if response.hovered() {
            palette.accent
        } else {
            palette.muted.gamma_multiply(0.6)
        };
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ];
        let painter = ui.painter();
        painter.extend(egui::Shape::dashed_line(
            &corners,
            egui::Stroke::new(1.5, stroke_color),
            8.0,
            6.0,
        ));
        painter.text(
            rect.center() - egui::vec2(0.0, 12.0),
            egui::Align2::CENTER_CENTER,
            "Arrastra aquí la carpeta que quieres inventariar",
            egui::FontId::proportional(16.0),
            ui.visuals().text_color(),
        );
        painter.text(
            rect.center() + egui::vec2(0.0, 14.0),
            egui::Align2::CENTER_CENTER,
            "o haz clic para elegirla",
            egui::FontId::proportional(12.0),
            palette.muted,
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                self.input_path = path.display().to_string();
            }
        }
    }

    /// ─── Diálogo: confirmar sobrescritura de reportes existentes ──
    pub(super) fn render_overwrite_confirm(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending_confirm else {
            return;
        };

        let mut confirmed = false;
        let mut rename = false;
        let mut cancelled = false;
        let mut dont_ask = self.dont_ask_checked;

        // Modal (como la ventana de resultado): bloquea el formulario por
        // debajo y se cierra con Esc o con un clic fuera, como "Cancelar".
        let width = (ctx.content_rect().width() - 80.0).clamp(280.0, 560.0);
        let response = egui::Modal::new(egui::Id::unique("overwrite_modal")).show(ctx, |ui| {
            ui.set_width(width);
            let palette = Palette::of(ui);
            panel_title(
                ui,
                palette.accent,
                "↻",
                "Ya hay reportes de una ejecución anterior",
            );
            ui.add_space(6.0);
            if let Some(dir) = pending.existing.first().and_then(|p| p.parent()) {
                let dir = dir.display().to_string();
                ui.label(
                    egui::RichText::new(format!("En {}", truncate_path_display(&dir, 70)))
                        .color(palette.muted)
                        .size(11.0),
                )
                .on_hover_text(dir);
            }
            let names: Vec<String> = pending
                .existing
                .iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            ui.label(
                egui::RichText::new(names.join("  ·  "))
                    .monospace()
                    .size(12.0),
            );
            ui.add_space(8.0);
            ui.label(
                "Al actualizarlos se reemplazan por los nuevos. Si prefieres \
                 conservarlos, guarda los nuevos con otro nombre:",
            );
            ui.label(
                egui::RichText::new(alt_names(&pending.alt_name, &pending.cfg.formats))
                    .monospace()
                    .size(11.0)
                    .color(palette.muted),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Actualizar reportes")
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(palette.accent),
                    )
                    .on_hover_text("Reemplaza los reportes anteriores por los nuevos")
                    .clicked()
                {
                    confirmed = true;
                }
                if ui
                    .button("Guardar con otro nombre")
                    .on_hover_text("Conserva los reportes anteriores")
                    .clicked()
                {
                    rename = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancelar").clicked() {
                        cancelled = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.checkbox(&mut dont_ask, "No volver a preguntar")
                .on_hover_text(
                    "Al actualizar, los reportes se reemplazarán sin preguntar. \
                     Se puede volver a activar en Opciones.",
                );
        });
        if response.should_close() {
            cancelled = true;
        }

        self.dont_ask_checked = dont_ask;
        // "No volver a preguntar" solo cuenta al actualizar: marcarla y
        // cancelar, o guardar con otro nombre, no cambia la preferencia.
        if confirmed && dont_ask {
            self.ask_before_overwrite = false;
        }
        if confirmed || rename || cancelled {
            self.dont_ask_checked = false;
        }
        if confirmed || rename {
            if let Some(mut pending) = self.pending_confirm.take() {
                if rename {
                    pending.cfg.report_name = pending.alt_name;
                }
                self.start_run(pending.cfg);
            }
        } else if cancelled {
            self.pending_confirm = None;
        }
    }
}

/// Archivos que se crearían con el nombre alternativo, en el orden de
/// siempre: `report-docs-2026-11-12.csv  ·  report-docs-2026-11-12.md`.
fn alt_names(base: &str, formats: &[ReportFormat]) -> String {
    ReportFormat::ALL
        .into_iter()
        .filter(|f| formats.contains(f))
        .map(|f| f.file_name(base))
        .collect::<Vec<_>>()
        .join("  ·  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_names_lists_the_files_in_the_usual_order() {
        assert_eq!(
            alt_names(
                "report-docs-2026-11-12",
                &[ReportFormat::Json, ReportFormat::Csv]
            ),
            "report-docs-2026-11-12.csv  ·  report-docs-2026-11-12.json"
        );
    }
}
