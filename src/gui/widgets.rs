//! Piezas de interfaz reutilizables, sin estado propio.

use super::util::truncate_path_display;
use eframe::egui;
use std::path::{Path, PathBuf};

/// Fila de selección de carpeta: input + botón "Examinar…" alineados y, si
/// `recent` no está vacío, un menú 🕘 con esas carpetas para rellenar el
/// campo con un clic. `recent` ya viene sin las que se sabe que no existen
/// (ver `form::RecentCheck`): aquí no se consulta el disco.
pub(super) fn folder_row(
    ui: &mut egui::Ui,
    value: &mut String,
    enabled: bool,
    hint: &str,
    recent: &[&String],
    pick: impl FnOnce() -> Option<PathBuf>,
) {
    // De derecha a izquierda: primero el botón, con su ancho real, y el campo
    // ocupa exactamente lo que queda. Reservar un ancho fijo para el botón
    // (como antes) se quedaba corto: la fila se salía del panel y este se
    // ensanchaba frame a frame, cortando el botón con rutas largas.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(enabled, egui::Button::new("Examinar…"))
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

/// Menú de carpetas recientes. Sin acceso al disco: se dibuja en cada frame
/// mientras está abierto, y una carpeta de red que no responde congelaría la
/// ventana.
fn recent_menu(ui: &mut egui::Ui, value: &mut String, recent: &[&String]) {
    ui.menu_button("🕘", |ui| {
        for &path in recent {
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

/// Fila de resultado, en tres celdas de un `Grid`: formato, nombre del
/// archivo (la ruta completa, en el tooltip) y los botones "Abrir" y
/// "Copiar ruta". `copied` muestra la confirmación de que esa ruta acaba de
/// copiarse. Quien llama cierra la fila (`end_row`).
pub(super) fn result_row(
    ui: &mut egui::Ui,
    label: &str,
    path: &Path,
    copied: bool,
) -> Option<RowAction> {
    let path_str = path.display().to_string();
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path_str.clone());
    let mut action = None;
    ui.label(egui::RichText::new(label).strong().size(12.0));
    ui.label(egui::RichText::new(&file_name).monospace().size(12.0))
        .on_hover_text(&path_str);
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
