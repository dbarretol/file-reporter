//! Interfaz gráfica (egui/eframe).
//!
//! - `mod.rs`: arranque, estado de la app, bucle por frame y paneles fijos.
//! - `form`: formulario y botones de acción.
//! - `run`: estado de la corrida y el hilo que escanea.
//! - `results`: panel de progreso, ventana (modal) con el resultado y su
//!   resumen en la fila de botones.
//! - `dialogs`: diálogo de sobrescritura y arrastrar y soltar.
//! - `palette`: colores por tema y elección de tema (`ThemeChoice`).
//! - `widgets`, `util`: piezas compartidas.

mod dialogs;
mod form;
mod palette;
mod results;
mod run;
mod util;
mod widgets;

use crate::runner::ReportFormat;
use anyhow::Result;
use eframe::egui;
use form::{PathCheck, RecentCheck};
use palette::{Palette, ThemeChoice};
use run::{PendingRun, RunState};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn launch() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 640.0]) // ancho suficiente para MAX_CONTENT_WIDTH (900) + márgenes
            .with_min_inner_size([720.0, 500.0]) // mínimo cómodo
            .with_title("File Reporter")
            // Icono de la ventana y de la barra de tareas (el del .exe en el
            // Explorador lo incrusta build.rs, con el mismo dibujo).
            .with_icon(egui::IconData {
                rgba: crate::icon::rgba(APP_ICON_SIZE),
                width: APP_ICON_SIZE,
                height: APP_ICON_SIZE,
            }),
        centered: true, // abre centrada en la pantalla (cuando el OS lo permite)
        ..Default::default()
    };

    eframe::run_native(
        "File Reporter",
        options,
        Box::new(|cc| {
            // Estilo global más amigable
            setup_custom_style(&cc.egui_ctx);
            Ok(Box::new(App::new(cc)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Error iniciando GUI: {e}"))
}

/// Tamaño del icono de la ventana; el sistema lo reduce para la barra de
/// título y la de tareas.
const APP_ICON_SIZE: u32 = 256;

/// Ajustes visuales globales (espaciado, tipografía, etc.).
fn setup_custom_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        // Espaciado más generoso
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 28.0;

        // Bordes ligeramente más redondeados
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);
    });
}

/// Tiempo que el botón "Copiar ruta" muestra "✔ Copiada".
const COPIED_TTL: Duration = Duration::from_secs(2);

/// Tiempo que un `ui_error` puntual permanece visible en la barra de estado
/// antes de desaparecer solo, si no lo reemplaza antes una acción nueva.
const UI_ERROR_TTL: Duration = Duration::from_secs(5);

/// Todos los formatos marcados (valor por defecto y al pulsar "Limpiar").
const ALL_FORMATS: [bool; 3] = [true; 3];

/// Carpetas a omitir de partida (valor por defecto y al pulsar "Limpiar"):
/// las comunes de proyectos de código, que casi nunca interesan en un
/// inventario. La CLI, en cambio, no omite nada si no se le pide.
fn default_exclude_dirs() -> String {
    crate::runner::COMMON_EXCLUDED_DIRS.join(", ")
}

const PERSIST_KEY: &str = eframe::APP_KEY;

/// Momentos, desde que la ventana se ve, en que se reenvía el tema a la
/// barra de título al arrancar; ver `App::sync_native_theme`.
const NATIVE_THEME_SYNC_AT: [Duration; 4] = [
    Duration::ZERO,
    Duration::from_millis(300),
    Duration::from_millis(1000),
    Duration::from_millis(2500),
];

#[derive(Default)]
struct App {
    // Formulario
    input_path: String,
    output_path: String,
    extensions: String,
    /// Nombres de carpeta a omitir, tal como los escribió el usuario.
    exclude_dirs: String,
    dirs_only: bool,
    /// Casillas CSV / Markdown / JSON, en el orden de `ReportFormat::ALL`.
    formats: [bool; 3],
    /// Últimas carpetas escaneadas, la más reciente primero (ver
    /// `util::push_recent`).
    recent_inputs: Vec<String>,
    /// Cuáles de `recent_inputs` siguen existiendo (ver `RecentCheck`).
    recent_check: RecentCheck,
    /// Tema elegido en la cabecera (por defecto, el del sistema).
    theme: ThemeChoice,
    /// Reenvíos del tema a la barra de título ya hechos al arrancar y
    /// cuándo se vio la ventana por primera vez (ver `sync_native_theme`).
    native_theme_syncs: usize,
    shown_at: Option<Instant>,

    /// Validación en vivo de las rutas tecleadas (ver `PathCheck`).
    input_check: PathCheck,
    output_check: PathCheck,

