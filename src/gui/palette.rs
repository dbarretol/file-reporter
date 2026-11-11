//! Colores de la interfaz, con una variante para cada tema.

use eframe::egui::{self, Color32};

/// Colores semánticos de la interfaz para el tema activo.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub accent: Color32,
    pub success: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub muted: Color32,
    /// Fondo del panel de error.
    pub error_fill: Color32,
}

impl Palette {
    /// Tonos pensados para tema oscuro (el diseño original).
    const DARK: Self = Self {
        accent: Color32::from_rgb(70, 130, 220),
        success: Color32::from_rgb(46, 160, 67),
        error: Color32::from_rgb(220, 70, 70),
        warning: Color32::from_rgb(210, 150, 40),
        muted: Color32::from_rgb(150, 150, 150),
        error_fill: Color32::from_rgba_unmultiplied_const(220, 70, 70, 20),
    };

    /// Variantes más oscuras/saturadas (~Tailwind 600-800) para tema claro:
    /// los tonos de `DARK`, calibrados para fondo oscuro, quedan por debajo
    /// de 4.5:1 de contraste (WCAG AA para texto) sobre fondo blanco.
    const LIGHT: Self = Self {
        accent: Color32::from_rgb(30, 64, 175),
        success: Color32::from_rgb(21, 128, 61),
        error: Color32::from_rgb(185, 28, 28),
        warning: Color32::from_rgb(180, 83, 9),
        muted: Color32::from_rgb(75, 85, 99),
        error_fill: Color32::from_rgba_unmultiplied_const(220, 70, 70, 20),
    };

    /// Paleta del tema activo (egui sigue el tema del sistema). Toma un
    /// `Ui` porque cada sitio de uso ya tiene uno a mano.
    pub fn of(ui: &egui::Ui) -> Self {
        if ui.visuals().dark_mode {
            Self::DARK
        } else {
            Self::LIGHT
        }
    }
}
