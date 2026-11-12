//! Panel de progreso, ventana (modal) con el resultado de la corrida y
//! resumen de una línea en la fila de botones.

use super::palette::Palette;
use super::run::RunState;
use super::util::{fmt_seconds, open_in_explorer, open_selecting};
use super::widgets::{panel_title, result_row, RowAction};
use super::{App, Copied};
use crate::models::ExtStat;
use crate::runner::{self, RunSummary};
use eframe::egui;
use std::path::PathBuf;
use std::time::Instant;

impl App {
    /// ─── Sección: Progreso (solo mientras hay un escaneo en curso) ──
    /// Al terminar, el panel desaparece y el resultado (con el tiempo total)
    /// se muestra en el modal de `render_results_modal`.
    pub(super) fn render_progress(&self, ui: &mut egui::Ui) {
        let RunState::Running(run) = &self.state else {
            return;
        };
        let palette = Palette::of(ui);
        let done = run.found();

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    // No hay total ni ETA: contar antes de escanear obligaría a
                    // recorrer el disco dos veces. Se muestra un contador en
                    // vivo y el tiempo transcurrido.
                    let text = if run.writing() {
                        format!("Generando reportes… ({done} {})", run.noun())
                    } else {
                        let status = runner::scan_status(done, run.dirs_scanned(), run.dirs_only);
                        format!("Escaneando… {status}")
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

    /// ─── Resultado de la corrida: resumen en la fila de botones ──
    /// El detalle se muestra en un modal (`render_results_modal`) que se abre
    /// solo al terminar; aquí queda un resumen corto, alineado a la derecha,
    /// con un botón para volver a abrirlo después de cerrarlo.
    pub(super) fn render_results(&mut self, ui: &mut egui::Ui) {
        let palette = Palette::of(ui);
        let (color, icon, text) = match &self.state {
            RunState::Done { summary: s, .. } => {
                let text = if s.dirs_only {
                    format!("{} carpetas", s.dirs_count)
                } else {
                    format!(
                        "{} archivos · {}",
                        s.files_count,
                        runner::format_size(s.total_bytes)
                    )
                };
                (palette.success, "✅", text)
            }
            RunState::Failed { .. } => (palette.error, "❌", "Error".to_string()),
            RunState::Cancelled { .. } => (palette.warning, "⏹", "Cancelado".to_string()),
            RunState::Idle | RunState::Running(_) => return,
        };

        // De derecha a izquierda: botón, texto, icono.
        let mut reopen = false;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            reopen = ui
                .add(egui::Button::new("Ver resultado").min_size(egui::vec2(0.0, 36.0)))
                .clicked();
            ui.label(text);
            ui.colored_label(color, icon);
        });
        if reopen {
            self.results_open = true;
            // El modal ya se dibujó (cerrado) en este cuadro: sin repintar,
            // no aparecería hasta el próximo movimiento del ratón.
            ui.ctx().request_repaint();
        }
    }

    /// `true` si el modal de resultado está abierto y hay algo que mostrar.
    pub(super) fn results_visible(&self) -> bool {
        self.results_open
            && matches!(
                self.state,
                RunState::Done { .. } | RunState::Failed { .. } | RunState::Cancelled { .. }
            )
    }

    /// Modal con el resultado de la última corrida: se abre al terminar y se
    /// cierra con "Cerrar", Enter, Esc o un clic fuera. Bloquea el
    /// formulario mientras está abierto, así que no hace falta desplazarse
    /// hasta el final de la ventana para ver el resultado.
    pub(super) fn render_results_modal(&mut self, ctx: &egui::Context) {
        if !self.results_visible() {
            return;
        }
        let mut action: Option<ResultAction> = None;
        let mut open_folder: Option<PathBuf> = None;
        let mut close = false;

        // Ancho y alto acotados a la ventana; si el contenido no cabe (muchas
        // filas de tipos de archivo, error largo) se desplaza dentro del modal.
        let screen = ctx.content_rect();
        let width = (screen.width() - 80.0).clamp(280.0, 720.0);
        let max_body = (screen.height() - 170.0).max(120.0);

        let response = egui::Modal::new(egui::Id::unique("results_modal")).show(ctx, |ui| {
            ui.set_width(width);
            egui::ScrollArea::vertical()
                .max_height(max_body)
                .auto_shrink([false, true])
                .show(ui, |ui| self.render_result_body(ui, &mut action));
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                // Abre la carpeta con el primer reporte resaltado (siempre
                // hay al menos uno: `runner::run` exige un formato).
                if let RunState::Done { summary, .. } = &self.state {
                    if let Some((_, first)) = summary.reports.first() {
                        if ui.button("📂  Abrir carpeta de salida").clicked() {
                            open_folder = Some(first.clone());
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.button("Cerrar").clicked();
                });
            });
        });
        if close || response.should_close() {
            self.results_open = false;
        }

