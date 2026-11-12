# 📋 File Reporter

[![CI](https://github.com/dbarretol/file-reporter/actions/workflows/ci.yml/badge.svg)](https://github.com/dbarretol/file-reporter/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)

**Escanea una carpeta y obtén, en segundos, un inventario completo de su contenido en CSV, Markdown y JSON.**

File Reporter es una herramienta escrita en Rust con **interfaz gráfica** (para uso diario) y **línea de comandos** (para scripts y automatización). Recorre el directorio en paralelo y genera hasta tres reportes con la estructura de carpetas y los metadatos de cada archivo.

```text
$ file-reporter C:\Demo\Proyecto

✅ Reportes generados correctamente:
   📄 CSV:      C:\Demo\Proyecto\report.csv
   📄 Markdown: C:\Demo\Proyecto\report.md
   📄 JSON:     C:\Demo\Proyecto\report.json

📊 Archivos procesados: 4 · 1.3 MB

🗂️  Tipos de archivo (por tamaño):
   .pdf                     1 arch. ·    1.3 MB ·  99.4 %
   .rs                      2 arch. ·    6.4 KB ·   0.5 %
   .md                      1 arch. ·    1.8 KB ·   0.1 %
⏱️  Tiempo: 1.98ms
```

```text
C:\Demo\Proyecto
├── docs
│   └── manual.pdf
├── src
│   ├── utils
│   │   └── helpers.rs
│   └── main.rs
└── README.md
```

---

## Contenido

- [¿Para qué sirve?](#-para-qué-sirve)
- [Características](#-características)
- [Instalación](#-instalación)
- [Uso](#-uso)
- [Reportes generados](#-reportes-generados)
- [Cómo funciona](#-cómo-funciona)
- [Desarrollo](#-desarrollo)
- [Hoja de ruta](#-hoja-de-ruta)
- [Contribuciones](#-contribuciones)
- [Licencia](#-licencia)

---

## 💡 ¿Para qué sirve?

- **Auditar un disco o carpeta compartida:** qué hay, cuánto ocupa y cuándo se modificó, en una hoja de Excel.
- **Documentar la estructura de un proyecto:** pega el árbol de `report.md` en tu documentación.
- **Inventariar tipos concretos de archivo:** por ejemplo, todos los `pdf` y `docx` de una unidad.
- **Alimentar otros procesos:** el JSON jerárquico se integra fácilmente en scripts o pipelines.

---

## ✨ Características

**Reportes**
- 📊 Tres formatos a la vez: **CSV** (listo para Excel), **Markdown** (árbol estilo `tree`) y **JSON** (jerárquico).
- 📅 Metadatos por archivo: nombre, extensión, tamaño (MB y bytes exactos), fechas de creación y modificación, y ruta.
- 🎯 **Filtro por extensiones** (`pdf, docx, jpg…`).
- 📂 **Modo solo carpetas:** genera únicamente la jerarquía de directorios, sin archivos.
- 🗂️ **Resumen por tipo de archivo:** cantidad, tamaño y porcentaje de cada extensión, en la GUI, en la consola y en `report.md`.
- 🚫 **Omite carpetas por nombre** en cualquier nivel (`.git`, `node_modules`, `target`…), sin llegar a leerlas.

**Rendimiento y robustez**
- 🚀 Escaneo **paralelo** con [`rayon`](https://crates.io/crates/rayon), en una sola pasada por el disco.
- 🛡️ Sigue funcionando ante carpetas sin permisos y avisa cuántos elementos se omitieron.
- 🔁 Detecta symlinks y junctions cíclicos para no quedarse colgado.
- 🔒 CSV protegido contra *formula injection*.

**Interfaz gráfica**
- 🖱️ Arrastra y suelta una carpeta sobre la ventana, o usa "Examinar…".
- ✅ Valida las rutas mientras escribes e indica dónde se guardarán los reportes.
- 📈 Contador en vivo y tiempo transcurrido; **cancela** en cualquier momento con el botón o con Esc.
- 📊 Resultado en una ventana propia: archivos, tamaño total, tiempo y tipos de archivo; abre o copia la ruta de cada reporte.
- 🔔 Si la ventana está en segundo plano, parpadea en la barra de tareas al terminar.
- ⚠️ Pide confirmación antes de sobrescribir reportes anteriores.
- ♻️ Recuerda rutas, opciones y tema entre sesiones.
- 🎨 Tema claro, oscuro o el del sistema operativo, a elegir en la cabecera.

---

## 📦 Instalación

### Requisitos

- [Rust](https://rustup.rs/) 1.95 o superior (toolchain estable).
- **Solo en Linux:** dependencias del sistema para la GUI y los diálogos nativos:
  ```bash
  sudo apt install libgtk-3-dev libxkbcommon-dev libwayland-dev
  ```

### Compilar desde el código fuente

```bash
git clone https://github.com/dbarretol/file-reporter.git
cd file-reporter
cargo build --release
```

El ejecutable queda en `target/release/file-reporter` (`file-reporter.exe` en Windows).

> Probado principalmente en **Windows**; también compila en Linux y macOS.

---

## 🚀 Uso

El mismo ejecutable funciona en dos modos:

| Cómo lo ejecutas | Modo |
|---|---|
| Sin argumentos (o doble clic en el `.exe`) | 🖥️ Interfaz gráfica |
| Con una ruta como argumento | ⌨️ Línea de comandos |

### 🖥️ Interfaz gráfica

```bash
./target/release/file-reporter
```

1. Elige la **carpeta a escanear**: escribe la ruta, pulsa "Examinar…" o arrástrala a la ventana.
2. *(Opcional)* Elige una **carpeta de salida**. Por defecto, la misma carpeta escaneada.
3. *(Opcional)* Escribe las **extensiones** a incluir. Las sugerencias se suman a lo escrito; pulsarlas otra vez las quita.
4. *(Opcional)* Revisa las **carpetas a omitir** por nombre. De partida se omiten `.git`, `node_modules`, `target`, `__pycache__`, `.venv`, `dist` y `build`; vacía el campo para incluirlas, o pulsa la sugerencia "Carpetas de proyectos" para quitarlas o volver a ponerlas.
5. *(Opcional)* Marca **Solo carpetas (omitir archivos)** para obtener solo la jerarquía de directorios.
6. *(Opcional)* Desmarca los **formatos** que no necesites (CSV, Markdown, JSON).
7. Pulsa **▶ Generar reportes** (o Enter). Al terminar se abre una ventana con el resultado: los reportes (abrir o copiar su ruta), la carpeta de salida y los tipos de archivo. Ciérrala con **Cerrar**, Enter o Esc; el botón **Ver resultado** la vuelve a abrir.

### ⌨️ Línea de comandos

```bash
# Todo el contenido de una carpeta
file-reporter /ruta/a/escanear

# Solo ciertas extensiones (separadas por coma o con -e repetido)
file-reporter /ruta --ext pdf,docx
file-reporter /ruta -e jpg -e png

# Guardar los reportes en otra carpeta
file-reporter /ruta -o /ruta/reportes

# Solo la estructura de carpetas
file-reporter /ruta -d

# Solo algunos formatos
file-reporter /ruta -f csv,md

# Omitir carpetas por nombre (.git, node_modules, target… con --exclude-common)
file-reporter /ruta --exclude-common
file-reporter /ruta -x "mis fotos" -x tmp,cache
```

En Windows (PowerShell):

```powershell
.\file-reporter.exe "D:\MisDocumentos" -e pdf,docx -o "D:\reportes"
```

| Opción | Descripción |
|---|---|
| `<PATH>` | Carpeta a escanear **(obligatorio)** |
| `-e`, `--ext` | Extensiones a incluir, separadas por coma, punto y coma o espacio (`"pdf, .DOCX"` vale). Sin esta opción se incluyen todas |
| `-o`, `--output` | Carpeta de salida (por defecto: la carpeta escaneada) |
| `-d`, `--dirs-only` | Solo estructura de carpetas. No se puede combinar con `-e` |
| `-x`, `--exclude` | Carpeta a omitir por nombre (no ruta), en cualquier nivel y con todo su contenido. Se puede repetir o separar por comas |
| `--exclude-common` | Omite `.git`, `node_modules`, `target`, `__pycache__`, `.venv`, `dist` y `build` |
| `-f`, `--format` | Reportes a generar: `csv`, `md` (o `markdown`), `json`, separados por coma. Sin esta opción se generan los tres |
| `-h`, `--help` / `-V`, `--version` | Ayuda / versión |

> **Sobre las extensiones:** se usa lo que va tras el **último** punto del nombre. `foo.tar.gz` tiene extensión `gz`, no `tar.gz`. Los archivos sin punto (`LICENSE`, `Makefile`) no tienen extensión y quedan fuera de cualquier filtro.

---

## 📁 Reportes generados

Cada ejecución crea en la carpeta de salida estos archivos (los tres por defecto, o los elegidos con `-f` o en la GUI):

| Archivo | Contenido | Ideal para |
|---|---|---|
| `report.csv` | Una fila por archivo con todos sus metadatos | Excel, Pandas, hojas de cálculo |
| `report.md` | Árbol de carpetas y archivos estilo `tree`, y tabla de tipos de archivo | Documentación, READMEs |
| `report.json` | Árbol completo con metadatos | Scripts e integraciones |

En **modo solo carpetas**, `report.md` y `report.json` contienen únicamente directorios y `report.csv` queda solo con la cabecera.

<details>
<summary><b>📊 report.csv</b>: columnas</summary>

<br>

UTF-8 con BOM y separador `;`, para que Excel lo abra correctamente sin configurar nada.

| Columna | Descripción |
|---|---|
| `name` | Nombre del archivo, sin extensión |
| `extension` | Extensión, sin punto |
| `size_mb` | Tamaño en MB (hasta 3 decimales) |
| `size_bytes` | Tamaño exacto en bytes |
| `created` / `modified` | Fechas en `DD/MM/AAAA` (formato que Excel reconoce directamente) |
| `created_iso` / `modified_iso` | Fecha y hora en ISO-8601, ordenables como texto |
| `directory` | Carpeta que contiene el archivo |
| `full_path` | Ruta completa |

```text
name;extension;size_mb;size_bytes;created;modified;created_iso;modified_iso;directory;full_path
manual;pdf;1.27;1331200;23/10/2026;23/10/2026;2026-10-23T04:42:45.034591400+00:00;...;C:\Demo\Proyecto\docs;C:\Demo\Proyecto\docs\manual.pdf
```

Si un nombre o ruta empieza por `=`, `+`, `-`, `@`, tabulador o retorno de carro, se le antepone un apóstrofo para que Excel no lo ejecute como fórmula.

</details>

<details>
<summary><b>🌳 report.md</b>: árbol de directorios</summary>

<br>

````markdown
# 📋 Estructura de Directorios

```
C:\Demo\Proyecto
├── docs
│   └── manual.pdf
├── src
│   ├── utils
│   │   └── helpers.rs
│   └── main.rs
└── README.md
```

## 📊 Tipos de archivo

| Extensión | Archivos | Tamaño | % del tamaño |
|---|--:|--:|--:|
| .pdf | 1 | 1.3 MB | 99.4 % |
| .rs | 2 | 6.4 KB | 0.5 % |
| .md | 1 | 1.8 KB | 0.1 % |
````

La tabla lista todas las extensiones, de mayor a menor tamaño total; no aparece en modo solo carpetas.

</details>

<details>
<summary><b>🧩 report.json</b>: estructura jerárquica</summary>

<br>

```json
{
  "type": "directory",
  "name": "Proyecto",
  "path": "C:\\Demo\\Proyecto",
  "children": [
    {
      "type": "directory",
      "name": "docs",
      "path": "C:\\Demo\\Proyecto\\docs",
      "children": [
        {
          "type": "file",
          "name": "manual.pdf",
          "path": "C:\\Demo\\Proyecto\\docs\\manual.pdf",
          "size_mb": 1.26953125,
          "size_bytes": 1331200,
          "extension": "pdf",
          "created": "2026-10-23T04:42:45.034591400Z",
          "modified": "2026-10-23T04:42:45.034591400Z"
        }
      ]
    }
  ]
}
```

</details>

---

## 🔧 Cómo funciona

CLI y GUI comparten exactamente la misma lógica; solo cambia la forma de recibir la configuración y de mostrar el progreso.

```text
  CLI (clap) ──┐
               ├──► runner ──► scanner (rayon, en paralelo) ──► árbol en memoria ──► reporters ──► CSV / MD / JSON
  GUI (egui) ──┘       ▲
                       └── progreso, cancelación y elementos omitidos (atómicos compartidos)
```

```text
build.rs             # Windows: incrusta el icono y la versión en el .exe
src/
├── main.rs          # Punto de entrada: decide entre CLI y GUI
├── cli.rs           # Argumentos de línea de comandos (clap)
├── gui/             # Interfaz gráfica (egui/eframe)
│   ├── mod.rs       # Arranque, estado de la app y paneles fijos
│   ├── form.rs      # Formulario y botones
│   ├── run.rs       # Estado de la corrida e hilo de escaneo
│   ├── results.rs   # Progreso y ventana de resultado
│   ├── dialogs.rs   # Diálogo de sobrescritura y arrastrar y soltar
│   └── …            # widgets, paleta de colores, utilidades
├── icon.rs          # Icono de la app, dibujado por código (sin imágenes)
├── runner.rs        # Flujo común: escanear → escribir reportes
├── scanner.rs       # Recorrido paralelo del directorio
├── models.rs        # Node (árbol), FileRecord (fila del CSV) y ExtStat (tipos)
└── reporters/
    ├── csv.rs       # CSV para Excel (BOM + ;)
    ├── markdown.rs  # Árbol estilo tree y tabla de tipos
    └── json.rs      # JSON con formato legible
```

<details>
<summary><b>Detalles de implementación</b></summary>

<br>

- **Una sola pasada por el disco:** no se cuentan los archivos antes de escanearlos. El progreso se reporta en vivo mientras se recorre el árbol.
- **Pocas llamadas al sistema:** el tipo y los metadatos de cada entrada salen de la propia lectura del directorio (en Windows, sin abrir cada archivo); los archivos que no pasan el filtro se descartan antes de leer sus metadatos, y el CSV se escribe directamente desde el árbol, sin una copia intermedia en memoria.
- **Symlinks y junctions:** no se siguen (se detectan con el tipo de la entrada), lo que evita ciclos. La carpeta raíz elegida por el usuario sí se sigue aunque sea un enlace (por ejemplo, carpetas de OneDrive).
- **Errores por elemento:** una carpeta sin permisos se omite y se contabiliza; no detiene el escaneo. Las carpetas omitidas a propósito (por nombre) no cuentan como errores.
- **GUI siempre responsiva:** el escaneo corre en un hilo aparte con pila ampliada y `catch_unwind`, de modo que un panic interno se muestra como error y no cierra la aplicación. La cancelación se distingue de un error real mediante una bandera compartida.
- **Rutas Unicode:** acentos y emojis se conservan en los tres reportes, y la exclusión de los propios reportes compara rutas sin distinguir mayúsculas, también fuera de ASCII.

</details>

### Tecnologías

[`clap`](https://crates.io/crates/clap) (CLI) ·
[`egui`/`eframe`](https://github.com/emilk/egui) (GUI) ·
[`rfd`](https://crates.io/crates/rfd) (diálogos nativos) ·
[`rayon`](https://crates.io/crates/rayon) (paralelismo) ·
[`serde`](https://crates.io/crates/serde) + [`serde_json`](https://crates.io/crates/serde_json) ·
[`csv`](https://crates.io/crates/csv) ·
[`chrono`](https://crates.io/crates/chrono) ·
[`anyhow`](https://crates.io/crates/anyhow) ·
[`winresource`](https://crates.io/crates/winresource) (icono del `.exe`)

---

## 🧪 Desarrollo

```bash
cargo test                          # tests
cargo fmt --check                   # formato
cargo clippy --all-targets -- -D warnings   # lint
cargo run --profile fast            # probar la app: recompila en segundos (sin LTO)
```

`--release` activa LTO, que da el ejecutable más rápido pero tarda minutos en recompilar; el perfil `fast` optimiza casi igual y es el indicado mientras se desarrolla.

Los tests trabajan sobre un sistema de archivos real (carpetas temporales con [`tempfile`](https://crates.io/crates/tempfile)) y cubren escaneo, filtros, exclusión de carpetas, modo solo carpetas, cancelación, estadísticas por tipo, los tres formatos de salida y la protección del CSV. En Windows también crean *junctions* para probar el manejo de enlaces sin necesidad de permisos de administrador.

Cada push a `main` o `dev` y cada pull request pasan por [GitHub Actions](.github/workflows/ci.yml) en Windows y Ubuntu.

---

## 🧭 Hoja de ruta

- [ ] Binarios precompilados en *Releases* e instalador `.msi` ([#15](https://github.com/dbarretol/file-reporter/issues/15)).
- [ ] Barra de progreso en la línea de comandos ([#7](https://github.com/dbarretol/file-reporter/issues/7)).
- [ ] Registro de las rutas omitidas con `tracing` ([#8](https://github.com/dbarretol/file-reporter/issues/8)).
- [ ] Vista previa del árbol antes de exportar ([#13](https://github.com/dbarretol/file-reporter/issues/13)).

---

## 🤝 Contribuciones

Las contribuciones son bienvenidas. Para cambios grandes, abre primero un *issue* para comentar la propuesta. Después, haz un fork, crea una rama desde `dev` y envía un *pull request* contra `dev` describiendo los cambios (`main` solo recibe versiones estables).

---

## 📝 Licencia

Distribuido bajo la licencia **MIT**. Consulta [`LICENSE`](LICENSE).

Desarrollado por [@dbarretol](https://github.com/dbarretol) como proyecto de aprendizaje en Rust. Si te resulta útil, deja una ⭐.
