# Registro de cambios

Los cambios notables de cada versión. El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el proyecto usa [versionado semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Corregido

- **Fechas en hora local:** `report.csv` calculaba `created` y `modified` en UTC, así que fuera de UTC un archivo podía salir con otra fecha que la del explorador de archivos (en UTC-5, todo lo modificado desde las 19:00 aparecía con la fecha del día siguiente). Ahora las fechas usan la hora local del equipo. Las columnas `created_iso`/`modified_iso` y las fechas de `report.json` pasan a llevar el desfase local (`2026-11-11T21:30:00-05:00`) en vez de UTC (`2026-11-12T02:30:00+00:00`): es el mismo instante, y cualquier lector de ISO-8601 lo interpreta igual ([#72](https://github.com/dbarretol/file-reporter/issues/72)).

### Seguridad

- **Dependencias actualizadas** a sus últimos parches compatibles (clap 4.6.7, serde_json 1.0.154, anyhow 1.0.104, chrono 0.4.45 y otras). Quedan resueltos los avisos RUSTSEC-2026-0190 (anyhow), RUSTSEC-2026-0204 (crossbeam-epoch), RUSTSEC-2026-0186 (memmap2), RUSTSEC-2026-0194 y RUSTSEC-2026-0195 (quick-xml) y RUSTSEC-2026-0486 (rustix). Ninguno afectaba en la práctica: la app no usa las funciones afectadas ([#73](https://github.com/dbarretol/file-reporter/issues/73)).

## [0.6.0] - 2026-11-12

### Añadido

- **Elegir el nombre de los reportes:** opción `-n/--name` en la CLI (`-n facturas` → `facturas.csv`, `facturas.md`, `facturas.json`) y, en la GUI, un botón "Guardar con otro nombre" en el diálogo de sobrescritura, que guarda los nuevos como `report-<carpeta>-<AAAA-MM-DD>` (con `-2`, `-3`… si ya existe) y conserva los anteriores. Los reportes con esos nombres no se incluyen al volver a escanear la misma carpeta ([#50](https://github.com/dbarretol/file-reporter/issues/50)).

### Cambiado

- **GUI: formulario más simple:** arriba solo las carpetas a escanear y de salida; extensiones, carpetas a omitir, "Solo carpetas" y formatos pasan a "Opciones", que aparece plegado al abrir la app con un resumen de una línea. Debajo de cada carpeta se ve el final de la ruta, que el campo corta si es larga. Las etiquetas pierden los emoji, poco legibles a ese tamaño ([#51](https://github.com/dbarretol/file-reporter/issues/51)).
- **GUI:** Enter ya no lanza el escaneo mientras se escribe en un campo; los escaneos de menos de un segundo muestran "<1 s" en vez de "0s"; la barra de estado muestra la versión; el diálogo de sobrescritura es un modal como la ventana de resultado, que se cierra con Esc o con un clic fuera ([#51](https://github.com/dbarretol/file-reporter/issues/51)).
- **Menos memoria al escribir `report.md`:** se escribe a medida que se recorre el árbol, en vez de armarlo entero en memoria. Con `C:\Windows` (~119 000 archivos) el pico baja de ~210 MB a ~157 MB; el contenido no cambia ([#54](https://github.com/dbarretol/file-reporter/issues/54)).

### Corregido

- **GUI: la ventana ya no se congela con rutas de red lentas.** La carpeta escrita se comprueba en otro hilo cuando lleva 300 ms sin cambiar; mientras tanto, el botón indica "Comprobando…". Antes, una ruta de red que no responde dejaba la ventana como "No responde" hasta ~27 s ([#54](https://github.com/dbarretol/file-reporter/issues/54)).

## [0.5.0] - 2026-11-12

### Añadido

- **Carpetas omitidas por nombre en el resultado:** la ventana de resultado y la salida de la CLI dicen cuántas carpetas se omitieron por nombre y cuáles, p. ej. "3 carpetas omitidas por nombre (.git, build ×2)". Antes una carpeta `build` o `dist` con contenido propio podía quedar fuera del reporte sin aviso ([#47](https://github.com/dbarretol/file-reporter/issues/47)).
- **GUI: lista de elementos no accesibles:** el aviso "N elementos no accesibles fueron omitidos" se despliega para ver cuáles (hasta 200) y el motivo de cada uno, con un botón "Copiar lista" ([#48](https://github.com/dbarretol/file-reporter/issues/48)).
- **Progreso con las carpetas revisadas:** el contador de la GUI y de la CLI muestra, además de los archivos encontrados, las carpetas revisadas. Con un filtro de extensiones el escaneo ya no parece colgado mientras no aparecen coincidencias. En la GUI el contador también se ve en la barra de estado ([#49](https://github.com/dbarretol/file-reporter/issues/49)).
- **GUI: carpetas recientes:** un botón 🕘 junto a "Carpeta a escanear" ofrece las últimas 5 carpetas escaneadas, que se recuerdan entre sesiones. Las que ya no existen no se muestran ([#52](https://github.com/dbarretol/file-reporter/issues/52)).

### Cambiado

- **Errores de la CLI en una línea:** `❌ <error>: <causa>`, en vez del formato de depuración (`Error: …` y `Caused by:`). El código de salida sigue siendo 1. Las extensiones y carpetas omitidas del encabezado se muestran como lista (`pdf, docx`) en lugar de `["pdf", "docx"]` ([#55](https://github.com/dbarretol/file-reporter/issues/55)).

### Corregido

- **Escritura segura de los reportes:** cada reporte se escribe primero en un temporal (`report.csv.tmp`…) que solo reemplaza al anterior si todo fue bien. Antes, un fallo a mitad de la escritura (disco lleno, `report.csv` abierto en Excel…) dejaba el reporte anterior vacío o a medias, incluso después de confirmar "Sobrescribir". Si el reporte está abierto en otro programa, el error ahora lo sugiere ([#45](https://github.com/dbarretol/file-reporter/issues/45)).
- **GUI: "Abrir carpeta de salida" con comas en la ruta:** con una carpeta como `docs,v2`, el Explorador abría otra ubicación (Documentos). Ahora abre la carpeta correcta con el reporte resaltado ([#46](https://github.com/dbarretol/file-reporter/issues/46)).

## [0.4.1] - 2026-11-12

### Cambiado

- **`size_mb` en `report.csv` usa coma decimal y siempre 3 decimales** (`1,431` en vez de `1.431`). Con el separador `;`, pensado para Excel en español, el punto se leía como separador de miles y Excel mostraba 1431. Si un script leía esa columna como número con punto, puede usar `size_bytes`, que no cambia ([#42](https://github.com/dbarretol/file-reporter/issues/42)).

### Corregido

- **CSV:** la columna `extension` también se protege contra *formula injection*: un archivo `x.=1+1` ya no deja una fórmula en la hoja ([#41](https://github.com/dbarretol/file-reporter/issues/41), [#56](https://github.com/dbarretol/file-reporter/pull/56), gracias a @bfs2021).
- **CSV:** sin archivos que listar (modo solo carpetas o filtro sin coincidencias), `report.csv` conserva la cabecera en vez de quedar vacío ([#43](https://github.com/dbarretol/file-reporter/issues/43)).
- **Markdown:** un nombre con tres acentos graves seguidos ya no cierra antes de tiempo el bloque del árbol ([#44](https://github.com/dbarretol/file-reporter/issues/44)).

## [0.4.0] - 2026-11-11

### Añadido

- **Progreso en la línea de comandos:** mientras escanea, la CLI muestra cuántos archivos (o carpetas) lleva encontrados y el tiempo transcurrido, y después que está escribiendo los reportes. No aparece si la salida se redirige ([#7](https://github.com/dbarretol/file-reporter/issues/7)).
- **`-v/--verbose` en la CLI:** lista en stderr qué se omitió y por qué (carpetas y archivos sin acceso, con el error del sistema; enlaces no seguidos; carpetas excluidas por nombre). `-vv` añade el detalle del proceso. Sin `-v`, el aviso de elementos omitidos sugiere usarlo ([#8](https://github.com/dbarretol/file-reporter/issues/8)).

## [0.3.0] - 2026-11-11

### Añadido

- **Ejecutables precompilados** para Windows y Linux en cada *Release* de GitHub: ya no hace falta instalar Rust para usar la herramienta ([#15](https://github.com/dbarretol/file-reporter/issues/15)).
- **Omitir carpetas por nombre** en cualquier nivel, sin llegar a leerlas: `-x/--exclude` y `--exclude-common` (`.git`, `node_modules`, `target`, `__pycache__`, `.venv`, `dist`, `build`) en la CLI, y campo "Carpetas a omitir" en la GUI, que parte con esa lista ([#4](https://github.com/dbarretol/file-reporter/issues/4)).
- **Resumen por tipo de archivo:** cantidad, tamaño y porcentaje de cada extensión en la consola, en la GUI y como tabla en `report.md` ([#17](https://github.com/dbarretol/file-reporter/issues/17)).
- **Elegir los formatos** a generar: `-f csv,md,json` en la CLI y casillas en la GUI.
- **GUI:**
  - El resultado se abre en una ventana propia, con los reportes (abrir o copiar su ruta), la carpeta de salida y los tipos de archivo; el botón **Ver resultado** la vuelve a abrir.
  - Validación de las rutas mientras se escriben e indicación de dónde se guardarán los reportes.
  - Contador en vivo y tiempo transcurrido; Esc cancela el escaneo.
  - Tamaño total en el resumen ([#5](https://github.com/dbarretol/file-reporter/issues/5)).
  - Parpadeo en la barra de tareas al terminar si la ventana está en segundo plano ([#14](https://github.com/dbarretol/file-reporter/issues/14)).
  - Selector de tema **Sistema · Claro · Oscuro**, recordado entre sesiones ([#11](https://github.com/dbarretol/file-reporter/issues/11)).
  - Las sugerencias de extensiones se suman a lo escrito y, pulsadas otra vez, se quitan.
- **Icono propio** en la ventana, la barra de tareas y el `.exe`, que además muestra nombre y versión en sus propiedades ([#6](https://github.com/dbarretol/file-reporter/issues/6)).

### Cambiado

- **Escaneo más rápido y con menos memoria** (−32 % de tiempo y −29 % de memoria en la medición de referencia): los metadatos salen de la propia lectura del directorio y el CSV se escribe directamente desde el árbol ([#10](https://github.com/dbarretol/file-reporter/issues/10)).
- La GUI usa egui/eframe 0.37; la versión mínima de Rust pasa a **1.95**.
- La barra de progreso de la GUI, que anunciaba un tiempo estimado que nunca se calculaba, se sustituye por el contador y el tiempo transcurrido.

### Corregido

- CLI: las extensiones escritas con espacios tras la coma (`-e "pdf, docx"`) no se filtraban ([#19](https://github.com/dbarretol/file-reporter/issues/19)).
- GUI: los mensajes de error muestran la causa completa, no solo el contexto ([#20](https://github.com/dbarretol/file-reporter/issues/20)).
- GUI: al arrancar con un tema distinto al del sistema, los botones de la barra de título de Windows quedaban casi invisibles.

### Desarrollo

- Perfil `fast` (`cargo run --profile fast`): optimizado sin LTO, recompila en segundos.
- La CI de Windows pasa de ~3 a ~1 minuto.
- La GUI se divide en módulos (`src/gui/`).

## [0.2.0] - 2026-10-22

Primera versión pública.

- CLI y GUI en el mismo ejecutable.
- Reportes en CSV (listo para Excel), Markdown (árbol estilo `tree`) y JSON (jerárquico), con los metadatos de cada archivo.
- Filtro por extensiones y modo solo carpetas.
- Escaneo paralelo en una sola pasada; tolera carpetas sin permisos y enlaces cíclicos.
- CSV protegido contra *formula injection*.

[Sin publicar]: https://github.com/dbarretol/file-reporter/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/dbarretol/file-reporter/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/dbarretol/file-reporter/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/dbarretol/file-reporter/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/dbarretol/file-reporter/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/dbarretol/file-reporter/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dbarretol/file-reporter/releases/tag/v0.2.0
