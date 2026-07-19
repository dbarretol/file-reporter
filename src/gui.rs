use crate::runner::{self, Progress, RunConfig, RunSummary};
use anyhow::Result;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Instant;

pub fn launch() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 640.0]) // ancho suficiente para MAX_CONTENT_WIDTH (900) + márgenes
            .with_min_inner_size([720.0, 500.0]) // mínimo cómodo
            .with_title("File Reporter"),
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

/// Ajustes visuales globales (espaciado, tipografía, etc.).
fn setup_custom_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Espaciado más generoso
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size.y = 28.0;

    // Bordes ligeramente más redondeados
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(4.0);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(4.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(4.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(4.0);

    ctx.set_style(style);
}

#[derive(Default)]
struct App {
    input_path: String,
    output_path: String,
    extensions: String,
    status: Status,

    // Comunicación con el worker
    progress: Option<Arc<Progress>>,
    rx: Option<Receiver<WorkerMsg>>,
    started_at: Option<Instant>,
    finished_at: Option<Instant>,
    summary: Option<RunSummary>,
    error: Option<String>,

    // Corrida a la espera de confirmación de sobrescritura (ver `request_run`).
    pending_confirm: Option<RunConfig>,

    /// Error de una acción puntual de UI (p. ej. "Abrir carpeta" falló al
    /// lanzar el explorador). Se muestra en la barra de estado sin tocar
    /// `status`/`error`, que son del resultado del escaneo en sí.
    ui_error: Option<String>,
    /// Cuándo se puso `ui_error`, para poder auto-expirarlo (ver
    /// `UI_ERROR_TTL`) en vez de dejarlo pegado en la barra de estado para
    /// siempre (tarea 32 en tasks.md).
    ui_error_at: Option<Instant>,
}

/// Tiempo que un `ui_error` puntual permanece visible en la barra de estado
/// antes de desaparecer solo, si no lo reemplaza antes una acción nueva.
const UI_ERROR_TTL: std::time::Duration = std::time::Duration::from_secs(5);

/// Único subconjunto de `App` que se persiste entre sesiones (`eframe`
/// "persistence"): las rutas y extensiones tecleadas, no el estado de una
/// corrida en curso.
#[derive(Default, Serialize, Deserialize)]
struct PersistedState {
    input_path: String,
    output_path: String,
    extensions: String,
}

const PERSIST_KEY: &str = eframe::APP_KEY;

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = App::default();
        if let Some(storage) = cc.storage {
            if let Some(state) = eframe::get_value::<PersistedState>(storage, PERSIST_KEY) {
                app.input_path = state.input_path;
                app.output_path = state.output_path;
                app.extensions = state.extensions;
            }
        }
        app
    }

    /// Establece un `ui_error` puntual y arranca su temporizador de
    /// auto-expiración (ver `UI_ERROR_TTL`).
    fn set_ui_error(&mut self, msg: impl Into<String>) {
        self.ui_error = Some(msg.into());
        self.ui_error_at = Some(Instant::now());
    }

    /// Limpia el `ui_error` puntual, si hay uno.
    fn clear_ui_error(&mut self) {
        self.ui_error = None;
        self.ui_error_at = None;
    }
}

#[derive(Default, PartialEq)]
enum Status {
    #[default]
    Idle,
    Running,
    Done,
    Failed,
    /// El usuario pidió cancelar y el worker terminó en respuesta a eso.
    /// Distinto de `Failed`: no es un error, así que la UI no debe mostrar
    /// el panel rojo de error (ver tarea 27 en tasks.md).
    Cancelled,
}

enum WorkerMsg {
    Done(RunSummary),
    Failed(String),
    Cancelled,
}

// ─── Constantes de estilo ────────────────────────────────────────
// Tonos pensados para tema oscuro (el diseño original).
const ACCENT_DARK: egui::Color32 = egui::Color32::from_rgb(70, 130, 220);
const SUCCESS_DARK: egui::Color32 = egui::Color32::from_rgb(46, 160, 67);
const ERROR_DARK: egui::Color32 = egui::Color32::from_rgb(220, 70, 70);
const WARNING_DARK: egui::Color32 = egui::Color32::from_rgb(210, 150, 40);
const MUTED_DARK: egui::Color32 = egui::Color32::from_rgb(150, 150, 150);

