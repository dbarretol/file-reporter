//! Lo que aparece por encima del formulario: el diálogo de sobrescritura y
//! el aviso al arrastrar una carpeta sobre la ventana.

use super::palette::Palette;
use super::util::dropped_path_to_input_dir;
use super::App;
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
            "📁  Suelta aquí para escanear",
            egui::FontId::proportional(24.0),
            egui::Color32::WHITE,
        );
    }

    /// ─── Diálogo: confirmar sobrescritura de reportes existentes ──
    pub(super) fn render_overwrite_confirm(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending_confirm else {
            return;
        };

        let mut confirmed = false;
        let mut rename = false;
        // Esc cierra el diálogo como si se pulsara "Cancelar" — sin esto, el
        // diálogo no responde al teclado en absoluto.
        let mut cancelled = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        egui::Window::new("⚠  Sobrescribir reportes existentes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Ya existen reportes de una corrida anterior en la carpeta de salida:");
                ui.add_space(6.0);
                for f in &pending.existing {
                    // Espacio no separable: si la ruta no cabe en una línea,
                    // que no se corte justo después de la viñeta.
                    ui.label(
                        egui::RichText::new(format!("•\u{a0}{}", f.display()))
                            .monospace()
                            .size(11.0),
                    );
                }
                ui.add_space(10.0);
                ui.label("¿Deseas sobrescribirlos o guardar los nuevos con otro nombre?");
                let extensions: Vec<&str> =
                    pending.cfg.formats.iter().map(|f| f.extension()).collect();
                ui.label(
                    egui::RichText::new(format!(
                        "Con otro nombre: {}.{}",
                        pending.alt_name,
                        if extensions.len() == 1 {
                            extensions[0].to_string()
                        } else {
                            format!("{{{}}}", extensions.join(","))
                        }
                    ))
                    .monospace()
                    .size(11.0),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Sobrescribir").color(egui::Color32::WHITE),
                            )
                            .fill(Palette::of(ui).error),
                        )
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
                    if ui.button("Cancelar").clicked() {
                        cancelled = true;
                    }
                });
            });

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
