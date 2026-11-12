//! Piezas de interfaz reutilizables, sin estado propio.

use super::util::truncate_path_display;
use eframe::egui;
use std::path::{Path, PathBuf};

/// Fila de selección de carpeta: input + botón "Examinar…" alineados y, si
/// `recent` no está vacío, un menú 🕘 con esas carpetas para rellenar el
/// campo con un clic.
pub(super) fn folder_row(
    ui: &mut egui::Ui,
    value: &mut String,
    enabled: bool,
    hint: &str,
    recent: &[String],
    pick: impl FnOnce() -> Option<PathBuf>,
) {
    // De derecha a izquierda: primero el botón, con su ancho real, y el campo
    // ocupa exactamente lo que queda. Reservar un ancho fijo para el botón
    // (como antes) se quedaba corto: la fila se salía del panel y este se
    // ensanchaba frame a frame, cortando el botón con rutas largas.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(enabled, egui::Button::new("📂  Examinar…"))
                .clicked()
            {
                if let Some(path) = pick() {
                    *value = path.display().to_string();
                }
            }
            if !recent.is_empty() {
                ui.add_enabled_ui(enabled, |ui| recent_menu(ui, value, recent));
            }
            ui.add_enabled(
                enabled,
                egui::TextEdit::singleline(value)
                    .hint_text(hint)
                    .desired_width(ui.available_width()),
            );
        });
    });
}

/// Menú de carpetas recientes. Las que ya no existen no se ofrecen; se
/// comprueba solo con el menú abierto, no en cada frame.
fn recent_menu(ui: &mut egui::Ui, value: &mut String, recent: &[String]) {
    ui.menu_button("🕘", |ui| {
        let existing: Vec<&String> = recent.iter().filter(|p| Path::new(p).is_dir()).collect();
        if existing.is_empty() {
            ui.label("Las carpetas recientes ya no existen.");
        }
        for path in existing {
            let button = egui::Button::new(truncate_path_display(path, 70)).truncate();
            if ui.add(button).on_hover_text(path.as_str()).clicked() {
                value.clone_from(path);
                ui.close();
            }
        }
    })
    .response
    .on_hover_text("Carpetas recientes");
}

/// Acción pedida desde una fila de resultado.
pub(super) enum RowAction {
    Open,
    Copy,
}

/// Fila de resultado: botones "Abrir" y "Copiar ruta", etiqueta y ruta
/// (truncada, completa en el tooltip). `copied` muestra la confirmación de
/// que esa ruta acaba de copiarse.
pub(super) fn result_row(
    ui: &mut egui::Ui,
    label: &str,
    path: &Path,
    copied: bool,
) -> Option<RowAction> {
    let path_str = path.display().to_string();
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .small_button("Abrir")
            .on_hover_text(format!("Abrir el reporte {label}"))
            .clicked()
        {
            action = Some(RowAction::Open);
        }
        let copy_label = if copied { "✔ Copiada" } else { "Copiar ruta" };
        if ui
            .small_button(copy_label)
            .on_hover_text("Copiar la ruta al portapapeles")
            .clicked()
        {
            action = Some(RowAction::Copy);
        }
        ui.label(egui::RichText::new(format!("{label}:")).strong().size(12.0));
        ui.label(
            egui::RichText::new(truncate_path_display(&path_str, 60))
                .monospace()
                .size(11.0),
        )
        .on_hover_text(&path_str);
    });
    action
}

/// Título del resultado de una corrida: icono + texto, ambos en `color`.
pub(super) fn panel_title(ui: &mut egui::Ui, color: egui::Color32, icon: &str, text: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(color, egui::RichText::new(icon).size(16.0));
        ui.label(egui::RichText::new(text).strong().size(14.0).color(color));
    });
}

/// Nota pequeña bajo un campo del formulario (validación o información).
pub(super) fn field_note(ui: &mut egui::Ui, color: egui::Color32, text: &str) {
    ui.label(egui::RichText::new(text).color(color).size(11.0));
}