// Variantes más oscuras/saturadas (~Tailwind 600-800) para tema claro: los
// tonos de arriba, calibrados para fondo oscuro, quedan por debajo de 4.5:1
// de contraste (WCAG AA para texto) sobre un fondo blanco/casi blanco.
const ACCENT_LIGHT: egui::Color32 = egui::Color32::from_rgb(30, 64, 175);
const SUCCESS_LIGHT: egui::Color32 = egui::Color32::from_rgb(21, 128, 61);
const ERROR_LIGHT: egui::Color32 = egui::Color32::from_rgb(185, 28, 28);
const WARNING_LIGHT: egui::Color32 = egui::Color32::from_rgb(180, 83, 9);
const MUTED_LIGHT: egui::Color32 = egui::Color32::from_rgb(75, 85, 99);

/// Elige la variante del color según el tema activo del sistema (egui sigue
/// el tema del SO). Todas las funciones toman `&egui::Ui` porque cada sitio
/// de uso ya tiene uno a mano — evita tener que enhebrar el tema como
/// parámetro extra por todos los métodos `render_*`.
fn accent_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode { ACCENT_DARK } else { ACCENT_LIGHT }
}
fn success_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode { SUCCESS_DARK } else { SUCCESS_LIGHT }
}
fn error_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode { ERROR_DARK } else { ERROR_LIGHT }
}
fn warning_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode { WARNING_DARK } else { WARNING_LIGHT }
}
fn muted_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode { MUTED_DARK } else { MUTED_LIGHT }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.status == Status::Running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Auto-expira el `ui_error` puntual en vez de dejarlo pegado en la
        // barra de estado para siempre (ver tarea 32 en tasks.md).
        if let Some(at) = self.ui_error_at {
            let elapsed = at.elapsed();
            if elapsed >= UI_ERROR_TTL {
                self.clear_ui_error();
            } else {
                ctx.request_repaint_after(UI_ERROR_TTL - elapsed);
            }
        }

        // Drenar mensajes del worker
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMsg::Done(s) => {
                        self.summary = Some(s);
                        self.status = Status::Done;
                        self.finished_at = Some(Instant::now());
                    }
                    WorkerMsg::Failed(e) => {
                        self.error = Some(e);
                        self.status = Status::Failed;
                        self.finished_at = Some(Instant::now());
                    }
                    WorkerMsg::Cancelled => {
                        self.status = Status::Cancelled;
                        self.finished_at = Some(Instant::now());
                    }
                }
            }
        }

        self.handle_dropped_files(ctx);
        self.render_drop_hint(ctx);
        self.handle_enter_to_run(ctx);
        self.render_overwrite_confirm(ctx);

        // Top bar con título
        egui::TopBottomPanel::top("header")
            .exact_height(56.0)
            .show(ctx, |ui| {
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
                            .color(muted_color(ui))
                            .size(12.0),
                        );
                    });
                });
            });

        // Status bar abajo
        egui::TopBottomPanel::bottom("statusbar")
            .exact_height(24.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    // Un error de una acción puntual (p. ej. "Abrir carpeta"
                    // falló) tiene prioridad sobre el estado general.
                    let (icon, text, color) = if let Some(err) = &self.ui_error {
                        ("⚠", err.as_str(), error_color(ui))
                    } else {
                        match self.status {
                            Status::Idle => ("⚪", "Listo", muted_color(ui)),
                            Status::Running => ("🔵", self.running_phase_text(), accent_color(ui)),
                            Status::Done => ("🟢", "Completado", success_color(ui)),
                            Status::Failed => ("🔴", "Error", error_color(ui)),
                            Status::Cancelled => ("⏹", "Cancelado", warning_color(ui)),
                        }
                    };
                    ui.colored_label(color, icon);
                    ui.colored_label(color, text);
                });
            });

        // Panel principal
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
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

                egui::Frame::none()
                    .inner_margin(egui::Margin {
                        left: side_margin,
                        right: side_margin,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .show(ui, |ui| {
                        // Forzamos a TODOS los hijos a respetar el ancho máximo.
                        ui.set_max_width(content_width);

                        ui.add_space(12.0);
                        self.render_form(ui);
                        ui.add_space(8.0);
                        self.render_actions(ui);
                        ui.add_space(8.0);
                        self.render_progress(ui);
                        ui.add_space(8.0);
                        self.render_results(ui);
                        ui.add_space(12.0);
                    });
            });
        });
    }

    /// Persiste las rutas y extensiones tecleadas (no el estado de una
    /// corrida en curso) para restaurarlas en la próxima sesión.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let state = PersistedState {
            input_path: self.input_path.clone(),
            output_path: self.output_path.clone(),
            extensions: self.extensions.clone(),
        };
        eframe::set_value(storage, PERSIST_KEY, &state);
    }
}

