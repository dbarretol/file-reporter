//! Formulario de configuración y botones de acción.

use super::palette::Palette;
use super::run::RunState;
use super::util::clean_path_input;
use super::widgets::{field_note, folder_row};
use super::{default_exclude_dirs, App, ALL_FORMATS};
use crate::runner::{self, ReportFormat};
use eframe::egui;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// Sugerencias rápidas del campo de extensiones.
const EXTENSION_PRESETS: [&str; 4] = [
    "pdf, docx, xlsx",
    "jpg, png, gif",
    "mp4, mkv, avi",
    "txt, md",
];

/// Qué hay en disco en la ruta que escribió el usuario.
#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub(super) enum PathStatus {
    #[default]
    Empty,
    /// Texto nuevo aún sin consultar (ver `PathCheck`).
    Checking,
    Missing,
    NotDir,
    Dir,
}

/// Tiempo sin cambios en el texto antes de consultar el disco.
const PATH_CHECK_DEBOUNCE: Duration = Duration::from_millis(300);
/// Cada cuánto se mira si terminó una consulta en curso.
const PATH_CHECK_POLL: Duration = Duration::from_millis(50);

/// Estado de una ruta del formulario. El disco se consulta en otro hilo y
/// solo cuando el texto lleva `PATH_CHECK_DEBOUNCE` sin cambiar: en una
/// ruta de red (`\\servidor\…`) cada consulta puede tardar segundos, y
/// hacerla en el hilo de la interfaz a cada pulsación congelaba la ventana
/// mientras se escribía.
#[derive(Default)]
pub(super) struct PathCheck {
    /// Texto al que corresponde `status`.
    checked: Option<String>,
    status: PathStatus,
    /// Texto nuevo y desde cuándo no cambia.
    waiting: Option<(String, Instant)>,
    /// Consulta en curso: texto y canal por el que llegará el resultado.
    running: Option<(String, Receiver<PathStatus>)>,
}

