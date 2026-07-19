# Tasks — Auditoría de código (2026-07-18)

Resultado de la auditoría de bugs, UI/UX, usabilidad y mejoras.
Orden sugerido de trabajo: P0 → P1 → P2 → P3.

---

## P0 — Bugs críticos (corrección funcional)

- [x] **1. Recursión infinita con symlinks / junctions** — `src/scanner.rs:18`, `src/runner.rs:117`
  `build_tree` y `count_files` usan `fs::metadata` / `p.is_dir()`, que **siguen** enlaces simbólicos.
  En Windows existen junctions cíclicas por defecto (p. ej. `C:\Users\<u>\AppData\Local\Application Data`),
  y en Unix un symlink a un ancestro produce recursión infinita → cuelgue o stack overflow.
  **Fix:** usar `fs::symlink_metadata` y omitir (o registrar sin descender) entradas cuyo
  `file_type().is_symlink()` sea true.
  **Criterio de aceptación:** escanear `C:\Users\<usuario>` completo termina sin colgarse.

- [x] **2. El progreso real es ficticio: la barra se queda en 0 % durante el escaneo** — `src/runner.rs:79-84`, `src/scanner.rs:125-127`
  El contador `done` solo se incrementa en `flatten_to_records`, que es una operación
  **en memoria** posterior al escaneo. Todo el trabajo pesado (I/O de `build_tree`) ocurre
  con la barra en 0 %, y luego salta a 100 % en milisegundos. El ETA calculado sobre esa
  fracción no significa nada.
  **Fix:** incrementar el contador dentro de `build_tree` cada vez que se procesa un archivo
  (pasar el `AtomicUsize` al scanner), y quitar el incremento de `flatten_to_records`.
  **Criterio:** en una carpeta con >50k archivos la barra avanza de forma continua durante el escaneo.

- [x] **3. Al lanzar la GUI desde una terminal, se oculta la ventana de la terminal del usuario** — `src/main.rs:61-74`
  `ShowWindow(GetConsoleWindow(), SW_HIDE)` oculta la consola **adjunta**, que al ejecutar
  `file-reporter.exe` sin argumentos desde PowerShell/cmd es la terminal del propio usuario:
  su ventana desaparece y no vuelve.
  **Fix:** antes de ocultar, comprobar con `GetConsoleProcessList` que el proceso es el único
  dueño de la consola (caso doble-clic); si hay más procesos (terminal interactiva), no ocultarla
  (o usar `FreeConsole`).
  **Criterio:** doble clic → sin consola visible; ejecutar desde PowerShell → la terminal sigue visible.

- [x] **4. Los reportes de ejecuciones anteriores se incluyen en el nuevo escaneo** — `src/runner.rs:64-102`
  Por defecto `output == input`, así que `report.csv/md/json` de la corrida anterior aparecen
  como archivos escaneados en la siguiente (y el conteo del progreso también los incluye).
  Además, `create_dir_all(output)` se ejecuta **antes** de escanear, por lo que un output
  dentro del input crea una carpeta que se incluye en el árbol.
  **Fix:** excluir del escaneo los tres paths de salida exactos (o toda la carpeta de salida si
  está dentro del input); crear el directorio de salida después del escaneo, o excluirlo.
  **Criterio:** dos corridas seguidas sobre la misma carpeta arrojan el mismo `files_count`.

- [x] **5. "Tiempo total" sigue creciendo después de terminar** — `src/gui.rs:485-490`
  Al terminar, `compute_eta` muestra `started_at.elapsed()`, que se recalcula en cada repaint:
  si el usuario interactúa 5 minutos después, ve "Tiempo total: 5m…" en vez de la duración real.
  **Fix:** guardar `finished_at` (o la duración) al recibir `WorkerMsg::Done/Failed` y mostrar eso.
  **Criterio:** el tiempo total mostrado queda congelado tras completar.

---

## P1 — Bugs de UX / usabilidad (alto impacto)