impl App {
    /// ─── Sección: Formulario de entrada ──────────────────────────
    fn render_form(&mut self, ui: &mut egui::Ui) {
        let enabled = self.status != Status::Running;

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12.0))
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

                ui.add_space(8.0);

                // Extensiones
                ui.label(egui::RichText::new("🎯  Extensiones (separadas por coma)").size(12.0));
                let ext_width = ui.available_width().max(120.0);
                ui.add_enabled(
                    enabled,
                    egui::TextEdit::singleline(&mut self.extensions)
                        .hint_text("pdf, docx, jpg  (vacío = todas)")
                        .desired_width(ext_width),
                )
                .on_hover_text(
                    "Solo la última parte tras el último punto cuenta como extensión: \
                     \"foo.tar.gz\" se filtra con \"gz\", no con \"tar.gz\". \
                     Los archivos sin punto en el nombre (p. ej. \"LICENSE\", \"Makefile\") \
                     no tienen extensión y quedan fuera de cualquier filtro no vacío.",
                );

                // Sugerencias rápidas
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Sugerencias:").color(muted_color(ui)).size(11.0));
                    for preset in [
                        "pdf, docx, xlsx",
                        "jpg, png, gif",
                        "mp4, mkv, avi",
                        "txt, md",
                    ] {
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new(egui::RichText::new(preset).size(11.0)).small(),
                            )
                            .clicked()
                        {
                            self.extensions = preset.to_string();
                        }
                    }
                });
            });
    }

    /// Arrastrar una carpeta a la ventana rellena "Carpeta a escanear"
    /// (el gesto más natural para esta app). Se ignora mientras hay un
    /// escaneo en curso, igual que el resto del formulario.
    /// Si el formulario tiene lo mínimo para lanzar un escaneo (no hay uno en
    /// curso, no hay un diálogo de confirmación pendiente, y hay una ruta de
    /// entrada). Compartido entre el botón "Generar reportes" y Enter.
    fn can_run(&self) -> bool {
        self.status != Status::Running
            && self.pending_confirm.is_none()
            && !clean_path_input(&self.input_path).is_empty()
    }

    /// Enter en cualquier campo del formulario dispara "Generar reportes",
    /// igual que pulsar el botón, siempre que el formulario sea válido.
    fn handle_enter_to_run(&mut self, ctx: &egui::Context) {
        let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        if enter_pressed && self.can_run() {
            self.request_run();
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        if self.status == Status::Running {
            return;
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.iter().find_map(|f| f.path.clone()) {
            self.input_path = dropped_path_to_input_dir(path).display().to_string();
        }
    }

    /// Overlay mientras se arrastra un archivo/carpeta sobre la ventana, para
    /// que quede claro que soltarlo ahí lo usa como carpeta a escanear.
    fn render_drop_hint(&self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering || self.status == Status::Running {
            return;
        }
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("drop_overlay"),
        ));
        painter.rect_filled(
            screen,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
        );
        painter.text(
            screen.center(),
            egui::Align2::CENTER_CENTER,
            "📁  Suelta aquí para escanear",
            egui::FontId::proportional(24.0),
            egui::Color32::WHITE,
        );
    }

    /// ─── Diálogo: confirmar sobrescritura de reportes existentes ──
    fn render_overwrite_confirm(&mut self, ctx: &egui::Context) {
        let Some(cfg) = &self.pending_confirm else {
            return;
        };
        let files = existing_reports(cfg);

        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("⚠  Sobrescribir reportes existentes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Ya existen reportes de una corrida anterior en la carpeta de salida:");
                ui.add_space(6.0);
                for f in &files {
                    ui.label(
                        egui::RichText::new(format!("• {}", f.display()))
                            .monospace()
                            .size(11.0),
                    );
                }
                ui.add_space(10.0);
                ui.label("¿Deseas sobrescribirlos?");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Sobrescribir").color(egui::Color32::WHITE),
                            )
                            .fill(error_color(ui)),
                        )
                        .clicked()
                    {
                        confirmed = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cancelled = true;
                    }
                });
            });

        if confirmed {
            if let Some(cfg) = self.pending_confirm.take() {
                self.start_run(cfg);
            }
        } else if cancelled {
            self.pending_confirm = None;
        }
    }

    /// ─── Sección: Botones de acción ──────────────────────────────
    fn render_actions(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let can_run = self.can_run();

            let run_btn = egui::Button::new(
                egui::RichText::new("▶  Generar reportes")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(accent_color(ui))
            .min_size(egui::vec2(180.0, 36.0));

            if ui.add_enabled(can_run, run_btn).clicked() {
                self.request_run();
            }

            // Botón de cancelación: solo visible mientras hay un escaneo en curso.
            if self.status == Status::Running {
                let cancel_btn = egui::Button::new(
                    egui::RichText::new("⏹  Cancelar")
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(error_color(ui))
                .min_size(egui::vec2(120.0, 36.0));

                if ui.add(cancel_btn).clicked() {
                    if let Some(p) = &self.progress {
                        p.cancel.store(true, Ordering::Relaxed);
                    }
                }
            }

            // Botón secundario: limpiar
            let can_clear = self.status != Status::Running
                && (!self.input_path.is_empty()
                    || !self.output_path.is_empty()
                    || !self.extensions.is_empty());
            if ui
                .add_enabled(
                    can_clear,
                    egui::Button::new("🧹  Limpiar").min_size(egui::vec2(100.0, 36.0)),
                )
                .clicked()
            {
                self.input_path.clear();
                self.output_path.clear();
                self.extensions.clear();
                self.summary = None;
                self.error = None;
                self.progress = None;
                self.pending_confirm = None;
                self.started_at = None;
                self.finished_at = None;
                self.clear_ui_error();
                self.status = Status::Idle;
            }
        });
    }

    /// ─── Sección: Barra de progreso + ETA ────────────────────────
    fn render_progress(&self, ui: &mut egui::Ui) {
        let Some(p) = &self.progress else { return };

        let frac = p.fraction();
        let done = p.done.load(Ordering::Relaxed);
        let total = p.total.load(Ordering::Relaxed);
        // Una vez que el worker terminó (Done/Failed/cancelado), el `Progress`
        // compartido queda congelado tal cual en su último instante y ya no
        // hay nada que animar: seguir dibujando `ui.spinner()` fuerza repaint
        // continuo (~60 fps) aunque la app esté ociosa. `running` gatea todas
        // las ramas animadas de abajo.
        let running = self.status == Status::Running;
        let writing = running && p.writing.load(Ordering::Relaxed);

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                // Header: "Progreso" + ETA
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Progreso").strong().size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(self.compute_eta(frac))
                                .color(muted_color(ui))
                                .size(12.0),
                        );
                    });
                });
                ui.add_space(6.0);

                if writing {
                    // El escaneo ya terminó (done/total quedaron fijos en el
                    // conteo final); ahora se escriben los 3 reportes, lo
                    // cual puede tardar un momento con árboles muy grandes.
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("Generando reportes…")
                                .color(muted_color(ui))
                                .size(12.0),
                        );
                    });
                } else if total > 0 {
                    // ProgressBar SIN desired_width: egui ajusta automáticamente
                    // y evitamos cualquier problema de cálculo de ancho.
                    let bar = egui::ProgressBar::new(frac.clamp(0.0, 1.0))
                        .show_percentage()
                        .animate(running);
                    ui.add(bar);

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("{done} / {total} archivos procesados"))
                            .color(muted_color(ui))
                            .size(11.0),
                    );
                } else if running {
                    // No hacemos una pasada previa solo para contar archivos
                    // (duplicaría el I/O y casi duplicaría el tiempo total),
                    // así que mientras se escanea no conocemos el total:
                    // mostramos progreso indeterminado con un contador en vivo.
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new(format!("{done} archivos encontrados…"))
                                .color(muted_color(ui))
                                .size(12.0),
                        );
                    });
                } else {
                    // Terminó (o se canceló) antes de llegar a conocer un
                    // total: no hay barra que mostrar, solo el conteo final
                    // congelado, sin animación.
                    ui.label(
                        egui::RichText::new(format!("{done} archivos procesados"))
                            .color(muted_color(ui))
                            .size(11.0),
                    );
                }
            });
    }

    /// Texto de la barra de estado mientras hay un escaneo en curso: la fase
    /// real (Escaneando vs. Generando reportes), no un genérico "Procesando…".
    fn running_phase_text(&self) -> &'static str {
        let writing = self
            .progress
            .as_ref()
            .is_some_and(|p| p.writing.load(Ordering::Relaxed));
        if writing {
            "Generando reportes…"
        } else {
            "Escaneando…"
        }
    }

    /// ─── Sección: Resultados o error ─────────────────────────────
    fn render_results(&mut self, ui: &mut egui::Ui) {
        match self.status {
            Status::Done => {
                let Some(s) = &self.summary else { return };
                let mut open_result: Option<Result<(), String>> = None;

                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(success_color(ui), egui::RichText::new("✅").size(16.0));
                            ui.label(
                                egui::RichText::new("Reportes generados correctamente")
                                    .strong()
                                    .size(14.0)
                                    .color(success_color(ui)),
                            );
                        });
                        ui.add_space(6.0);
                        ui.label(format!(
                            "📊 Total de archivos procesados: {}",
                            s.files_count
                        ));
                        if s.skipped_count > 0 {
                            ui.colored_label(
                                warning_color(ui),
                                format!(
                                    "⚠ {} elementos no accesibles fueron omitidos",
                                    s.skipped_count
                                ),
                            );
                        }
                        ui.add_space(8.0);

                        // Tabla de archivos generados
                        result_row(ui, "CSV", &s.csv_path);
                        result_row(ui, "Markdown", &s.md_path);
                        result_row(ui, "JSON", &s.json_path);

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button(
                                    egui::RichText::new("📂  Abrir carpeta de salida").size(12.0),
                                )
                                .clicked()
                            {
                                // /select, abre el Explorador en la carpeta
                                // contenedora con el CSV ya resaltado.
                                open_result = Some(
                                    open_selecting(&s.csv_path)
                                        .map_err(|e| format!("No se pudo abrir la carpeta: {e}")),
                                );
                            }
                            if ui
                                .button(egui::RichText::new("📄  Abrir CSV").size(12.0))
                                .clicked()
                            {
                                open_result = Some(
                                    open_in_explorer(&s.csv_path)
                                        .map_err(|e| format!("No se pudo abrir el CSV: {e}")),
                                );
                            }
                        });
                    });

                if let Some(result) = open_result {
                    // Una acción puntual exitosa limpia cualquier ui_error
                    // pendiente de una acción anterior, no solo lo reemplaza
                    // si falla de nuevo (ver tarea 32).
                    match result.err() {
                        Some(e) => self.set_ui_error(e),
                        None => self.clear_ui_error(),
                    }
                }
            }
            Status::Failed => {
                let Some(e) = &self.error else { return };
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12.0))
                    .fill(egui::Color32::from_rgba_unmultiplied(220, 70, 70, 20))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(error_color(ui), egui::RichText::new("❌").size(16.0));
                            ui.label(
                                egui::RichText::new("Ha ocurrido un error")
                                    .strong()
                                    .color(error_color(ui)),
                            );
                        });
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(e).monospace().size(12.0));
                    });
            }
            Status::Cancelled => {
                // Neutral, no rojo: cancelar no es un error del programa,
                // es una acción deliberada del usuario (ver tarea 27).
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(warning_color(ui), egui::RichText::new("⏹").size(16.0));
                            ui.label(
                                egui::RichText::new("Escaneo cancelado")
                                    .strong()
                                    .color(warning_color(ui)),
                            );
                        });
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "El escaneo se detuvo antes de terminar porque el usuario lo canceló.",
                            )
                            .color(muted_color(ui))
                            .size(12.0),
                        );
                    });
            }
            _ => {}
        }
    }

    /// Arma el `RunConfig` a partir de los campos del formulario.
    fn build_cfg(&self) -> RunConfig {
        // "Copiar como ruta de acceso" de Windows pega la ruta entre comillas
        // (p. ej. "C:\..."); las quitamos además del trim de espacios normal.
        let input = PathBuf::from(clean_path_input(&self.input_path));
        let output = if clean_path_input(&self.output_path).is_empty() {
            input.clone()
        } else {
            PathBuf::from(clean_path_input(&self.output_path))
        };
        let extensions: Vec<String> = self
            .extensions
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .map(|s| s.trim().trim_start_matches('.').to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        RunConfig {
            input,
            output,
            extensions,
        }
    }

    /// Se llama al pulsar "Generar reportes". Si ya existen reportes de una
    /// corrida anterior en la carpeta de salida, pide confirmación antes de
    /// sobrescribirlos en vez de pisarlos en silencio.
    fn request_run(&mut self) {
        let cfg = self.build_cfg();
        if existing_reports(&cfg).is_empty() {
            self.start_run(cfg);
        } else {
            self.pending_confirm = Some(cfg);
        }
    }

    fn start_run(&mut self, cfg: RunConfig) {
        self.summary = None;
        self.error = None;
        self.clear_ui_error();
        self.status = Status::Running;
        self.started_at = Some(Instant::now());
        self.finished_at = None;

        let progress = Progress::new();
        self.progress = Some(progress.clone());

        let (tx, rx) = mpsc::channel::<WorkerMsg>();
        self.rx = Some(rx);

        // Worker en hilo separado. Capturamos panics para que nunca tumben la GUI.
        // Pila ampliada (ver `main::SCAN_THREAD_STACK_SIZE`): este hilo inicia la
        // recursión de `scanner::build_tree` y, por el work-stealing de rayon,
        // también puede ejecutar parte de ella directamente en su propia pila.
        let spawn_result = std::thread::Builder::new()
            .stack_size(crate::SCAN_THREAD_STACK_SIZE)
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runner::run(&cfg, Some(progress.clone()))
                }));

                let msg = match result {
                    Ok(Ok(s)) => WorkerMsg::Done(s),
                    // Distinguimos cancelación de error real preguntando la
                    // bandera compartida (la puso en `true` el botón
                    // "Cancelar"), no comparando el texto del error: así no
                    // depende de que el mensaje de `runner::run` no cambie
                    // (ver tarea 27).
                    Ok(Err(e)) => {
                        if progress.cancel.load(Ordering::Relaxed) {
                            WorkerMsg::Cancelled
                        } else {
                            WorkerMsg::Failed(e.to_string())
                        }
                    }
                    Err(panic_payload) => {
                        let detail = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "panic desconocido en el worker".to_string()
                        };
                        WorkerMsg::Failed(format!("Error inesperado: {detail}"))
                    }
                };
                let _ = tx.send(msg);
            });
        if spawn_result.is_err() {
            self.status = Status::Idle;
            self.set_ui_error("No se pudo iniciar el hilo de escaneo");
        }
    }

    fn compute_eta(&self, fraction: f32) -> String {
        let Some(start) = self.started_at else {
            return "ETA: —".into();
        };
        // Congelamos el tiempo total en cuanto el worker termina (Done/Failed),
        // en vez de recalcular `start.elapsed()` en cada repaint (que seguiría
        // creciendo aunque el usuario mire la pantalla mucho después).
        if let Some(finished) = self.finished_at {
            return format!(
                "⏱  Tiempo total: {}",
                fmt_seconds(finished.duration_since(start).as_secs_f32())
            );
        }
        // Durante la fase de escritura, `done`/`total` (y por lo tanto
        // `fraction`) ya quedaron fijos en el conteo final antes de empezar
        // a volcar los reportes a disco, así que sin este chequeo caería en
        // la rama de `fraction >= 1.0` de más abajo y diría "Tiempo total"
        // como si ya hubiera terminado, cuando el worker sigue corriendo.
        let writing = self
            .progress
            .as_ref()
            .is_some_and(|p| p.writing.load(Ordering::Relaxed));
        if writing {
            return format!(
                "⏱  Escribiendo reportes… {}",
                fmt_seconds(start.elapsed().as_secs_f32())
            );
        }
        if fraction <= 0.001 {
            return "ETA: calculando…".into();
        }
        if fraction >= 1.0 {
            return format!(
                "⏱  Tiempo total: {}",
                fmt_seconds(start.elapsed().as_secs_f32())
            );
        }
        let elapsed = start.elapsed().as_secs_f32();
        let total_est = elapsed / fraction;
        let remaining = (total_est - elapsed).max(0.0);
        format!("⏳  ETA: {}", fmt_seconds(remaining))
    }
}