        let open_result = match (action, open_folder) {
            (Some(ResultAction::Row(RowAction::Copy, path)), _) => {
                ctx.copy_text(path.display().to_string());
                self.copied = Some((Copied::Path(path), Instant::now()));
                None
            }
            (Some(ResultAction::CopySkipped), _) => {
                if let RunState::Done { summary, .. } = &self.state {
                    ctx.copy_text(skipped_list_text(summary));
                    self.copied = Some((Copied::SkippedList, Instant::now()));
                }
                None
            }
            (Some(ResultAction::Row(RowAction::Open, path)), _) => Some(
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

    /// Contenido del modal según el resultado. Las acciones de los botones
    /// se devuelven en `action` para aplicarlas fuera del modal.
    fn render_result_body(&self, ui: &mut egui::Ui, action: &mut Option<ResultAction>) {
        let palette = Palette::of(ui);
        match &self.state {
            RunState::Done {
                summary: s,
                elapsed,
            } => {
                let elapsed = fmt_seconds(elapsed.as_secs_f32());
                let copied = self.copied.as_ref().map(|(c, _)| c);
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
                // Lo que falta en el reporte, a propósito (carpetas omitidas
                // por nombre) o no (sin acceso), para que nadie lo dé por
                // completo sin saberlo.
                if let Some(text) = runner::excluded_dirs_text(&s.excluded_dirs) {
                    ui.label(egui::RichText::new(format!("🚫 {text}")).color(palette.muted))
                        .on_hover_text("Se configuran en «Carpetas a omitir».");
                }
                if s.skipped_count > 0 {
                    let list_copied = copied == Some(&Copied::SkippedList);
                    if render_skipped(ui, &palette, s, list_copied) {
                        *action = Some(ResultAction::CopySkipped);
                    }
                }
                ui.add_space(8.0);
                for (format, path) in &s.reports {
                    let is_copied = copied == Some(&Copied::Path(path.clone()));
                    if let Some(a) = result_row(ui, format.label(), path, is_copied) {
                        *action = Some(ResultAction::Row(a, path.clone()));
                    }
                }
                if !s.ext_stats.is_empty() {
                    ui.add_space(6.0);
                    render_type_stats(ui, &palette, &s.ext_stats, s.total_bytes);
                }
            }
            RunState::Failed { error } => {
                panel_title(ui, palette.error, "❌", "Ha ocurrido un error");
                ui.add_space(4.0);
                egui::Frame::new()
                    .fill(palette.error_fill)
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(error).monospace().size(12.0));
                    });
            }
            RunState::Cancelled {
                found,
                dirs_only,
                elapsed,
            } => {
                // Neutral, no rojo: cancelar no es un error del programa,
                // es una acción deliberada del usuario.
                let noun = crate::runner::found_noun(*dirs_only);
                let elapsed = fmt_seconds(elapsed.as_secs_f32());
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
            }
            RunState::Idle | RunState::Running(_) => {}
        }
    }
}

/// Acción pedida desde el modal de resultado, aplicada fuera de él.
enum ResultAction {
    /// Botón de la fila de un reporte.
    Row(RowAction, PathBuf),
    /// "Copiar lista" de los elementos no accesibles.
    CopySkipped,
}

/// Aviso plegable de los elementos que no se pudieron leer: la lista (hasta
/// `scanner::SKIPPED_LIST_LIMIT`) con el motivo de cada uno y un botón para
/// copiarla. Devuelve `true` si se pulsó "Copiar lista".
fn render_skipped(ui: &mut egui::Ui, palette: &Palette, s: &RunSummary, copied: bool) -> bool {
    let title = egui::RichText::new(format!("⚠ {}", runner::skipped_text(s.skipped_count)))
        .color(palette.warning);
    let mut copy = false;
    egui::CollapsingHeader::new(title)
        .id_salt("skipped_items")
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("skipped_scroll")
                .max_height(160.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for item in &s.skipped {
                        let path = item.path.display().to_string();
                        ui.label(egui::RichText::new(&path).monospace().size(11.0));
                        ui.label(
                            egui::RichText::new(format!("   {}", item.reason))
                                .color(palette.muted)
                                .size(11.0),
                        );
                    }
                });
            let rest = s.skipped_count.saturating_sub(s.skipped.len());
            if rest > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "… y {rest} más. La CLI con -v los muestra todos."
                    ))
                    .color(palette.muted)
                    .size(11.0),
                );
            }
            let label = if copied {
                "✔ Copiada"
            } else {
                "Copiar lista"
            };
            copy = ui
                .small_button(label)
                .on_hover_text("Copiar las rutas y los motivos al portapapeles")
                .clicked();
        });
    copy
}

