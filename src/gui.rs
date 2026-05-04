use crate::runner::{self, Progress, RunConfig, RunSummary};
use anyhow::Result;
use eframe::egui;
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
            Ok(Box::<App>::default())
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
    summary: Option<RunSummary>,
    error: Option<String>,
}

#[derive(Default, PartialEq)]
enum Status {
    #[default]
    Idle,
    Running,
    Done,
    Failed,
}

enum WorkerMsg {
    Done(RunSummary),
    Failed(String),
}

// ─── Constantes de estilo ────────────────────────────────────────
const ACCENT: egui::Color32 = egui::Color32::from_rgb(70, 130, 220);
const SUCCESS: egui::Color32 = egui::Color32::from_rgb(46, 160, 67);
const ERROR: egui::Color32 = egui::Color32::from_rgb(220, 70, 70);
const MUTED: egui::Color32 = egui::Color32::from_rgb(150, 150, 150);

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.status == Status::Running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Drenar mensajes del worker
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMsg::Done(s) => {
                        self.summary = Some(s);
                        self.status = Status::Done;
                    }
                    WorkerMsg::Failed(e) => {
                        self.error = Some(e);
                        self.status = Status::Failed;
                    }
                }
            }
        }

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
                            .color(MUTED)
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
                    let (icon, text, color) = match self.status {
                        Status::Idle => ("⚪", "Listo", MUTED),
                        Status::Running => ("🔵", "Procesando…", ACCENT),
                        Status::Done => ("🟢", "Completado", SUCCESS),
                        Status::Failed => ("🔴", "Error", ERROR),
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

                // Si el usuario eligió input pero output está vacío, sincronizar
                if !self.input_path.is_empty() && self.output_path.is_empty() {
                    self.output_path = self.input_path.clone();
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

                ui.add_space(8.0);

                // Extensiones
                ui.label(egui::RichText::new("🎯  Extensiones (separadas por coma)").size(12.0));
                let ext_width = ui.available_width().max(120.0);
                ui.add_enabled(
                    enabled,
                    egui::TextEdit::singleline(&mut self.extensions)
                        .hint_text("pdf, docx, jpg  (vacío = todas)")
                        .desired_width(ext_width),
                );

                // Sugerencias rápidas
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Sugerencias:").color(MUTED).size(11.0));
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

    /// ─── Sección: Botones de acción ──────────────────────────────
    fn render_actions(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let can_run = self.status != Status::Running && !self.input_path.trim().is_empty();

            let run_btn = egui::Button::new(
                egui::RichText::new("▶  Generar reportes")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(ACCENT)
            .min_size(egui::vec2(180.0, 36.0));

            if ui.add_enabled(can_run, run_btn).clicked() {
                self.start_run();
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

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                // Header: "Progreso" + ETA
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Progreso").strong().size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(self.compute_eta(frac))
                                .color(MUTED)
                                .size(12.0),
                        );
                    });
                });
                ui.add_space(6.0);

                // ProgressBar SIN desired_width: egui ajusta automáticamente
                // y evitamos cualquier problema de cálculo de ancho.
                let bar = egui::ProgressBar::new(frac.clamp(0.0, 1.0))
                    .show_percentage()
                    .animate(self.status == Status::Running);
                ui.add(bar);

                ui.add_space(4.0);
                let label = if total > 0 {
                    format!("{} / {} archivos procesados", done, total)
                } else {
                    "Contando archivos…".to_string()
                };
                ui.label(egui::RichText::new(label).color(MUTED).size(11.0));
            });
    }

    /// ─── Sección: Resultados o error ─────────────────────────────
    fn render_results(&self, ui: &mut egui::Ui) {
        match self.status {
            Status::Done => {
                let Some(s) = &self.summary else { return };

                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(SUCCESS, egui::RichText::new("✅").size(16.0));
                            ui.label(
                                egui::RichText::new("Reportes generados correctamente")
                                    .strong()
                                    .size(14.0)
                                    .color(SUCCESS),
                            );
                        });
                        ui.add_space(6.0);
                        ui.label(format!(
                            "📊 Total de archivos procesados: {}",
                            s.files_count
                        ));
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
                                if let Some(parent) = s.csv_path.parent() {
                                    let _ = open_in_explorer(parent);
                                }
                            }
                            if ui
                                .button(egui::RichText::new("📄  Abrir CSV").size(12.0))
                                .clicked()
                            {
                                let _ = open_in_explorer(&s.csv_path);
                            }
                        });
                    });
            }
            Status::Failed => {
                let Some(e) = &self.error else { return };
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12.0))
                    .fill(egui::Color32::from_rgba_unmultiplied(220, 70, 70, 20))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(ERROR, egui::RichText::new("❌").size(16.0));
                            ui.label(
                                egui::RichText::new("Ha ocurrido un error")
                                    .strong()
                                    .color(ERROR),
                            );
                        });
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(e).monospace().size(12.0));
                    });
            }
            _ => {}
        }
    }

    fn start_run(&mut self) {
        self.summary = None;
        self.error = None;
        self.status = Status::Running;
        self.started_at = Some(Instant::now());

        let progress = Progress::new();
        self.progress = Some(progress.clone());

        let (tx, rx) = mpsc::channel::<WorkerMsg>();
        self.rx = Some(rx);

        let input = PathBuf::from(self.input_path.trim());
        let output = if self.output_path.trim().is_empty() {
            input.clone()
        } else {
            PathBuf::from(self.output_path.trim())
        };
        let extensions: Vec<String> = self
            .extensions
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .map(|s| s.trim().trim_start_matches('.').to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        let cfg = RunConfig {
            input,
            output,
            extensions,
        };

        // Worker en hilo separado. Capturamos panics para que nunca tumben la GUI.
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runner::run(&cfg, Some(progress))
            }));

            let msg = match result {
                Ok(Ok(s)) => WorkerMsg::Done(s),
                Ok(Err(e)) => WorkerMsg::Failed(e.to_string()),
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
    }

    fn compute_eta(&self, fraction: f32) -> String {
        let Some(start) = self.started_at else {
            return "ETA: —".into();
        };
        if fraction <= 0.001 {
            return "ETA: calculando…".into();
        }
        if fraction >= 1.0 || self.status == Status::Done {
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