- [x] **6. La sincronización input→output captura texto a medio escribir** — `src/gui.rs:216-218`
  El auto-relleno se dispara en cuanto `input_path` no está vacío. Si el usuario **teclea** la ruta
  en vez de usar "Examinar…", al primer carácter (`C`) el output queda fijado en `"C"` y ya no se
  actualiza. Además impide dejar el output vacío (el hint "Por defecto: misma que la entrada"
  nunca se llega a ver porque el campo se rellena solo).
  **Fix:** no auto-rellenar el campo; dejar el output vacío con su hint y resolver el default en
  `start_run` (que ya lo hace en `src/gui.rs:436-440`). Si se quiere mostrar el valor efectivo,
  hacerlo como texto informativo, no escribiéndolo en el campo.
  **Criterio:** teclear la ruta de entrada manualmente no contamina el campo de salida.

- [x] **7. No hay forma de cancelar un escaneo en curso** — `src/gui.rs`, `src/runner.rs`
  Con una carpeta enorme o una unidad de red lenta, la GUI queda en "Procesando…" sin salida
  posible (los campos y botones quedan deshabilitados). El campo `Progress.finished` existe
  pero **nunca se lee** (código muerto).
  **Fix:** añadir un `AtomicBool cancel` a `Progress`, botón "⏹ Cancelar" visible durante
  `Running`, y chequear la bandera en `build_tree`/`count_files` (retornando error "Cancelado").
  Aprovechar para eliminar o usar `finished`.
  **Criterio:** cancelar durante un escaneo grande devuelve la GUI a estado utilizable en <1 s.

- [x] **8. Rutas pegadas con comillas fallan** — `src/gui.rs:435-440`
  "Copiar como ruta de acceso" de Windows pega `"C:\..."` con comillas; el run falla con
  "La ruta no existe" sin pista del motivo.
  **Fix:** aplicar `trim_matches('"')` (además del `trim()` actual) a input y output.
  **Criterio:** pegar una ruta con comillas funciona.

- [x] **9. Sobrescritura silenciosa de reportes existentes** — `src/runner.rs:96-98`
  Los nombres `report.csv/md/json` son fijos: cada corrida pisa la anterior sin avisar.
  **Fix (elegir una):** (a) confirmar sobrescritura en la GUI, o (b) nombres con timestamp
  (`report_2026-07-18_1530.csv`) opcionales. La (b) además elimina parte del problema #4.
  **Criterio:** el usuario no pierde reportes previos sin enterarse.

- [x] **10. Errores de lectura ignorados sin rastro** — `src/scanner.rs:29-32,37`
  Carpetas sin permiso se tratan como vacías y archivos con error se descartan con `.ok()`.
  El usuario ve "Completado" creyendo que el reporte es completo cuando puede faltar media unidad.
  **Fix:** contar entradas omitidas (`AtomicUsize skipped` en `Progress`/`RunSummary`) y mostrar
  en resultados: "⚠ N elementos no accesibles fueron omitidos".
  **Criterio:** escanear una carpeta con subcarpetas sin permiso lo informa en el resultado.

---

## P2 — Mejoras de UI/UX (pulido)

- [x] **11. Doble recorrido de disco: `count_files` duplica todo el I/O** — `src/runner.rs:71-76,117-134`
  El pre-conteo recorre el árbol completo y luego `build_tree` lo vuelve a recorrer: el tiempo
  total casi se duplica solo para tener un ETA. Con la tarea #2 hecha, evaluar contar en un
  hilo/estimación incremental, o aceptar barra indeterminada + contador "N archivos encontrados"
  durante el conteo.
  **Criterio:** el escaneo no lee el disco dos veces (o el costo extra queda justificado y documentado).

- [x] **12. Drag & drop de carpetas** — `src/gui.rs`
  egui expone `ctx.input(|i| i.raw.dropped_files)`. Arrastrar una carpeta a la ventana debería
  rellenar "Carpeta a escanear". Es el gesto más natural para esta app.