/// Texto de "Copiar lista": una línea por elemento, `ruta<TAB>motivo` (se
/// pega en una hoja de cálculo como dos columnas), y una línea final si la
/// lista se cortó.
fn skipped_list_text(s: &RunSummary) -> String {
    let mut text: String = s
        .skipped
        .iter()
        .map(|i| format!("{}\t{}\n", i.path.display(), i.reason))
        .collect();
    let rest = s.skipped_count.saturating_sub(s.skipped.len());
    if rest > 0 {
        text.push_str(&format!("… y {rest} más\n"));
    }
    text
}

/// Tipos de archivo mostrados en el modal de resultado; el resto se agrupa
/// en "Otros". El detalle completo queda en report.md.
const TOP_TYPES: usize = 8;

/// Tabla plegable con los tipos de archivo que más ocupan: cantidad,
/// tamaño y una barra proporcional al porcentaje del tamaño total.
fn render_type_stats(ui: &mut egui::Ui, palette: &Palette, stats: &[ExtStat], total: u64) {
    egui::CollapsingHeader::new(format!("📁 Tipos de archivo ({})", stats.len()))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("type_stats")
                .num_columns(4)
                .spacing([14.0, 2.0])
                // Sin esto las filas toman `interact_size.y` (28 px, pensado
                // para botones) y la tabla queda demasiado espaciada.
                .min_row_height(18.0)
                .show(ui, |ui| {
                    for row in runner::top_types(stats, TOP_TYPES) {
                        // Números en monoespaciada con relleno para que queden
                        // alineados a la derecha (un `Layout::right_to_left`
                        // dentro del Grid estira la fila en alto).
                        ui.label(egui::RichText::new(&row.label).monospace());
                        ui.label(
                            egui::RichText::new(format!("{:>7} arch.", row.count)).monospace(),
                        );
                        ui.label(
                            egui::RichText::new(format!("{:>9}", runner::format_size(row.bytes)))
                                .monospace(),
                        );
                        ui.horizontal(|ui| {
                            let fraction = if total == 0 {
                                0.0
                            } else {
                                row.bytes as f32 / total as f32
                            };
                            share_bar(ui, palette, fraction);
                            ui.label(
                                egui::RichText::new(runner::format_percent(row.bytes, total))
                                    .color(palette.muted)
                                    .size(12.0),
                            );
                        });
                        ui.end_row();
                    }
                });
            if stats.len() > TOP_TYPES + 1 {
                ui.label(
                    egui::RichText::new("La lista completa está en report.md.")
                        .color(palette.muted)
                        .size(11.0),
                );
            }
        });
}

/// Barra horizontal de ancho fijo rellena en `fraction` (0–1).
fn share_bar(ui: &mut egui::Ui, palette: &Palette, fraction: f32) {
    let size = egui::vec2(120.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let radius = egui::CornerRadius::same(2);
    let painter = ui.painter();
    painter.rect_filled(rect, radius, palette.muted.gamma_multiply(0.25));
    // Un mínimo visible para tipos que ocupan muy poco pero no cero.
    let width = if fraction > 0.0 {
        (rect.width() * fraction.min(1.0)).max(2.0)
    } else {
        0.0
    };
    if width > 0.0 {
        let filled = egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height()));
        painter.rect_filled(filled, radius, palette.accent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::SkippedItem;

    fn summary(skipped: Vec<SkippedItem>, skipped_count: usize) -> RunSummary {
        RunSummary {
            reports: vec![],
            files_count: 0,
            total_bytes: 0,
            dirs_count: 0,
            dirs_only: false,
            skipped_count,
            skipped,
            excluded_dirs: vec![],
            ext_stats: vec![],
        }
    }

    #[test]
    fn skipped_list_text_is_one_tab_separated_line_per_item() {
        let item = |p: &str| SkippedItem {
            path: PathBuf::from(p),
            reason: "Acceso denegado. (os error 5)".to_string(),
        };
        let s = summary(vec![item("a"), item("b")], 2);
        assert_eq!(
            skipped_list_text(&s),
            "a\tAcceso denegado. (os error 5)\nb\tAcceso denegado. (os error 5)\n"
        );
        // Si la lista se cortó, se indica cuántos faltan.
        let s = summary(vec![item("a")], 5);
        assert!(skipped_list_text(&s).ends_with("… y 4 más\n"));
    }
}
