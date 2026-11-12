//! Formulario de configuración y botones de acción.

use super::palette::Palette;
use super::run::RunState;
use super::util::clean_path_input;
use super::widgets::{field_note, folder_row};
use super::{default_exclude_dirs, App, ALL_FORMATS};
use crate::runner::{self, ReportFormat};
use eframe::egui;

/// Sugerencias rápidas del campo de extensiones.
const EXTENSION_PRESETS: [&str; 4] = [
    "pdf, docx, xlsx",
    "jpg, png, gif",
    "mp4, mkv, avi",
    "txt, md",
];

/// Qué hay en disco en la ruta que escribió el usuario.
#[derive(Default, Clone, Copy, PartialEq)]
pub(super) enum PathStatus {
    #[default]
    Empty,
    Missing,
    NotDir,
    Dir,
}

/// Estado de una ruta del formulario, recalculado solo cuando cambia el
/// texto: consultar el disco en cada frame sería costoso con unidades de
/// red lentas.
#[derive(Default)]
pub(super) struct PathCheck {
    text: Option<String>,
    status: PathStatus,
}

impl PathCheck {
    pub fn refresh(&mut self, raw: &str) -> PathStatus {
        let cleaned = clean_path_input(raw);
        if self.text.as_deref() != Some(cleaned) {
            let path = std::path::Path::new(cleaned);
            self.status = if cleaned.is_empty() {
                PathStatus::Empty
            } else if path.is_dir() {
                PathStatus::Dir
            } else if path.exists() {
                PathStatus::NotDir
            } else {
                PathStatus::Missing
            };
            self.text = Some(cleaned.to_string());
        }
        self.status
    }

    pub fn status(&self) -> PathStatus {
        self.status
    }

    /// Fuerza a volver a consultar el disco la próxima vez (p. ej. al
    /// lanzar una corrida, por si la carpeta cambió sin tocar el texto).
    pub fn invalidate(&mut self) {
        self.text = None;
    }
}

