//! Icono de la aplicación dibujado por código: un portapapeles azul con una
//! hoja que muestra una lista indentada, como un árbol de carpetas.
//!
//! Solo usa `std`, para poder reutilizarlo desde `build.rs` (que genera el
//! `.ico` incrustado en el ejecutable de Windows) además de en la GUI (icono
//! de la ventana y de la barra de tareas).

/// Color RGBA sin premultiplicar.
type Rgba = [f32; 4];

const BOARD_TOP: Rgba = [0.33, 0.58, 0.93, 1.0];
const BOARD_BOTTOM: Rgba = [0.18, 0.40, 0.78, 1.0];
const PAPER: Rgba = [0.97, 0.98, 1.0, 1.0];
const CLIP: Rgba = [0.17, 0.21, 0.29, 1.0];
const CLIP_HOLE: Rgba = [0.33, 0.58, 0.93, 1.0];
const BULLET: Rgba = [0.27, 0.51, 0.88, 1.0];
const LINE: Rgba = [0.55, 0.60, 0.68, 1.0];

/// Filas de la lista: (sangría en x, y, largo de la línea).
const ROWS: [(f32, f32, f32); 4] = [
    (0.0, 0.40, 0.30),
    (0.09, 0.52, 0.21),
    (0.09, 0.64, 0.17),
    (0.0, 0.76, 0.27),
];

/// Píxeles RGBA (8 bits por canal, filas de arriba abajo) del icono de
/// `size` × `size`.
pub fn rgba(size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    // Ancho del borde suavizado: un píxel, en coordenadas normalizadas.
    let aa = 1.0 / size as f32;
    for py in 0..size {
        for px in 0..size {
            let x = (px as f32 + 0.5) / size as f32;
            let y = (py as f32 + 0.5) / size as f32;
            let c = pixel(x, y, aa);
            out.extend(c.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    out
}

/// Color del punto (x, y), con x, y en [0, 1] y el origen arriba a la
/// izquierda. Las formas se componen de abajo arriba con "over".
fn pixel(x: f32, y: f32, aa: f32) -> Rgba {
    let mut c: Rgba = [0.0; 4];

    // Tablero, con un degradado vertical suave.
    let t = ((y - 0.10) / 0.84).clamp(0.0, 1.0);
    let board = lerp(BOARD_TOP, BOARD_BOTTOM, t);
    over(
        &mut c,
        board,
        cover(rounded_rect(x, y, 0.13, 0.10, 0.87, 0.95, 0.11), aa),
    );

    // Hoja.
    over(
        &mut c,
        PAPER,
        cover(rounded_rect(x, y, 0.23, 0.24, 0.77, 0.87, 0.05), aa),
    );

    // Pinza, con un agujero redondo.
    over(
        &mut c,
        CLIP,
        cover(rounded_rect(x, y, 0.33, 0.04, 0.67, 0.22, 0.06), aa),
    );
    over(
        &mut c,
        CLIP_HOLE,
        cover(circle(x, y, 0.50, 0.105, 0.035), aa),
    );

    // Lista: viñeta cuadrada + línea, con sangría en las filas 2 y 3.
    for (indent, ry, len) in ROWS {
        let bx = 0.31 + indent;
        over(
            &mut c,
            BULLET,
            cover(
                rounded_rect(x, y, bx - 0.035, ry - 0.035, bx + 0.035, ry + 0.035, 0.015),
                aa,
            ),
        );
        let lx = bx + 0.075;
        over(
            &mut c,
            LINE,
            cover(capsule(x, y, lx, lx + len, ry, 0.022), aa),
        );
    }
    c
}

/// Distancia con signo a un rectángulo redondeado (negativa dentro).
fn rounded_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let cx = (x0 + x1) / 2.0;
    let cy = (y0 + y1) / 2.0;
    let hx = (x1 - x0) / 2.0 - r;
    let hy = (y1 - y0) / 2.0 - r;
    let dx = (x - cx).abs() - hx;
    let dy = (y - cy).abs() - hy;
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    outside + dx.max(dy).min(0.0) - r
}

fn circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> f32 {
    ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r
}

/// Segmento horizontal de x0 a x1 en la altura `cy`, con extremos redondos.
fn capsule(x: f32, y: f32, x0: f32, x1: f32, cy: f32, r: f32) -> f32 {
    let px = x.clamp(x0, x1);
    ((x - px).powi(2) + (y - cy).powi(2)).sqrt() - r
}

/// Cobertura (0–1) a partir de la distancia con signo, suavizada en `aa`.
fn cover(d: f32, aa: f32) -> f32 {
    (0.5 - d / aa).clamp(0.0, 1.0)
}

/// Compone `src` con opacidad `k` sobre `dst` (alfa sin premultiplicar).
fn over(dst: &mut Rgba, src: Rgba, k: f32) {
    let a = src[3] * k;
    if a <= 0.0 {
        return;
    }
    let out_a = a + dst[3] * (1.0 - a);
    for i in 0..3 {
        dst[i] = (src[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

fn lerp(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_the_right_size_and_transparent_corners() {
        for size in [16, 32, 256] {
            let px = rgba(size);
            assert_eq!(px.len(), (size * size * 4) as usize);
            // Esquina superior izquierda: fuera del tablero redondeado.
            assert_eq!(px[3], 0, "esquina no transparente en {size}px");
        }
    }

    #[test]
    fn icon_center_is_opaque_paper() {
        let size = 64;
        let px = rgba(size);
        // Un punto de la hoja sin líneas: entre dos filas, a la derecha.
        let (x, y) = (44, 29);
        let i = ((y * size + x) * 4) as usize;
        assert_eq!(px[i + 3], 255);
        assert!(px[i] > 230 && px[i + 1] > 230 && px[i + 2] > 230);
    }
}
