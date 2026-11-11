//! Paneles de progreso y de resultado de la corrida.

use super::palette::Palette;
use super::run::{found_noun, RunState};
use super::util::{fmt_seconds, open_in_explorer, open_selecting};
use super::widgets::{panel_title, result_row, RowAction};
use super::App;
use crate::runner;
use eframe::egui;
use std::path::PathBuf;
use std::time::Instant;

impl App {
    /// ─── Sección: Progreso (solo mientras hay un escaneo en curso) ──
    /// Al terminar, el panel desaparece: el resultado (con el tiempo total)
    /// se muestra una sola vez, en `render_results`.
    pub(super) fn render_progress(&self, ui: &mut egui::Ui) {
        let RunState::Running(run) = &self.state else {
            return;
        };
        let palette = Palette::of(ui);
        let done = run.found();
        let noun = run.noun();

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    // No hay total ni ETA: contar antes de escanear obligaría a
                    // recorrer el disco dos veces. Se muestra un contador en
                    // vivo y el tiempo transcurrido.
                    let text = if run.writing() {
                        format!("Generando reportes… ({done} {noun})")
                    } else {
                        format!("Escaneando… {done} {noun}")
                    };
                    ui.label(egui::RichText::new(text).size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let elapsed = fmt_seconds(run.started.elapsed().as_secs_f32());
                        ui.label(
                            egui::RichText::new(format!("⏱  {elapsed}"))
                                .color(palette.muted)
                                .size(12.0),
                        );
                    });
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Pulsa Esc para cancelar.")
                        .color(palette.muted)
                        .size(11.0),
                );
            });
    }

    /// ─── Sección: Resultado de la corrida ────────────────────────
    pub(super) fn render_results(&mut self, ui: &mut egui::Ui) {
        let palette = Palette::of(ui);
        let mut action: Option<(RowAction, PathBuf)> = None;
        let mut open_folder: Option<PathBuf> = None;

        let panel = match &self.state {
            RunState::Done {
                summary: s,
                elapsed,
            } => {
                let elapsed = fmt_seconds(elapsed.as_secs_f32());
                let copied = self.copied.as_ref().map(|(p, _)| p);
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        panel_title(
                            ui,
                            palette.success,
                            "✅",
                            "Reportes generados correctamente",
                        );
                        ui.add_space(6.0);
                        let counts = if s.dirs_only {
                            format!("📊 {} carpetas · ⏱ {elapsed}", s.dirs_count)
                        } else {
                            format!(
                                "📊 {} archivos · {} · ⏱ {elapsed}",
                                s.files_count,
                                runner::format_size(s.total_bytes)
                            )
                        };
                        ui.label(counts);
                        if s.skipped_count > 0 {
                            ui.colored_label(
                                palette.warning,
                                format!(
                                    "⚠ {} elementos no accesibles fueron omitidos",
                                    s.skipped_count
                                ),
                            );
                        }
                        ui.add_space(8.0);

                        for (format, path) in &s.reports {
                            let is_copied = copied == Some(path);
                            if let Some(a) = result_row(ui, format.label(), path, is_copied) {
                                action = Some((a, path.clone()));
                            }
                        }

                        ui.add_space(10.0);
                        // Abre la carpeta con el primer reporte resaltado
                        // (siempre hay al menos uno: `runner::run` exige un
                        // formato).
                        let first = s.reports.first().map(|(_, p)| p.clone());
                        if ui
                            .add_enabled(
                                first.is_some(),
                                egui::Button::new(
                                    egui::RichText::new("📂  Abrir carpeta de salida").size(12.0),
                                ),
                            )
                            .clicked()
                        {
                            open_folder = first;
                        }
                    })
                    .response
            }
            RunState::Failed { error } => {
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12))
                    .fill(palette.error_fill)
                    .show(ui, |ui| {
                        panel_title(ui, palette.error, "❌", "Ha ocurrido un error");
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(error).monospace().size(12.0));
                    })
                    .response
            }
            RunState::Cancelled {
                found,
                dirs_only,
                elapsed,
            } => {
                // Neutral, no rojo: cancelar no es un error del programa,
                // es una acción deliberada del usuario.
                let noun = found_noun(*dirs_only);
                let elapsed = fmt_seconds(elapsed.as_secs_f32());
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        panel_title(ui, palette.warning, "⏹", "Escaneo cancelado");
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "Se detuvo tras {found} {noun} (⏱ {elapsed}). \
                                 No se generaron reportes.",
                            ))
                            .color(palette.muted)
                            .size(12.0),
                        );
                    })
                    .response
            }
            RunState::Idle | RunState::Running(_) => return,
        };

        // El panel puede quedar por debajo del borde de la ventana: se
        // desplaza hasta él una sola vez, al llegar el resultado.
        if self.scroll_to_results {
            panel.scroll_to_me(Some(egui::Align::BOTTOM));
            self.scroll_to_results = false;
        }

        let open_result = match (action, open_folder) {
            (Some((RowAction::Copy, path)), _) => {
                ui.ctx().copy_text(path.display().to_string());
                self.copied = Some((path, Instant::now()));
                None
            }
            (Some((RowAction::Open, path)), _) => Some(
                open_in_explorer(&path).map_err(|e| format!("No se pudo abrir el archivo: {e}")),
            ),
            // /select, abre el Explorador en la carpeta contenedora con el
            // primer reporte ya resaltado.
            (None, Some(path)) => {
                Some(open_selecting(&path).map_err(|e| format!("No se pudo abrir la carpeta: {e}")))
            }
            (None, None) => None,
        };
        if let Some(result) = open_result {
            // Una acción puntual exitosa limpia cualquier ui_error pendiente
            // de una acción anterior, no solo lo reemplaza si falla de nuevo.
            match result.err() {
                Some(e) => self.set_ui_error(e),
                None => self.clear_ui_error(),
            }
        }
    }
}