- [x] **13. Recordar últimas rutas y extensiones** — `src/gui.rs:22-27`
  eframe trae persistencia (`cc.storage`, feature `persistence`). Restaurar input/output/extensiones
  de la sesión anterior ahorra el paso más repetitivo.

- [x] **14. "Abrir carpeta de salida" debería seleccionar el archivo, y los errores no se reportan** — `src/gui.rs:388-397,580-586`
  Con `explorer /select,<ruta>` se abre la carpeta con el CSV resaltado. Además ambos botones
  ignoran errores (`let _ =`): si explorer falla, el clic no hace nada visible.
  **Fix:** usar `/select,` para "Abrir carpeta", y mostrar el error en la status bar si el spawn falla.

- [x] **15. Tamaños "0.000 MB" para archivos pequeños** — `src/scanner.rs:112`, reportes
  Todo archivo <524 bytes reporta 0.000. Mostrar tamaño adaptativo (B/KB/MB/GB) en columnas
  legibles, o añadir columna `size_bytes` exacta al CSV/JSON para poder ordenar y sumar.

- [x] **16. Enter para ejecutar** — `src/gui.rs:196-266`
  Con el foco en cualquier campo, Enter debería disparar "Generar reportes" si `can_run`.

- [x] **17. Estado "Contando archivos…" mezclado con la fase real** — `src/gui.rs:341-345`
  El texto se decide solo por `total == 0`; con #2 y #11 resueltos, modelar las fases
  explícitamente (Contando → Escaneando → Generando reportes) y mostrarlas en la barra de estado.

- [x] **18. Contraste/estilo en tema claro** — `src/gui.rs:80-83`
  `MUTED` (150,150,150) sobre fondo claro roza el mínimo de contraste, y `ACCENT/SUCCESS/ERROR`
  están calibrados para tema oscuro. Verificar ambos temas (egui sigue el del sistema) y ajustar
  colores por tema (`ui.visuals().dark_mode`).

---

## P3 — Calidad de código y deuda técnica

- [x] **19. Cero tests** — todo el crate
  Nada tiene cobertura. Prioridad de tests unitarios (con `tempfile` como dev-dependency):
  - `scanner::build_tree`: filtrado por extensión, dirs vacíos tras filtrar, orden dirs-primero.
  - `scanner::flatten_to_records`: conteo y campos del `FileRecord`.
  - `runner::run`: input inexistente / no-directorio, corrida feliz genera los 3 archivos.
  - `truncate_path_display` y `fmt_seconds` (`src/gui.rs:555-578`): bordes UTF-8 y 0 s/59 s/1 h.
  - `Cli::normalized_extensions`: puntos y mayúsculas.

- [x] **20. `file-reporter.exe` suelto en la raíz del repo** — raíz
  Binario sin trackear junto al código. Borrarlo y añadir `*.exe` al `.gitignore` (el build
  vive en `target/`).

- [x] **21. Extensiones compuestas y archivos sin extensión no documentados** — `src/scanner.rs:62-66`, `README.md`
  `path.extension()` da `gz` para `foo.tar.gz`, y los archivos sin extensión son imposibles de
  incluir cuando hay filtro activo. Documentar la semántica en README y en el hint de la GUI;
  opcionalmente aceptar un token especial (p. ej. `(sin ext)`) en el filtro.

- [x] **22. Fechas del CSV no ordenables y sin hora** — `src/scanner.rs:113-119`
  `DD/MM/YYYY` no ordena como texto y descarta la hora. Mantenerlo para Excel es válido, pero
  el JSON ya lleva fechas completas; considerar columna adicional ISO-8601 en CSV o dejarlo
  documentado como decisión.

- [x] **23. Recursión profunda puede agotar la pila** — `src/scanner.rs:18-77`, `src/runner.rs:117-134`
  `build_tree` recursa dentro de hilos rayon (pila por defecto ~2 MB). Árboles muy profundos
  (paths largos de node_modules, unidades con anidamiento patológico) pueden reventar la pila.
  Bajo riesgo; mitigar con recorrido iterativo (stack explícito) o documentar el límite.