    /// Corrida en curso o resultado de la última.
    state: RunState,
    /// Corrida a la espera de confirmar la sobrescritura. Aparte de `state`
    /// porque el diálogo se abre encima del resultado anterior, que sigue
    /// a la vista.
    pending_confirm: Option<PendingRun>,

    /// Error de una acción puntual de UI (p. ej. "Abrir carpeta" falló al
    /// lanzar el explorador) y cuándo se produjo. Se muestra en la barra de
    /// estado sin tocar `state`, que es el resultado del escaneo en sí, y
    /// expira solo (ver `UI_ERROR_TTL`).
    ui_error: Option<(String, Instant)>,

    /// Si el modal con el resultado de la última corrida está abierto. Se
    /// abre al terminar cada corrida; al cerrarlo queda un resumen de una
    /// línea con un botón para volver a abrirlo.
    results_open: bool,

    /// Lo último copiado desde el resultado ("Copiar ruta" o "Copiar
    /// lista") y cuándo, para confirmar la copia en el botón durante
    /// `COPIED_TTL`.
    copied: Option<(Copied, Instant)>,

    /// Si "Opciones" ya se forzó plegado al arrancar (ver `render_options`).
    options_initialized: bool,
}

/// Qué se copió desde el modal de resultado.
#[derive(PartialEq)]
enum Copied {
    /// La ruta de un reporte.
    Path(PathBuf),
    /// La lista de elementos no accesibles.
    SkippedList,
}