impl App {
    /// ─── Sección: Formulario de entrada ──────────────────────────
    pub(super) fn render_form(&mut self, ui: &mut egui::Ui) {
        let palette = Palette::of(ui);
        let enabled = !self.is_busy();
        // En modo solo carpetas las extensiones no tienen efecto.
        let ext_enabled = enabled && !self.dirs_only;

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Configuración").strong().size(14.0));
                ui.add_space(6.0);

                // Carpeta a escanear
                ui.label(egui::RichText::new("📁  Carpeta a escanear").size(12.0));
                folder_row(
                    ui,
                    &mut self.input_path,
                    enabled,
                    "Selecciona la carpeta…",
                    || rfd::FileDialog::new().pick_folder(),
                );
                match self.input_check.status() {
                    PathStatus::Empty | PathStatus::Dir => {}
                    PathStatus::Missing => field_note(ui, palette.error, "✖ La ruta no existe"),
                    PathStatus::NotDir => {
                        field_note(ui, palette.error, "✖ Es un archivo, no una carpeta")
                    }
                }

                ui.add_space(8.0);

                // Carpeta de salida
                ui.label(egui::RichText::new("💾  Carpeta de salida").size(12.0));
                folder_row(
                    ui,
                    &mut self.output_path,
                    enabled,
                    "Por defecto: misma que la entrada",
                    || rfd::FileDialog::new().pick_folder(),
                );
                // Dónde acabarán los reportes, para que el valor por defecto
                // (la carpeta escaneada) no sea una sorpresa.
                match (self.output_check.status(), self.input_check.status()) {
                    (PathStatus::NotDir, _) => {
                        field_note(ui, palette.error, "✖ Es un archivo, no una carpeta")
                    }
                    (PathStatus::Missing, _) => field_note(
                        ui,
                        palette.muted,
                        "La carpeta no existe: se creará al generar los reportes",
                    ),
                    (PathStatus::Empty, PathStatus::Dir) => field_note(
                        ui,
                        palette.muted,
                        &format!(
                            "Los reportes se guardarán en la carpeta escaneada: {}",
                            clean_path_input(&self.input_path)
                        ),
                    ),
                    _ => {}
                }

                ui.add_space(8.0);

                // Extensiones
                ui.label(egui::RichText::new("🎯  Extensiones (separadas por coma)").size(12.0));
                let ext_width = ui.available_width().max(120.0);
                ui.add_enabled(
                    ext_enabled,
                    egui::TextEdit::singleline(&mut self.extensions)
                        .hint_text("pdf, docx, jpg  (vacío = todas)")
                        .desired_width(ext_width),
                )
                .on_disabled_hover_text("Sin efecto en modo solo carpetas.")
                .on_hover_text(
                    "Ej.: pdf, docx. Vacío = todas.\n\
                     Cuenta lo que va tras el último punto: \"foo.tar.gz\" es gz.",
                );

                // Sugerencias rápidas: cada una se suma a lo escrito (o se
                // quita, si ya estaba entera), en vez de reemplazarlo.
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Sugerencias:")
                            .color(palette.muted)
                            .size(11.0),
                    );
                    for preset in EXTENSION_PRESETS {
                        let active = preset_is_active(&self.extensions, preset);
                        if ui
                            .add_enabled(
                                ext_enabled,
                                egui::Button::new(egui::RichText::new(preset).size(11.0))
                                    .small()
                                    .selected(active),
                            )
                            .on_hover_text(if active { "Quitar" } else { "Añadir" })
                            .clicked()
                        {
                            self.extensions = toggle_preset(&self.extensions, preset);
                        }
                    }
                });

                ui.add_space(8.0);

                // Carpetas a omitir por nombre
                ui.label(
                    egui::RichText::new("🚫  Carpetas a omitir (separadas por coma)").size(12.0),
                );
                ui.add_enabled(
                    enabled,
                    egui::TextEdit::singleline(&mut self.exclude_dirs)
                        .hint_text(".git, node_modules  (vacío = ninguna)")
                        .desired_width(ui.available_width().max(120.0)),
                )
                .on_hover_text(
                    "Nombres de carpeta, no rutas: se omiten en cualquier\n\
                     nivel, con todo su contenido.",
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Sugerencias:")
                            .color(palette.muted)
                            .size(11.0),
                    );
                    let common = runner::COMMON_EXCLUDED_DIRS.join(", ");
                    let active = names_active(&self.exclude_dirs, &common, dir_list);
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(
                                egui::RichText::new("Carpetas de proyectos").size(11.0),
                            )
                            .small()
                            .selected(active),
                        )
                        .on_hover_text(format!(
                            "{}: {common}",
                            if active { "Quitar" } else { "Añadir" }
                        ))
                        .clicked()
                    {
                        self.exclude_dirs = toggle_names(&self.exclude_dirs, &common, dir_list);
                    }
                });

                ui.add_space(8.0);
                ui.add_enabled(
                    enabled,
                    egui::Checkbox::new(&mut self.dirs_only, "📂 Solo carpetas (omitir archivos)"),
                );

                // Formatos a generar
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("📄  Generar:").size(12.0));
                    for (format, on) in ReportFormat::ALL.iter().zip(self.formats.iter_mut()) {
                        ui.add_enabled(enabled, egui::Checkbox::new(on, format.label()));
                    }
                });
                if !self.formats.contains(&true) {
                    field_note(ui, palette.error, "✖ Elige al menos un formato");
                }
            });
    }

    /// Por qué no se puede lanzar un escaneo ahora mismo, o `None` si se
    /// puede. Compartido entre el botón "Generar reportes" (que lo muestra
    /// como tooltip al estar deshabilitado) y Enter.
    pub(super) fn run_blocker(&self) -> Option<&'static str> {
        if self.is_busy() {
            return Some("Hay un escaneo en curso.");
        }
        match self.input_check.status() {
            PathStatus::Empty => return Some("Elige primero la carpeta a escanear."),
            PathStatus::Missing | PathStatus::NotDir => {
                return Some("La carpeta a escanear no es válida.")
            }
            PathStatus::Dir => {}
        }
        if self.output_check.status() == PathStatus::NotDir {
            return Some("La carpeta de salida no es válida.");
        }
        if !self.formats.contains(&true) {
            return Some("Elige al menos un formato.");
        }
        None
    }

    pub(super) fn can_run(&self) -> bool {
        self.run_blocker().is_none()
    }

    /// ─── Sección: Botones de acción ──────────────────────────────
    pub(super) fn render_actions(&mut self, ui: &mut egui::Ui) {
        let palette = Palette::of(ui);
        ui.horizontal(|ui| {
            let run_btn = egui::Button::new(
                egui::RichText::new("▶  Generar reportes")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(palette.accent)
            .min_size(egui::vec2(180.0, 36.0));

            let run_resp = ui.add_enabled(self.can_run(), run_btn);
            let run_resp = match self.run_blocker() {
                Some(reason) => run_resp.on_disabled_hover_text(reason),
                None => run_resp.on_hover_text("Atajo: Enter"),
            };
            if run_resp.clicked() {
                self.request_run();
            }

            // Botón de cancelación: solo visible mientras hay un escaneo en curso.
            if self.is_running() {
                let cancel_btn = egui::Button::new(
                    egui::RichText::new("⏹  Cancelar")
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(palette.error)
                .min_size(egui::vec2(120.0, 36.0));

                if ui.add(cancel_btn).on_hover_text("Atajo: Esc").clicked() {
                    self.request_cancel();
                }
            }

            // Botón secundario: limpiar
            let can_clear = !self.is_busy()
                && (!self.input_path.is_empty()
                    || !self.output_path.is_empty()
                    || !self.extensions.is_empty()
                    || self.exclude_dirs != default_exclude_dirs()
                    || self.dirs_only
                    || self.formats != ALL_FORMATS);
            if ui
                .add_enabled(
                    can_clear,
                    egui::Button::new("🗑  Limpiar").min_size(egui::vec2(100.0, 36.0)),
                )
                .clicked()
            {
                self.clear();
            }

            // Resumen de la última corrida, en la misma fila para que se vea
            // sin desplazarse aunque la ventana sea baja.
            self.render_results(ui);
        });
    }

    /// Vuelve el formulario y el resultado al estado inicial.
    fn clear(&mut self) {
        self.input_path.clear();
        self.output_path.clear();
        self.extensions.clear();
        self.exclude_dirs = default_exclude_dirs();
        self.formats = ALL_FORMATS;
        self.dirs_only = false;
        self.state = RunState::Idle;
        self.pending_confirm = None;
        self.results_open = false;
        self.copied = None;
        self.clear_ui_error();
    }
}

/// Lista de extensiones de un campo (ver `runner::parse_extensions`).
fn ext_list(field: &str) -> Vec<String> {
    runner::parse_extensions([field])
}

/// Lista de nombres de carpeta de un campo (ver `runner::parse_dir_names`).
fn dir_list(field: &str) -> Vec<String> {
    runner::parse_dir_names([field])
}

/// `true` si todo lo de `preset` ya está en el campo, leídos ambos con
/// `parse`.
fn names_active(field: &str, preset: &str, parse: fn(&str) -> Vec<String>) -> bool {
    let current = parse(field);
    parse(preset).iter().all(|e| current.contains(e))
}

/// Pulsar una sugerencia: si ya estaba entera, la quita; si no, añade lo que
/// falte al final, conservando lo que el usuario escribió.
fn toggle_names(field: &str, preset: &str, parse: fn(&str) -> Vec<String>) -> String {
    let mut current = parse(field);
    let preset = parse(preset);
    if preset.iter().all(|e| current.contains(e)) {
        current.retain(|e| !preset.contains(e));
    } else {
        for e in preset {
            if !current.contains(&e) {
                current.push(e);
            }
        }
    }
    current.join(", ")
}

fn preset_is_active(field: &str, preset: &str) -> bool {
    names_active(field, preset, ext_list)
}

fn toggle_preset(field: &str, preset: &str) -> String {
    toggle_names(field, preset, ext_list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_preset_adds_missing_and_keeps_user_text() {
        assert_eq!(toggle_preset("", "pdf, docx"), "pdf, docx");
        assert_eq!(toggle_preset("txt", "pdf, docx"), "txt, pdf, docx");
        // Solo añade lo que falta.
        assert_eq!(toggle_preset("pdf; .TXT", "pdf, docx"), "pdf, txt, docx");
    }

    #[test]
    fn toggle_preset_removes_it_when_already_complete() {
        assert!(preset_is_active("txt, pdf, docx", "pdf, docx"));
        assert_eq!(toggle_preset("txt, pdf, docx", "pdf, docx"), "txt");
        assert!(!preset_is_active("pdf", "pdf, docx"));
    }

    #[test]
    fn dir_presets_keep_names_with_spaces_and_dots() {
        // Los nombres de carpeta no se separan por espacios ni pierden el
        // punto inicial, a diferencia de las extensiones.
        let common = ".git, node_modules";
        assert_eq!(
            toggle_names("Mis fotos", common, dir_list),
            "Mis fotos, .git, node_modules"
        );
        assert!(names_active("node_modules, .git", common, dir_list));
        assert_eq!(
            toggle_names("Mis fotos, .git, node_modules", common, dir_list),
            "Mis fotos"
        );
    }
}