- [x] **24. Ajustes de release** — `Cargo.toml:20-22`
  Añadir `strip = true` (reduce varios MB del binario). **No** usar `panic = "abort"`: rompería
  el `catch_unwind` del worker de la GUI (`src/gui.rs:456`).

- [x] **25. Dependencias desactualizadas** — `Cargo.toml`
  `eframe/egui 0.28` y `rfd 0.14` tienen varias versiones más recientes con fixes de plataforma.
  Migrar (la API de `Rounding`→`CornerRadius` y `Margin` cambia en 0.29+); hacerlo después de
  las tareas P0/P1 para no mezclar refactor con fixes.
  **Resolución (2026-07-18):** se actualizó a **eframe/egui 0.30 y rfd 0.15**, no a la última
  (0.35), deliberadamente: 0.31+ renombra `Rounding`→`CornerRadius` y cambia tipos de
  `Margin`/`Shadow`, y 0.34+ rediseña el trait `eframe::App` (`update`→`ui`), mueve paneles
  fuera de `egui::` y elimina `Context::style()`/`screen_rect()` — un refactor de arquitectura,
  no una actualización de deps. Se verificó con el compilador que 0.30 compila con cero cambios
  de código y trae los fixes de plataforma. La migración a 0.35 queda como tarea nueva (ver #34).

---

## Ronda 2 — Re-auditoría (2026-07-18, tras completar 1-25)

Verifiqué en el código cada uno de los 25 puntos: **todos están correctamente
implementados** (además: `cargo test` 14/14 y `cargo clippy --all-targets -D warnings`
limpios, re-ejecutados en esta ronda). Los siguientes son hallazgos **nuevos**,
casi todos introducidos por (o expuestos en) los cambios de la ronda 1.

### P1 — Bugs nuevos

- [x] **26. Tras terminar, el panel de progreso muestra un spinner permanente y fuerza repaint continuo** — `src/gui.rs:535-600`, `src/runner.rs:131-132`
  `Progress.writing` se pone en `true` antes de escribir los reportes y **nunca vuelve a
  `false`**, y `self.progress` no se limpia al terminar. Resultado: con status `Done`, el panel
  de progreso sigue mostrando el spinner con "Generando reportes…" (contradice el "🟢 Completado"
  de la status bar y el "Tiempo total" congelado de al lado). Tras `Failed`/cancelación a mitad
  de escaneo pasa lo mismo con "N archivos encontrados…". Peor: `ui.spinner()` pide repaint en
  cada frame, así que la app queda repintando a ~60 fps indefinidamente (CPU/batería) aunque
  esté ociosa.
  **Fix:** en `render_progress`, si `status != Running` no dibujar spinner: mostrar el resumen
  estático (barra al 100 % o solo el tiempo total), o directamente ocultar el panel cuando hay
  `summary`/`error`. Relacionado: durante la fase de escritura el ETA cae en la rama
  `fraction >= 1.0` y ya dice "Tiempo total" aunque siga corriendo (`src/gui.rs:808-813`) —
  mostrar "escribiendo reportes…" o el tiempo transcurrido hasta que llegue `finished_at`.
  **Criterio:** tras completar o cancelar, no queda ningún spinner visible y el uso de CPU en
  reposo es ~0 %.

### P2 — Correcciones menores

- [x] **27. La cancelación del usuario se presenta como error** — `src/gui.rs:687-704`, `src/runner.rs:112-114`
  Al pulsar "⏹ Cancelar", el run termina en `WorkerMsg::Failed("Escaneo cancelado por el
  usuario")` y la UI muestra el panel rojo "❌ Ha ocurrido un error". Cancelar es una acción
  voluntaria, no un fallo.
  **Fix:** distinguir la cancelación (p. ej. variante `Status::Cancelled` o detectar el error de
  cancelación) y mostrarla neutra: "⏹ Escaneo cancelado" en gris/ámbar, sin panel de error.
  **Criterio:** cancelar no muestra rojo ni la palabra "error".

- [x] **28. La exclusión de reportes compara rutas sin canonicalizar (case-sensitive)** — `src/runner.rs:77-85`, `src/scanner.rs:93`
  `excluded.contains(p)` compara `PathBuf` por componentes **sensible a mayúsculas**, pero NTFS
  es case-insensitive. Si el usuario teclea la entrada como `c:\data` y elige la salida con el
  diálogo (`C:\Data`), la exclusión no matchea: los `report.*` viejos vuelven a incluirse en el
  escaneo (reaparece el bug #4) y el check `cfg.output != cfg.input` también se equivoca.
  **Fix:** canonicalizar (`fs::canonicalize`/`dunce`) input y output al armar `excluded` y usar
  las rutas canónicas también para las entradas comparadas, o comparar case-insensitive en Windows.
  **Criterio:** con input `c:\data` y output `C:\DATA`, dos corridas seguidas dan el mismo
  `files_count`.

### P3 — Pulido / decisiones a documentar

- [x] **29. Si la carpeta raíz elegida es un symlink/junction, el escaneo falla con mensaje confuso** — `src/scanner.rs:71-73`, `src/runner.rs:66-71`
  El check anti-ciclos rechaza symlinks en **todos** los niveles, incluida la raíz: elegir una
  carpeta que es junction (OneDrive, unidades mapeadas, `Documents` redirigido) falla con
  "Error al construir el árbol de directorios: Enlace simbólico omitido: …". La raíz la eligió
  el usuario explícitamente: no puede formar un ciclo por sí sola.
  **Fix:** en la raíz, resolver el symlink (`fs::canonicalize`) y escanear el destino; mantener
  el rechazo solo para descendientes.

- [x] **30. Soltar un archivo (no una carpeta) por drag & drop deja una ruta inválida** — `src/gui.rs:381-389`
  El overlay dice "Suelta aquí para escanear", pero si lo soltado es un archivo, `input_path`
  queda apuntando a él y el run falla con "La ruta no es un directorio".
  **Fix:** si el path soltado es un archivo, usar su carpeta contenedora (`parent()`).

- [x] **31. Los botones primarios ignoran el sistema de colores por tema** — `src/gui.rs:482,497,450`
  La tarea #18 introdujo `accent_color()/error_color()` por tema, pero los **fills** de
  "Generar reportes" (`ACCENT_DARK`), "Cancelar" (`ERROR_DARK`) y "Sobrescribir" (`ERROR_DARK`)
  siguen fijos: en tema claro, blanco sobre `rgb(70,130,220)` ronda 3:1 de contraste.
  **Fix:** usar las funciones por tema también en los fills de botones (las variantes `_LIGHT`
  ya existen y cumplen AA con texto blanco).

- [x] **32. `ui_error` queda pegado en la status bar indefinidamente** — `src/gui.rs:213-215,683-685`
  Si "Abrir carpeta" falla una vez, el "⚠ No se pudo abrir…" permanece hasta la próxima corrida
  o "Limpiar", tapando el estado real ("Completado").
  **Fix:** limpiarlo cuando una acción posterior tiene éxito, o auto-expirarlo (p. ej. 5 s).

- [x] **33. El diálogo de sobrescritura no es modal ni responde al teclado** — `src/gui.rs:418-469`
  Con el diálogo abierto se puede seguir editando el formulario de abajo (el `RunConfig`
  capturado en `pending_confirm` queda desactualizado respecto de lo que se ve) y también
  soltar un drag & drop. `Esc` no cancela.
  **Fix:** deshabilitar el formulario y el drop mientras `pending_confirm.is_some()` (o usar
  `egui::Modal` al migrar a egui ≥0.31, ver #34) y mapear `Esc` → cancelar.

- [ ] **34. Migración mayor a egui/eframe 0.35** — `Cargo.toml:16-26`
  Heredada de la resolución de #25. Refactor real: `Rounding`→`CornerRadius`, tipos de
  `Margin`, rediseño del trait `eframe::App` (`update`→`ui`), paneles fuera de `egui::`,
  `Context::style()/screen_rect()` eliminados. Hacerla aislada, en su propia rama/PR, sin
  mezclar con fixes funcionales. Bonus al llegar a 0.31+: `egui::Modal` nativo para #33.

### Verificado y correcto en esta ronda (sin acción)

- Symlinks descendientes omitidos sin recursión (tarea 1) y `skipped` no los cuenta como error — decisión documentada en el docstring.
- Progreso granular real durante el I/O con total desconocido → spinner + contador en vivo (tareas 2, 11 y 17): una sola pasada de disco.
- `hide_console_window` usa `GetConsoleProcessList` con buffer de 2 — patrón correcto (tarea 3).
- `create_dir_all` movido a después del escaneo y reportes propios excluidos (tarea 4) — ver matiz de casing en #28.
- `finished_at` congela el tiempo total (tarea 5) y `Limpiar` resetea todos los campos nuevos.
- Cancelación cooperativa chequeada en cada nodo (tarea 7) — ver presentación en #27.
- `clean_path_input` cubre comillas de "Copiar como ruta de acceso" y se usa en `can_run` y `build_cfg` (tarea 8).
- Confirmación de sobrescritura solo cuando existen reportes previos (tarea 9) — ver modalidad en #33.
- `skipped_count` llega a CLI y GUI con aviso ámbar (tarea 10).
- Persistencia de formulario con `eframe::APP_KEY` sin persistir estado de corrida (tarea 13).
- `spawn` del worker ahora maneja el fallo de creación del hilo (antes `std::thread::spawn` habría panickeado).
- Los 14 tests cubren lo pedido en la tarea 19 y pasan; clippy `-D warnings` limpio.

## Ronda 3 — Re-auditoría (2026-07-18, tras completar 26-33)

Verifiqué en el diff cada fix de la ronda 2: **los 8 están correctamente implementados**
(#34 sigue pendiente, correctamente marcada). `cargo test` 19/19 (incluye los tests nuevos
de junctions en raíz/descendiente, exclusión case-insensitive, `writing` reseteado y drop
de archivos) y `cargo clippy --all-targets -D warnings` limpios, re-ejecutados en esta ronda.

Detalles que confirmé especialmente bien resueltos:
- #26: `running` gatea todas las ramas animadas de `render_progress` **y** el runner resetea
  `writing` — doble protección, con test de regresión.
- #27: la cancelación se detecta por la bandera compartida, no comparando el texto del error —
  robusto ante cambios de mensaje.
- #29: seguir el symlink solo en la raíz no reabre el riesgo de ciclos (un ciclo necesita
  ≥1 enlace descendiente, que se sigue rechazando); el test cubre ambos niveles con junctions
  reales sin necesitar permisos de admin.

Quedan solo residuos menores (P3):

- [x] **35. La comparación case-insensitive de rutas excluidas es solo ASCII y solo Windows** — `src/scanner.rs:15-39`
  Dos limitaciones del fix de #28: (a) `eq_ignore_ascii_case` no pliega caracteres no-ASCII —
  en NTFS (case-insensitive **Unicode**) una ruta con `Ñ`/`á` tecleada con otra caja no
  matchea (p. ej. `C:\AÑO2026` vs `c:\año2026`); (b) la rama case-insensitive es
  `#[cfg(windows)]`, pero APFS/HFS+ en macOS también son case-insensitive por defecto y el
  crate sí contempla macOS (`open_in_explorer`), así que ahí el bug #28 sigue vivo.
  **Fix:** comparar con `to_lowercase()` de Unicode y activar la rama para
  `any(windows, target_os = "macos")`.
  **Criterio:** el test de exclusión pasa con un path que contenga `Ñ` en distinta caja.

- [x] **36. Si falla la escritura de un reporte, `writing` queda en `true`** — `src/runner.rs:141-148`
  Los `?` de `write_csv/markdown/json` retornan antes del `p.writing.store(false)`. Hoy es
  inofensivo (la GUI gatea el spinner con `running`, y `finished_at` corta el ETA), pero es
  una trampa latente: cualquier uso futuro de `writing` sin ese gate revive el bug #26.
  **Fix:** resetear `writing` también en el camino de error (p. ej. envolver la fase de
  escritura en un closure y hacer el `store(false)` antes de propagar con `?`).

### Notas (sin acción, decisiones aceptables)

- Carrera cancelar/terminar: si el usuario pulsa "Cancelar" cuando el escaneo ya está en la
  fase de escritura, la corrida termina en "Completado" (los reportes se generaron completos);
  y si un error real coincide en el tiempo con la cancelación, se muestra como "Cancelado".
  Ambos desenlaces son razonables; no vale la pena complicar el protocolo por ellos.
- Enter no confirma el diálogo de sobrescritura (solo Esc cancela) — defendible: la acción
  destructiva requiere clic deliberado.
- Los symlinks descendientes se omiten sin sumar a `skipped` — decisión documentada en el
  docstring de `build_tree` (omisión intencional, no fallo).

---

## Ronda 4 — Re-auditoría (2026-07-18, tras completar 35-36)

**#35 y #36 verificados y correctos.** `cargo test` 21/21 y `cargo clippy --all-targets
-D warnings` limpios, re-ejecutados en esta ronda. Detalles confirmados:
- #35: `to_lowercase()` Unicode en `cfg(any(windows, macos))`, con test de `AÑO2026` vs
  `año2026`. (El plegado de `to_lowercase` no es idéntico byte a byte a la tabla upcase de
  NTFS en rarezas tipo `ß`/İ turca, pero cubre todo caso realista — no requiere acción.)
- #36: fase de escritura envuelta en closure con reset de `writing` en el único punto de
  salida, y test de regresión que fuerza el camino de error (output bloqueado por un archivo).


Esta ronda incluyó una pasada dedicada a los reporters (la zona menos revisada hasta ahora):
JSON escapa vía serde y el árbol Markdown no puede romper su code fence (los conectores
`├──`/`└──` impiden que un nombre malicioso inicie línea). Un solo hallazgo nuevo:

- [x] **37. Inyección de fórmulas (CSV injection) al abrir el reporte en Excel** — `src/reporters/csv.rs`, `src/scanner.rs:flatten_to_records`
  Los nombres de archivo y rutas escaneados se vuelcan tal cual al CSV. Un archivo cuyo
  nombre empiece por `=`, `+`, `-` o `@` (p. ej. `=cmd|'/c calc'!A1.txt`, o el inocente
  `-informe.txt`) se interpreta como **fórmula** al abrir `report.csv` en Excel — el vector
  clásico de CSV injection: el CSV con BOM y `;` está explícitamente diseñado para abrirse
  en Excel, y el contenido proviene de directorios que el usuario no controla (descargas,
  carpetas compartidas).
  **Fix:** al serializar campos de texto (`name`, `directory`, `full_path`), si empiezan por
  `=`, `+`, `-`, `@`, tab o CR, anteponer `'` (apóstrofo: Excel lo trata como texto literal).
  Hacerlo solo en el CSV (JSON/Markdown no ejecutan fórmulas), idealmente detrás de una
  función `sanitize_for_spreadsheet()` con test.
  **Criterio:** un archivo llamado `=1+1.txt` aparece en Excel como texto, no como `2`.

---

## Notas de la auditoría

Cosas revisadas que **están bien** y no requieren acción:
- El worker captura panics (`catch_unwind`) y rayon propaga panics de `par_iter` al caller, así que la GUI no se cae.
- `Progress::fraction` está protegido contra NaN/fuera de rango.
- El CSV escribe BOM UTF-8 y usa `;` — correcto para Excel es-ES.
- `truncate_path_display` es seguro con UTF-8 (opera sobre chars, no bytes).
- El botón "Generar" se deshabilita durante `Running`, así que no hay corridas concurrentes.