impl PathCheck {
    /// Se llama en cada frame con el texto del campo. Devuelve el estado
    /// conocido; mientras el texto cambia o se consulta, `Checking`.
    pub fn refresh(&mut self, raw: &str, ctx: &egui::Context) -> PathStatus {
        let text = clean_path_input(raw);

        if let Some((asked, rx)) = &self.running {
            match rx.try_recv() {
                Ok(status) => {
                    // Un resultado de un texto que ya cambió se descarta.
                    if asked == text {
                        self.status = status;
                        self.checked = Some(asked.clone());
                    }
                    self.running = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(PATH_CHECK_POLL),
                Err(TryRecvError::Disconnected) => self.running = None,
            }
        }

        if self.checked.as_deref() == Some(text) {
            self.waiting = None;
            return self.status;
        }
        if text.is_empty() {
            self.status = PathStatus::Empty;
            self.checked = Some(String::new());
            self.waiting = None;
            return self.status;
        }
        if self
            .running
            .as_ref()
            .is_some_and(|(asked, _)| asked == text)
        {
            return self.status;
        }

        // Texto nuevo: esperar a que deje de cambiar antes de ir al disco.
        let since = match &self.waiting {
            Some((t, at)) if t == text => *at,
            _ => {
                self.status = PathStatus::Checking;
                let now = Instant::now();
                self.waiting = Some((text.to_string(), now));
                now
            }
        };
        let left = PATH_CHECK_DEBOUNCE.saturating_sub(since.elapsed());
        if !left.is_zero() {
            ctx.request_repaint_after(left);
            return self.status;
        }

        self.waiting = None;
        let (tx, rx) = mpsc::channel();
        let path = text.to_string();
        std::thread::spawn(move || {
            let _ = tx.send(disk_status(&path));
        });
        self.running = Some((text.to_string(), rx));
        ctx.request_repaint_after(PATH_CHECK_POLL);
        self.status
    }

    pub fn status(&self) -> PathStatus {
        self.status
    }

    /// Vuelve a consultar el disco cuanto antes (p. ej. al terminar una
    /// corrida, que pudo crear la carpeta de salida), sin esperar ni pasar
    /// por `Checking`: hasta que llegue la respuesta vale el estado anterior.
    pub fn invalidate(&mut self) {
        if let Some(text) = self.checked.take() {
            let past = Instant::now()
                .checked_sub(PATH_CHECK_DEBOUNCE)
                .unwrap_or_else(Instant::now);
            self.waiting = Some((text, past));
        }
    }
}

/// Qué hay en disco en `path` (no vacío).
fn disk_status(path: &str) -> PathStatus {
    let path = std::path::Path::new(path);
    if path.is_dir() {
        PathStatus::Dir
    } else if path.exists() {
        PathStatus::NotDir
    } else {
        PathStatus::Missing
    }
}

impl App {
    /// ─── Sección: Formulario de entrada ──────────────────────────
    /// Arriba lo imprescindible (carpeta a escanear y de salida); el resto,
    /// en "Opciones", plegado por defecto y con un resumen de una línea.
    pub(super) fn render_form(&mut self, ui: &mut egui::Ui) {
        let palette = Palette::of(ui);
        let enabled = !self.is_busy();

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                // Carpeta a escanear
                ui.label(
                    egui::RichText::new("Carpeta a escanear")
                        .strong()
                        .size(13.0),
                );
                folder_row(
                    ui,
                    &mut self.input_path,
                    enabled,
                    "Selecciona la carpeta o arrástrala a la ventana…",
                    &self.recent_inputs,
                    || rfd::FileDialog::new().pick_folder(),
                );
                match self.input_check.status() {
                    PathStatus::Empty | PathStatus::Checking => {}
                    // El campo corta las rutas largas por el final, que es
                    // lo que identifica la carpeta: se repite aquí.
                    PathStatus::Dir => field_note(
                        ui,
                        palette.muted,
                        &format!(
                            "Se escaneará: {}",
                            path_tail(clean_path_input(&self.input_path))
                        ),
                    ),
                    PathStatus::Missing => field_note(ui, palette.error, "✖ La ruta no existe"),
                    PathStatus::NotDir => {
                        field_note(ui, palette.error, "✖ Es un archivo, no una carpeta")
                    }
                }

                ui.add_space(8.0);

                // Carpeta de salida
                ui.label(egui::RichText::new("Carpeta de salida").strong().size(13.0));
                folder_row(
                    ui,
                    &mut self.output_path,
                    enabled,
                    "Por defecto: la carpeta escaneada",
                    &[],
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
                    (PathStatus::Dir, _) => field_note(
                        ui,
                        palette.muted,
                        &format!(
                            "Los reportes se guardarán en: {}",
                            path_tail(clean_path_input(&self.output_path))
                        ),
                    ),
                    (PathStatus::Empty, PathStatus::Dir) => field_note(
                        ui,
                        palette.muted,
                        "Los reportes se guardarán en la carpeta escaneada",
                    ),
                    _ => {}
                }

                ui.add_space(8.0);
                self.render_options(ui, &palette, enabled);
                // Fuera del plegable: con "Opciones" cerrado también se ve.
                if !self.formats.contains(&true) {
                    field_note(ui, palette.error, "✖ Elige al menos un formato en Opciones");
                }
            });
    }

    /// "Opciones": extensiones, carpetas a omitir, solo carpetas y formatos.
    /// Plegado muestra un resumen de lo elegido.
    fn render_options(&mut self, ui: &mut egui::Ui, palette: &Palette, enabled: bool) {
        // En modo solo carpetas las extensiones no tienen efecto.
        let ext_enabled = enabled && !self.dirs_only;
        let summary = self.options_summary();
        let mut header = egui::CollapsingHeader::new(
            egui::RichText::new(format!("Opciones  ·  {summary}")).size(13.0),
        )
        .id_salt("options")
        .default_open(false);
        // Siempre plegado al abrir la app: egui recuerda entre sesiones si
        // se dejó abierto, y se fuerza cerrado en el primer frame. Después,
        // abrirlo o cerrarlo funciona con normalidad.
        if !self.options_initialized {
            header = header.open(Some(false));
            self.options_initialized = true;
        }
        header.show(ui, |ui| {
            ui.add_space(4.0);
            // Extensiones
            ui.label(egui::RichText::new("Extensiones (separadas por coma)").size(12.0));
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
            ui.label(egui::RichText::new("Carpetas a omitir (separadas por coma)").size(12.0));
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
                        egui::Button::new(egui::RichText::new("Carpetas de proyectos").size(11.0))
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
                egui::Checkbox::new(&mut self.dirs_only, "Solo carpetas (omitir archivos)"),
            );

            // Formatos a generar
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Generar:").size(12.0));
                for (format, on) in ReportFormat::ALL.iter().zip(self.formats.iter_mut()) {
                    ui.add_enabled(enabled, egui::Checkbox::new(on, format.label()));
                }
            });
        });
    }

    /// Resumen de una línea de "Opciones", p. ej. "Todas las extensiones ·
    /// omite 7 carpetas · CSV + MD + JSON".
    fn options_summary(&self) -> String {
        let what = if self.dirs_only {
            "Solo carpetas".to_string()
        } else {
            let exts = ext_list(&self.extensions);
            if exts.is_empty() {
                "Todas las extensiones".to_string()
            } else {
                exts.join(", ")
            }
        };
        let dirs = dir_list(&self.exclude_dirs).len();
        let omit = match dirs {
            0 => "sin carpetas omitidas".to_string(),
            1 => "omite 1 carpeta".to_string(),
            n => format!("omite {n} carpetas"),
        };
        let formats: Vec<&str> = ReportFormat::ALL
            .iter()
            .zip(self.formats)
            .filter(|(_, on)| *on)
            .map(|(f, _)| match f {
                ReportFormat::Csv => "CSV",
                ReportFormat::Md => "MD",
                ReportFormat::Json => "JSON",
            })
            .collect();
        let formats = if formats.is_empty() {
            "ningún formato".to_string()
        } else {
            formats.join(" + ")
        };
        format!("{what}  ·  {omit}  ·  {formats}")
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
            PathStatus::Checking => return Some("Comprobando la carpeta a escanear…"),
            PathStatus::Missing | PathStatus::NotDir => {
                return Some("La carpeta a escanear no es válida.")
            }
            PathStatus::Dir => {}
        }
        match self.output_check.status() {
            PathStatus::NotDir => return Some("La carpeta de salida no es válida."),
            PathStatus::Checking => return Some("Comprobando la carpeta de salida…"),
            _ => {}
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

/// Últimos dos componentes de una ruta (`…\review\in`), la parte que la
/// identifica y que el campo de texto corta cuando es larga.
fn path_tail(path: &str) -> String {
    let p = std::path::Path::new(path);
    let mut parts: Vec<String> = p
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.reverse();
    let sep = std::path::MAIN_SEPARATOR.to_string();
    let tail = parts.join(&sep);
    if p.components().count() > 2 {
        format!("…{sep}{tail}")
    } else {
        path.to_string()
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

    /// Llama a `refresh` hasta que el estado deja de ser `Checking`.
    fn settle(check: &mut PathCheck, text: &str, ctx: &egui::Context) -> PathStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = check.refresh(text, ctx);
            if status != PathStatus::Checking || Instant::now() > deadline {
                return status;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn path_check_waits_and_then_asks_the_disk() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let dir_text = dir.path().display().to_string();
        let mut check = PathCheck::default();

        assert_eq!(check.refresh("", &ctx), PathStatus::Empty);
        // Texto nuevo: no va al disco enseguida.
        assert_eq!(check.refresh(&dir_text, &ctx), PathStatus::Checking);
        assert_eq!(settle(&mut check, &dir_text, &ctx), PathStatus::Dir);

        let missing = dir.path().join("no_existe").display().to_string();
        assert_eq!(settle(&mut check, &missing, &ctx), PathStatus::Missing);

        let file = dir.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let file_text = file.display().to_string();
        assert_eq!(settle(&mut check, &file_text, &ctx), PathStatus::NotDir);
    }

    #[test]
    fn path_check_invalidate_rechecks_without_flashing_checking() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("salida");
        let out_text = out.display().to_string();
        let mut check = PathCheck::default();
        assert_eq!(settle(&mut check, &out_text, &ctx), PathStatus::Missing);

        std::fs::create_dir(&out).unwrap();
        check.invalidate();
        // Hasta que llega la respuesta se conserva el estado anterior.
        assert_eq!(check.refresh(&out_text, &ctx), PathStatus::Missing);
        let deadline = Instant::now() + Duration::from_secs(5);
        while check.refresh(&out_text, &ctx) != PathStatus::Dir && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(check.status(), PathStatus::Dir);
    }

    fn app_with(extensions: &str, exclude: &str, dirs_only: bool, formats: [bool; 3]) -> App {
        App {
            extensions: extensions.to_string(),
            exclude_dirs: exclude.to_string(),
            dirs_only,
            formats,
            ..App::default()
        }
    }

    #[test]
    fn options_summary_describes_the_choices() {
        let app = app_with("", &default_exclude_dirs(), false, ALL_FORMATS);
        assert_eq!(
            app.options_summary(),
            "Todas las extensiones  ·  omite 7 carpetas  ·  CSV + MD + JSON"
        );
        let app = app_with(".PDF, docx", "build", false, [true, false, false]);
        assert_eq!(
            app.options_summary(),
            "pdf, docx  ·  omite 1 carpeta  ·  CSV"
        );
        // Solo carpetas: las extensiones no cuentan.
        let app = app_with("pdf", "", true, [false, false, false]);
        assert_eq!(
            app.options_summary(),
            "Solo carpetas  ·  sin carpetas omitidas  ·  ningún formato"
        );
    }

    #[test]
    fn path_tail_keeps_the_last_two_components() {
        let sep = std::path::MAIN_SEPARATOR;
        let long = ["", "datos", "proyectos", "review", "in"].join(&sep.to_string());
        assert_eq!(path_tail(&long), format!("…{sep}review{sep}in"));
        // Rutas cortas, tal cual.
        assert_eq!(path_tail("docs"), "docs");
    }

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