// ─── Helpers de UI ──────────────────────────────────────────────

/// Fila de selección de carpeta: input + botón "Examinar…" alineados.
fn folder_row(
    ui: &mut egui::Ui,
    value: &mut String,
    enabled: bool,
    hint: &str,
    pick: impl FnOnce() -> Option<PathBuf>,
) {
    ui.horizontal(|ui| {
        let btn_width = 110.0;
        let spacing = ui.spacing().item_spacing.x;

        // Ancho disponible REAL (respetando max_width del padre).
        // Reservamos el botón + spacing + un pequeño margen de seguridad.
        let raw = ui.available_width() - btn_width - spacing - 4.0;
        let available = raw.clamp(80.0, 2000.0); // mínimo 80, máximo 2000

        ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .desired_width(available),
        );
        if ui
            .add_enabled(
                enabled,
                egui::Button::new("📂  Examinar…").min_size(egui::vec2(btn_width, 0.0)),
            )
            .clicked()
        {
            if let Some(path) = pick() {
                *value = path.display().to_string();
            }
        }
    });
}

/// Fila de resultado: etiqueta + ruta con tooltip al pasar el mouse.
fn result_row(ui: &mut egui::Ui, label: &str, path: &std::path::Path) {
    let path_str = path.display().to_string();
    let display = truncate_path_display(&path_str, 70);

    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!("• {label}:"))
                .strong()
                .size(12.0),
        );
        ui.label(egui::RichText::new(display).monospace().size(11.0))
            .on_hover_text(&path_str);
    });
}