/// Único subconjunto de `App` que se persiste entre sesiones (`eframe`
/// "persistence"): las rutas y extensiones tecleadas, no el estado de una
/// corrida en curso.
/// `#[serde(default)]`: un estado guardado por una versión anterior (sin
/// algún campo nuevo, p. ej. `dirs_only`) sigue cargándose con el valor por
/// defecto para ese campo, en vez de fallar y perder todo lo recordado.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct PersistedState {
    input_path: String,
    output_path: String,
    extensions: String,
    exclude_dirs: String,
    dirs_only: bool,
    /// Formatos marcados, en el orden de `ReportFormat::ALL`. Por defecto
    /// (también al cargar un estado de una versión que no lo guardaba), los
    /// tres.
    formats: [bool; 3],
    /// Tema elegido; los estados guardados antes de existir el selector
    /// siguen el del sistema.
    theme: ThemeChoice,
    /// Últimas carpetas escaneadas (vacío en estados de versiones
    /// anteriores).
    recent_inputs: Vec<String>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            input_path: String::new(),
            output_path: String::new(),
            extensions: String::new(),
            exclude_dirs: default_exclude_dirs(),
            dirs_only: false,
            formats: ALL_FORMATS,
            theme: ThemeChoice::System,
            recent_inputs: Vec::new(),
        }
    }
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = cc
            .storage
            .and_then(|s| eframe::get_value::<PersistedState>(s, PERSIST_KEY))
            .unwrap_or_default();
        state.theme.apply(&cc.egui_ctx);
        App {
            input_path: state.input_path,
            output_path: state.output_path,
            extensions: state.extensions,
            exclude_dirs: state.exclude_dirs,
            dirs_only: state.dirs_only,
            formats: state.formats,
            theme: state.theme,
            recent_inputs: state.recent_inputs,
            ..App::default()
        }
    }

    /// Formatos marcados en el formulario.
    fn selected_formats(&self) -> Vec<ReportFormat> {
        ReportFormat::ALL
            .into_iter()
            .zip(self.formats)
            .filter_map(|(f, on)| on.then_some(f))
            .collect()
    }

    /// Establece un `ui_error` puntual y arranca su temporizador de
    /// auto-expiración (ver `UI_ERROR_TTL`).
    fn set_ui_error(&mut self, msg: impl Into<String>) {
        self.ui_error = Some((msg.into(), Instant::now()));
    }

    /// Limpia el `ui_error` puntual, si hay uno.
    fn clear_ui_error(&mut self) {
        self.ui_error = None;
    }

    /// Quita los mensajes temporales que ya expiraron y pide un repintado
    /// para cuando expiren los que quedan.
    fn expire_messages(&mut self, ctx: &egui::Context) {
        if let Some((_, at)) = &self.copied {
            match COPIED_TTL.checked_sub(at.elapsed()) {
                Some(left) if !left.is_zero() => ctx.request_repaint_after(left),
                _ => self.copied = None,
            }
        }
        if let Some((_, at)) = &self.ui_error {
            match UI_ERROR_TTL.checked_sub(at.elapsed()) {
                Some(left) if !left.is_zero() => ctx.request_repaint_after(left),
                _ => self.clear_ui_error(),
            }
        }
    }

    /// Atajos de teclado: Enter lanza "Generar reportes" (igual que el
    /// botón, si el formulario es válido) y Esc cancela un escaneo en curso.
    /// Con el modal de resultado abierto, Enter lo cierra en vez de lanzar
    /// otra corrida por detrás (Esc lo cierra el propio modal).
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (enter, esc) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        // Enter en un campo de texto solo lo "confirma": no lanza el escaneo.
        let typing = ctx.memory(|m| m.focused().is_some());
        if self.results_visible() {
            if enter {
                self.results_open = false;
            }
        } else if enter && !typing && self.can_run() {
            self.request_run();
        }
        if esc && self.is_running() {
            self.request_cancel();
        }
    }

    /// eframe crea la ventana oculta y la muestra tras el primer cuadro, así
    /// que el tema aplicado en `App::new` le llega a la barra de título
    /// (que dibuja Windows) antes de que sea visible y Windows no la
    /// repinta: al arrancar en "Claro" con el sistema en oscuro, quedaba
    /// oscura con los botones oscuros hasta hacer clic. Se reenvía varias
    /// veces durante los primeros segundos desde el segundo cuadro (ya con la
    /// ventana visible): un solo reenvío no siempre llegaba a tiempo. Cada
    /// reenvío es invisible si la barra ya estaba bien.
    fn sync_native_theme(&mut self, ctx: &egui::Context) {
        let Some(&due) = NATIVE_THEME_SYNC_AT.get(self.native_theme_syncs) else {
            return;
        };
        if ctx.cumulative_frame_nr() == 0 {
            return;
        }
        let elapsed = self.shown_at.get_or_insert_with(Instant::now).elapsed();
        if elapsed >= due {
            self.theme.apply_native(ctx);
            self.native_theme_syncs += 1;
        }
        if let Some(&next) = NATIVE_THEME_SYNC_AT.get(self.native_theme_syncs) {
            ctx.request_repaint_after(next.saturating_sub(elapsed));
        }
    }

    /// Barra superior con el título y el selector de tema.
    fn render_header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("header")
            .exact_size(56.0)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("📋  File Reporter")
                                .size(20.0)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Genera reportes (CSV, Markdown, JSON) de la estructura de un directorio",
                            )
                            .color(Palette::of(ui).muted)
                            .size(12.0),
                        );
                    });
                    // Selector de tema a la derecha: tres botones, el
                    // elegido resaltado.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(16.0);
                        self.render_theme_selector(ui);
                    });
                });
            });
    }

    /// Botones Sistema / Claro / Oscuro. El cambio se aplica al instante y
    /// se guarda con el resto del estado.
    fn render_theme_selector(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        // De derecha a izquierda: se recorren al revés para que se lean
        // "Sistema · Claro · Oscuro".
        ui.spacing_mut().item_spacing.x = 4.0;
        for choice in ThemeChoice::ALL.into_iter().rev() {
            let label = egui::RichText::new(choice.label()).size(12.0);
            if ui
                .selectable_label(self.theme == choice, label)
                .on_hover_text(choice.hint())
                .clicked()
            {
                picked = Some(choice);
            }
        }
        ui.label(
            egui::RichText::new("Tema:")
                .color(Palette::of(ui).muted)
                .size(12.0),
        );
        if let Some(choice) = picked.filter(|c| *c != self.theme) {
            self.theme = choice;
            choice.apply(ui.ctx());
        }
    }

    /// Barra de estado inferior.
    fn render_statusbar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("statusbar")
            .exact_size(24.0)
            .show(ui, |ui| {
                let palette = Palette::of(ui);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    // Un error de una acción puntual (p. ej. "Abrir carpeta"
                    // falló) tiene prioridad sobre el estado general.
                    let (icon, text, color): (&str, Cow<str>, _) = if let Some((err, _)) =
                        &self.ui_error
                    {
                        ("⚠", err.into(), palette.error)
                    } else {
                        // ⚪/⚫ y no 🟢 & co.: la fuente de emoji de egui es
                        // monocroma y no trae todos los círculos de color
                        // (🟢 salía como un cuadro). El color lo da la
                        // etiqueta.
                        match &self.state {
                            RunState::Idle => ("⚪", "Listo".into(), palette.muted),
                            // La fase real, no un genérico "Procesando…".
                            RunState::Running(run) if run.writing() => {
                                ("⚫", "Generando reportes…".into(), palette.accent)
                            }
                            // Con los contadores: el panel de progreso
                            // puede quedar fuera de la vista en ventanas
                            // bajas, la barra de estado siempre se ve.
                            RunState::Running(run) => {
                                let status = crate::summary::scan_status(
                                    run.found(),
                                    run.dirs_scanned(),
                                    run.dirs_only,
                                );
                                ("⚫", format!("Escaneando… {status}").into(), palette.accent)
                            }
                            RunState::Done { .. } => ("⚫", "Completado".into(), palette.success),
                            RunState::Failed { .. } => ("⚫", "Error".into(), palette.error),
                            RunState::Cancelled { .. } => {
                                ("⏹", "Cancelado".into(), palette.warning)
                            }
                        }
                    };
                    ui.colored_label(color, icon);
                    ui.colored_label(color, text.as_ref());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                                .color(palette.muted)
                                .size(11.0),
                        );
                    });
                });
            });
    }

    /// Panel central: formulario, acciones (con el resumen del resultado) y
    /// progreso. El detalle del resultado va en un modal aparte.
    fn render_main(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default_margins().show(ui, |ui| {
            // Sin auto_shrink el ancho del área es siempre el del panel, no el
            // del contenido del frame anterior.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Centrar el contenido y limitar su ancho máximo en pantallas grandes.
                    const MAX_CONTENT_WIDTH: f32 = 900.0;
                    const SIDE_PADDING: f32 = 16.0;

                    let available = ui.available_width();

                    // El contenido NUNCA debe exceder lo realmente disponible.
                    // - En pantallas grandes: usa MAX_CONTENT_WIDTH y se centra.
                    // - En pantallas pequeñas: usa todo el ancho menos el padding mínimo.
                    let content_width = if available > MAX_CONTENT_WIDTH + 2.0 * SIDE_PADDING {
                        MAX_CONTENT_WIDTH
                    } else {
                        (available - 2.0 * SIDE_PADDING).max(200.0)
                    };

                    let side_margin = ((available - content_width) / 2.0).max(SIDE_PADDING);

                    // Margen lateral con `add_space` y no con `Frame::inner_margin`:
                    // `Margin` usa `i8` y no admite márgenes de más de 127 px.
                    let item_spacing = ui.spacing().item_spacing;
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(side_margin);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = item_spacing;
                            // Forzamos a TODOS los hijos a respetar el ancho máximo.
                            ui.set_max_width(content_width);

                            ui.add_space(12.0);
                            self.render_form(ui);
                            ui.add_space(8.0);
                            self.render_actions(ui);
                            ui.add_space(8.0);
                            self.render_progress(ui);
                            ui.add_space(12.0);
                        });
                    });
                });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        if self.is_running() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        self.input_check.refresh(&self.input_path, ctx);
        self.output_check.refresh(&self.output_path, ctx);
        self.recent_check.refresh(&self.recent_inputs, ctx);
        self.sync_native_theme(ctx);
        self.expire_messages(ctx);
        self.poll_worker(ctx);

        self.handle_dropped_files(ctx);
        self.render_drop_hint(ctx);
        self.handle_shortcuts(ctx);
        self.render_overwrite_confirm(ctx);
        self.render_results_modal(ctx);

        self.render_header(ui);
        self.render_statusbar(ui);
        self.render_main(ui);
    }

    /// Persiste las rutas y extensiones tecleadas (no el estado de una
    /// corrida en curso) para restaurarlas en la próxima sesión.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let state = PersistedState {
            input_path: self.input_path.clone(),
            output_path: self.output_path.clone(),
            extensions: self.extensions.clone(),
            exclude_dirs: self.exclude_dirs.clone(),
            dirs_only: self.dirs_only,
            formats: self.formats,
            theme: self.theme,
            recent_inputs: self.recent_inputs.clone(),
        };
        eframe::set_value(storage, PERSIST_KEY, &state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_state_from_older_version_gets_the_defaults() {
        // Un estado guardado antes de existir `formats` no debe dejar la
        // GUI sin ningún formato marcado.
        // (eframe lo guarda en RON; `#[serde(default)]` actúa igual en
        // cualquier formato de serde, así que basta con JSON.)
        let old = r#"{"input_path":"C:\\x","output_path":"","extensions":"","dirs_only":false}"#;
        let state: PersistedState = serde_json::from_str(old).unwrap();
        assert_eq!(state.formats, ALL_FORMATS);
        assert_eq!(state.exclude_dirs, default_exclude_dirs());
        assert_eq!(state.theme, ThemeChoice::System);
        assert_eq!(state.input_path, "C:\\x");
        assert!(state.recent_inputs.is_empty());
    }

    #[test]
    fn persisted_recent_inputs_round_trip() {
        let state = PersistedState {
            recent_inputs: vec!["C:\\a".to_string(), "D:\\b".to_string()],
            ..PersistedState::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PersistedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.recent_inputs, state.recent_inputs);
    }
    #[test]
    fn persisted_theme_round_trips() {
        let state = PersistedState {
            theme: ThemeChoice::Dark,
            ..PersistedState::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: PersistedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.theme, ThemeChoice::Dark);
    }
}
