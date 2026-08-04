# 📋 File Reporter

> Herramienta multiplataforma escrita en **Rust** que escanea recursivamente un directorio y genera reportes en **CSV, Markdown y JSON** sobre su estructura y contenido.

Diseñada para ser **rápida**, **robusta** y **fácil de usar**, ofrece tanto una interfaz gráfica (GUI) amigable como una potente interfaz de línea de comandos (CLI) para integrarse en flujos de trabajo automatizados.

---

## ✨ Características principales

- 🚀 **Alto rendimiento** — escaneo paralelo con [`rayon`](https://crates.io/crates/rayon) para grandes volúmenes de archivos, en una única pasada por disco (sin doble recorrido para contar y luego escanear).
- 🖥️ **Doble interfaz** — GUI nativa (con [`egui`](https://github.com/emilk/egui)) o línea de comandos (con [`clap`](https://crates.io/crates/clap)), compartiendo exactamente la misma lógica de escaneo.
- 📊 **Tres formatos de salida**:
  - **CSV** compatible con Excel (UTF-8 con BOM, separador `;`), protegido contra inyección de fórmulas.
  - **Markdown** con árbol jerárquico estilo `tree`.
  - **JSON** estructurado para integraciones.
- 🎯 **Filtro por extensiones** — analiza solo los tipos de archivo que te interesan.
- 📂 **Modo solo carpetas** — omite los archivos y genera reportes con únicamente la jerarquía de directorios.
- 📅 **Metadatos completos** — nombre, extensión, tamaño en MB y en bytes exactos, fechas de creación y modificación (formato local + ISO-8601), ruta.
- 🛡️ **Manejo robusto de errores** — sigue funcionando aunque haya carpetas sin permisos (y avisa cuántos elementos quedaron fuera del reporte), y detecta symlinks/junctions cíclicos sin colgarse.
- 📈 **Barra de progreso granular con ETA**, que distingue la fase de escaneo de la de escritura de reportes.
- ⏹️ **Cancelable en cualquier momento**, con vuelta a un estado utilizable en menos de un segundo.
- 🖱️ **Drag & drop** de carpetas directamente sobre la ventana.
- ♻️ **Recuerda** las últimas rutas y extensiones usadas entre sesiones.
- ⚠️ **Confirma antes de sobrescribir** reportes de una corrida anterior.
- ⌨️ **Enter para ejecutar** desde cualquier campo del formulario.
- 🎨 **Tema claro/oscuro automático**, siguiendo la configuración del sistema operativo, con contraste ajustado (WCAG AA) en ambos.
- ✅ **Cubierto por tests automatizados** (incluye pruebas con operaciones reales de filesystem vía `tempfile`, y con junctions de Windows para los casos de symlinks).
- 🪟 **Multiplataforma** — Windows, Linux y macOS (probado principalmente en Windows).

---

## 🖼️ Capturas

> *(Añade aquí tus screenshots de la GUI cuando los tengas. Por ejemplo:)*
>
> ![GUI principal](docs/screenshot-main.png)
> ![Reportes generados](docs/screenshot-results.png)

---

## 📦 Requisitos

- **Rust** 1.76 o superior — instalable desde [rustup.rs](https://rustup.rs/).
- **Linux únicamente:** dependencias del sistema para diálogos nativos:
  ```bash
  sudo apt install libgtk-3-dev libxkbcommon-dev
  ```

---

## 🛠️ Compilación

Clona el repositorio y compila en modo **release** para máximo rendimiento:

```bash
git clone https://github.com/tu-usuario/file-reporter.git
cd file-reporter
cargo build --release
```

El ejecutable se generará en `target/release/file-reporter` (o `file-reporter.exe` en Windows).

---

## 🚀 Uso

### Modo GUI (gráfico)

Simplemente ejecuta el binario sin argumentos, o haz **doble clic** sobre el `.exe` en Windows:

```bash
./target/release/file-reporter
```

Aparecerá una ventana donde podrás:

1. Seleccionar la carpeta a escanear (escribiendo la ruta, usando "Examinar…", o arrastrándola directamente a la ventana).
2. Definir una carpeta de salida personalizada (opcional).
3. Especificar extensiones a filtrar, con presets rápidos.
4. Marcar **📂 Solo carpeta (omitir archivos)** si solo quieres la jerarquía de directorios, sin analizar archivos.
5. Pulsar **▶ Generar reportes** (o Enter desde cualquier campo) y monitorizar el progreso en tiempo real, con opción de **⏹ Cancelar** en cualquier momento.

Si la carpeta de salida ya contiene reportes de una corrida anterior, se pide confirmación antes de sobrescribirlos. Las rutas y extensiones usadas se recuerdan automáticamente para la próxima sesión.

### Modo CLI (línea de comandos)

```bash
# Escanear todo el contenido de un directorio
./target/release/file-reporter /ruta/a/escanear

# Filtrar por extensiones (varias formas)
./target/release/file-reporter /ruta -e txt -e jpg -e pdf
./target/release/file-reporter /ruta --ext txt,jpg,png

# Especificar carpeta de salida personalizada
./target/release/file-reporter /ruta -e mp4 -o /ruta/reportes

# Escanear solo la estructura de carpetas (omitir archivos)
./target/release/file-reporter /ruta -d

# En Windows con PowerShell
.\target\release\file-reporter.exe "D:\MisDocumentos" -e pdf,docx -o "D:\reportes"
```

#### Argumentos disponibles

| Argumento | Descripción | Obligatorio |
|-----------|-------------|-------------|
| `<PATH>` | Ruta de la carpeta a escanear | ✅ Sí |
| `-e`, `--ext` | Extensiones a filtrar (separadas por coma o repetidas) | ❌ No |
| `-o`, `--output` | Directorio donde guardar los reportes (por defecto: misma ruta de entrada) | ❌ No |
| `-d`, `--dirs-only` | Solo escanear la estructura de carpetas (omitir archivos) | ❌ No |
| `-h`, `--help` | Muestra la ayuda | — |
| `-V`, `--version` | Muestra la versión | — |

> ⚠️ **Semántica de extensiones:** el filtro usa solo la última parte del nombre tras el
> **último** punto. Para `foo.tar.gz` la extensión es `gz`, no `tar.gz` — filtrar por `tar`
> no lo incluirá. Los archivos sin ningún punto en el nombre (`LICENSE`, `Makefile`, etc.)
> no tienen extensión: quedan **excluidos** de cualquier filtro no vacío, aunque sí aparecen
> cuando el filtro está vacío (se incluye todo).

---

## 📁 Archivos generados

Al finalizar, se generan **tres archivos** en la carpeta de salida:

> 📂 **Modo solo carpetas** (`-d`/`--dirs-only` en CLI, checkbox en GUI): los archivos se
> omiten por completo. `report.csv` queda solo con la cabecera (sin filas), y `report.md`/
> `report.json` muestran únicamente la jerarquía de directorios, sin nodos de archivo.

### `report.csv`

Tabla plana con un registro por archivo, ideal para abrir en Excel o procesar con Pandas:

| Campo | Descripción |
|-------|-------------|
| `name` | Nombre del archivo (sin extensión) |
| `extension` | Extensión (sin punto) |
| `size_mb` | Tamaño en MB con 3 decimales (muestra "0.0" para archivos <524 bytes) |
| `size_bytes` | Tamaño exacto en bytes, para ordenar/sumar sin perder precisión |
| `created` | Fecha de creación (DD/MM/AAAA) |
| `modified` | Fecha de última modificación (DD/MM/AAAA) |
| `created_iso` | Fecha y hora de creación en ISO-8601 (`2024-03-15T14:23:01+00:00`) |
| `modified_iso` | Fecha y hora de modificación en ISO-8601 |
| `directory` | Ruta del directorio padre |
| `full_path` | Ruta completa del archivo |

> 💡 **Compatible con Excel:** UTF-8 con BOM y separador `;` para evitar problemas de codificación y alineación de columnas.
>
> 📅 **Sobre las fechas:** `created`/`modified` usan `DD/MM/AAAA` porque es el formato que
> Excel reconoce sin configuración adicional, pero como texto no ordena cronológicamente y
> descarta la hora. Las columnas `*_iso` llevan fecha+hora completas en un formato que sí
> ordena como texto y es estándar para procesar con scripts.
>
> 🛡️ **Protegido contra CSV injection:** si un nombre de archivo o ruta empieza por `=`, `+`,
> `-`, `@`, tabulador o retorno de carro, se antepone un apóstrofo al escribirlo en el CSV para
> que Excel lo trate como texto literal en vez de evaluarlo como fórmula.

### `report.md`

Árbol jerárquico estilo `tree` listo para incluir en documentación:

````markdown
# 📋 Estructura de Directorios

```
D:\MiCarpeta
├── carpeta1
│   └── archivo1.mp4
└── carpeta2
    ├── archivo2.txt
    └── subcarpeta
        └── archivo3.jpg
```
````

### `report.json`

Estructura jerárquica completa, útil para automatización e integraciones:

```json
{
  "type": "directory",
  "name": "MiCarpeta",
  "path": "D:\\MiCarpeta",
  "children": [
    {
      "type": "file",
      "name": "documento.pdf",
      "path": "D:\\MiCarpeta\\documento.pdf",
      "size_mb": 1.234,
      "size_bytes": 1294237,
      "extension": "pdf",
      "created": "2024-03-15T14:23:01Z",
      "modified": "2024-03-15T14:23:01Z"
    }
  ]
}
```

---

## 🏗️ Arquitectura del proyecto

```
file-reporter/
├── Cargo.toml
└── src/
    ├── main.rs          # Punto de entrada: decide CLI vs GUI
    ├── cli.rs           # Definición de argumentos con clap
    ├── gui.rs           # Interfaz gráfica con egui/eframe
    ├── runner.rs        # Lógica común compartida CLI ↔ GUI
    ├── scanner.rs       # Escaneo paralelo con rayon
    ├── models.rs        # Estructuras de datos (Node, FileRecord)
    └── reporters/
        ├── mod.rs
        ├── csv.rs       # Generación CSV (BOM + ;)
        ├── markdown.rs  # Árbol estilo `tree`
        └── json.rs      # JSON pretty-printed
```

### Diseño modular

- **`runner.rs`** centraliza la lógica de ejecución, permitiendo que CLI y GUI compartan exactamente el mismo flujo. Expone un `Progress` compartido (`Arc<...>` con varios `Atomic*`) que reporta contador de archivos, fase actual (escaneando/escribiendo), elementos omitidos y una bandera de cancelación.
- **`scanner.rs`** usa `rayon` para procesar directorios en paralelo en una única pasada, con manejo robusto de errores por archivo (las carpetas sin permisos se omiten en vez de detener todo el escaneo) y detección de symlinks/junctions vía `symlink_metadata` para evitar recursión infinita en ciclos — salvo en la raíz elegida por el usuario, que sí se sigue.
- **`gui.rs`** ejecuta el escaneo en un hilo separado (con pila ampliada y `catch_unwind` para que un panic interno no tumbe la app) con comunicación vía `mpsc::channel` y el `Progress` compartido, manteniendo la UI 100% responsive; distingue cancelación de error real consultando la bandera compartida, no el texto del mensaje.
- **`reporters/csv.rs`** sanea los campos de texto libre antes de escribirlos para neutralizar CSV injection.

---

## 🧰 Tecnologías utilizadas

| Crate | Uso |
|-------|-----|
| [`clap`](https://crates.io/crates/clap) | Parseo de argumentos CLI |
| [`eframe`](https://crates.io/crates/eframe) / [`egui`](https://crates.io/crates/egui) | Interfaz gráfica nativa |
| [`rfd`](https://crates.io/crates/rfd) | Diálogo nativo de selección de carpetas |
| [`rayon`](https://crates.io/crates/rayon) | Paralelismo data-parallel |
| [`serde`](https://crates.io/crates/serde) + [`serde_json`](https://crates.io/crates/serde_json) | Serialización JSON |
| [`csv`](https://crates.io/crates/csv) | Generación CSV |
| [`chrono`](https://crates.io/crates/chrono) | Fechas y formatos |
| [`anyhow`](https://crates.io/crates/anyhow) | Manejo ergonómico de errores |
| [`tempfile`](https://crates.io/crates/tempfile) *(dev)* | Filesystem real en los tests |

> ℹ️ `eframe`/`egui` están fijados en `0.30` (no la última disponible) a propósito: las
> versiones posteriores traen renombres de API y cambios de tipos en los estilos, además de
> un cambio de firma en el ciclo de render de `eframe::App` (0.34+). Ver el comentario en
> `Cargo.toml` para el detalle.

---

## ✅ Tests

El proyecto cuenta con una batería de tests automatizados (`cargo test`) que cubre escaneo,
filtrado, exclusión de rutas, cancelación, generación de los tres formatos de reporte y
saneamiento contra CSV injection. Varios usan [`tempfile`](https://crates.io/crates/tempfile)
para operar sobre un filesystem real en vez de mockear, incluyendo la creación de *junctions*
de Windows para simular symlinks sin requerir privilegios de administrador.

```bash
cargo test
```

---

## 🐛 Manejo de errores

- **Rutas inválidas o inexistentes:** mensaje de error claro, sin crash.
- **Permisos denegados:** las carpetas inaccesibles se omiten silenciosamente, el escaneo continúa, y el resultado final avisa cuántos elementos quedaron fuera del reporte.
- **Symlinks y junctions cíclicos:** se detectan y se omiten (salvo en la raíz, que se sigue una vez) para evitar recursión infinita.
- **Caracteres especiales en rutas (UTF-8):** soportados correctamente, incluida la comparación case-insensitive de rutas excluidas a nivel Unicode (no solo ASCII) en Windows/macOS; los acentos y emojis se preservan en los tres reportes.
- **Cancelación del usuario:** se distingue explícitamente de un error real (bandera compartida, no texto del mensaje) y se muestra en la GUI como un estado neutral, no como fallo.
- **CSV injection:** los campos de texto libre que empiezan por `=`, `+`, `-`, `@`, tab o CR se neutralizan antes de escribirse en el CSV.
- **GUI:** los errores se muestran en un panel rojo dentro de la ventana, sin cerrar la aplicación. Cualquier panic interno del worker se captura con `catch_unwind`.

---

## 🚧 Posibles mejoras futuras

- [ ] Migración mayor a egui/eframe 0.35+ (ver nota en la sección de tecnologías).
- [ ] Lista configurable de carpetas a excluir (`.git`, `node_modules`, etc.).
- [ ] Toggle manual de modo claro/oscuro (hoy sigue automáticamente el tema del sistema operativo).
- [ ] Tamaño total de archivos en el resumen.
- [ ] Vista previa del árbol antes de exportar.
- [ ] Empaquetar como instalador `.msi` con `cargo-wix`.

---

## 📝 Licencia

Este proyecto se distribuye bajo la licencia **MIT**. Consulta el archivo [`LICENSE`](LICENSE) para más detalles.

---

## 👤 Autor

Desarrollado como proyecto de aprendizaje en Rust.  
*Si te resulta útil, considera dejar una ⭐ en el repositorio.*

---

## 🙋 Contribuciones

Las contribuciones son bienvenidas. Para cambios mayores:

1. Abre un issue describiendo la propuesta.
2. Haz un fork y crea una rama para tu feature.
3. Envía un pull request con descripción clara de los cambios.