/// Limpia una ruta pegada por el usuario: quita espacios y, si vienen,
/// las comillas que agrega "Copiar como ruta de acceso" de Windows.
fn clean_path_input(s: &str) -> &str {
    s.trim().trim_matches('"')
}

/// El campo "carpeta a escanear" espera un directorio. Si el usuario suelta
/// un archivo suelto (no una carpeta) por drag & drop, usar su ruta tal
/// cual dejaría un `input_path` que `runner::run` rechazaría con "La ruta
/// no es un directorio" — en su lugar, usamos la carpeta que lo contiene
/// (ver tarea 30 en tasks.md).
fn dropped_path_to_input_dir(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        path
    } else {
        path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
    }
}

/// Reportes de una corrida anterior que ya existen en la carpeta de salida
/// de `cfg` y que esta corrida sobrescribiría.
fn existing_reports(cfg: &RunConfig) -> Vec<PathBuf> {
    ["report.csv", "report.md", "report.json"]
        .into_iter()
        .map(|name| cfg.output.join(name))
        .filter(|p| p.exists())
        .collect()
}

/// Trunca un string a `max_chars` caracteres (no bytes), prefijando "…"
/// si fue acortado. Seguro con UTF-8.
fn truncate_path_display(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    // Tomar los últimos (max_chars - 1) caracteres y prefijar con "…"
    let skip = char_count - (max_chars - 1);
    let tail: String = s.chars().skip(skip).collect();
    format!("…{}", tail)
}

fn fmt_seconds(s: f32) -> String {
    let s = s as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{h}h {m:02}m {sec:02}s")
    } else if m > 0 {
        format!("{m}m {sec:02}s")
    } else {
        format!("{sec}s")
    }
}

#[cfg(target_os = "windows")]
fn open_in_explorer(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(not(target_os = "windows"))]
fn open_in_explorer(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    std::process::Command::new(cmd)
        .arg(path)
        .spawn()
        .map(|_| ())
}

/// Abre la carpeta contenedora de `path` con el archivo ya resaltado
/// (en vez de una carpeta "en blanco" sin selección).
#[cfg(target_os = "windows")]
fn open_selecting(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
}

#[cfg(not(target_os = "windows"))]
fn open_selecting(path: &std::path::Path) -> std::io::Result<()> {
    // No hay un equivalente universal a "/select," fuera de Windows;
    // abrimos la carpeta contenedora sin resaltar el archivo.
    match path.parent() {
        Some(parent) => open_in_explorer(parent),
        None => open_in_explorer(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_path_display_noop_when_short_enough() {
        assert_eq!(truncate_path_display("short", 10), "short");
        assert_eq!(truncate_path_display("exact", 5), "exact");
    }

    #[test]
    fn truncate_path_display_truncates_from_the_front() {
        let s = "C:\\Users\\someone\\Documents\\file.txt";
        let out = truncate_path_display(s, 10);
        assert_eq!(out.chars().count(), 10);
        assert!(out.starts_with('…'));
        let tail: String = out.chars().skip(1).collect();
        assert!(s.ends_with(&tail));
    }

    #[test]
    fn truncate_path_display_respects_utf8_char_boundaries() {
        // Cadena con multibyte (acentos, emoji) cerca del punto de corte:
        // debe truncar por caracteres, no por bytes, sin panickear.
        let s = "café_😀_carpeta_con_ñ_y_más_texto.txt";
        let out = truncate_path_display(s, 12);
        assert_eq!(out.chars().count(), 12);
        assert!(out.starts_with('…'));
    }

    #[test]
    fn fmt_seconds_boundaries() {
        assert_eq!(fmt_seconds(0.0), "0s");
        assert_eq!(fmt_seconds(59.0), "59s");
        assert_eq!(fmt_seconds(60.0), "1m 00s");
        assert_eq!(fmt_seconds(3599.0), "59m 59s");
        assert_eq!(fmt_seconds(3600.0), "1h 00m 00s");
        assert_eq!(fmt_seconds(3661.0), "1h 01m 01s");
    }

    #[test]
    fn dropped_path_to_input_dir_keeps_directories_as_is() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();
        assert_eq!(dropped_path_to_input_dir(dir_path.clone()), dir_path);
    }

    #[test]
    fn dropped_path_to_input_dir_uses_parent_for_files() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("dropped.txt");
        std::fs::write(&file_path, "x").unwrap();
        assert_eq!(dropped_path_to_input_dir(file_path), dir.path().to_path_buf());
    }
}
