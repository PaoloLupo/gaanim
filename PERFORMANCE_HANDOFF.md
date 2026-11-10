# Informe de traspaso: rendimiento y simplificación de gaanim

Documento de trabajo para el agente que implemente las soluciones. No es
documentación del producto y no hace falta fusionarlo en `main`.

## 1. Cómo leer este informe

- **Commit de referencia:** `59fba63`. Todos los números de línea son de ese
  commit. Si el código se ha movido, busca por el nombre de función, que
  siempre acompaña a la línea.
- **Origen:** revisión de solo lectura. No se compiló, ni se ejecutó, ni se
  midió nada. Las magnitudes ("O(escena)", "cuadrático") salen de leer el
  código; ninguna está respaldada por un perfil.
- **Nivel de confianza de cada hallazgo:**
  - **[Comprobado]**: el código citado se releyó de forma independiente
    después de la revisión.
  - **[Una lectura]**: lo vio un solo revisor leyendo la ruta completa.
    Reléelo antes de actuar.
  - **[Sospecha]**: inferido, sin leer toda la ruta. Mídelo o descártalo.
- **Lee `AGENTS.md` antes de empezar.** Fija los comandos (`just dev`,
  `just check-package`, `just test-package`), prohíbe mezclar `cargo` directo
  con las recetas dinámicas, y trata el tiempo de compilación como una
  restricción del proyecto.
- **Líneas base visuales:** nunca uses `--bless` sin aprobación explícita del
  dueño. Un cambio que altere píxeles se entrega con el informe de
  `--diff --example ...` y se espera la aprobación.
- **Mediciones:** usa la skill `plugins/gaanim-dev/skills/gaanim-profile-performance`
  y `just benchmark smoke` (o `standard`). Mide antes y después con el mismo
  perfil; sin eso, no afirmes una mejora de rendimiento.
- **Alcance por entrega:** un paquete de trabajo por rama o PR. Varios tocan
  los mismos archivos gigantes (`pipeline.rs`, `timeline.rs`, `compile.rs`) y
  mezclarlos hace imposible revisar.

## 2. Decisiones que debe tomar el dueño antes de ciertos paquetes

No las des por resueltas; pregunta cuando llegues a ellas.

1. **API pública de Rust.** `Tween`, `PropertyLens`, `TweenState` se exportan
   en `gaanim_animation::prelude` (`crates/gaanim_animation/src/prelude.rs:12`),
   `SnapshotDiff` en `gaanim_timeline::prelude`, y `SceneBuilder` en el prelude
   de `gaanim_api`. Borrarlos o hacerlos privados (WP2, WP8) rompe a quien los
   use desde fuera del repositorio.
2. **Export con ventana.** `ExportConfig::headless = false` es una opción
   pública que ningún llamador del repositorio usa (WP2). Hay que decidir si se
   elimina la opción o se mantiene.
3. **Formato de bundle.** Hacer opcional el digest por fotograma o cambiar qué
   se hashea (WP6) afecta a `Bundle::verify` y puede requerir subir la versión
   del formato `.gaanim`.
4. **Reorganización de crates.** Mover tipos de `gaanim_animation` a
   `gaanim_scene` (WP10) cambia rutas de importación públicas.

## 3. Orden recomendado

| Orden | Paquete | Tipo | Riesgo | Depende de |
|---|---|---|---|---|
| 1 | WP1 `Text.stroke` y bindings duplicados | Bug + limpieza | Bajo | — |
| 2 | WP2 Código muerto | Borrado | Bajo (decisión 1 y 2) | — |
| 3 | WP3 Tests inline a archivos aparte | Mecánico | Muy bajo | — |
| 4 | WP4 Sistemas que no miran si algo cambió | Rendimiento | Bajo-medio | — |
| 5 | WP5 Unificar la extracción del renderer | Arquitectura | Alto (visual) | WP3 ayuda |
| 6 | WP6 Bundle: digest y clones del códec | Rendimiento | Medio (decisión 3) | — |
| 7 | WP7 Seek del timeline | Arquitectura | Alto | WP2, WP4 |
| 8 | WP8 `compile.rs` y la capa de autoría | Arquitectura | Medio | WP3 |
| 9 | WP9 Rutas de export secundarias | Rendimiento | Medio | — |
| 10 | WP10 Grafo de crates | Compilación | Medio (decisión 4) | WP2 |

WP1 a WP3 no cambian comportamiento observable y abaratan todo lo demás;
empieza por ahí. WP5 y WP7 son los de mayor beneficio y mayor riesgo: escribe
primero un diseño corto y valídalo con el dueño.

---

## WP1. `Text.stroke` no acepta `scale_with_object`; setters duplicados en PyO3

**Problema [Comprobado].** `PyText` extiende `PyDrawable`
(`crates/gaanim_python/src/pytext.rs:883`) pero redeclara a mano unos 40
setters de la clase base. Ya hay una divergencia:

- `Text.stroke`: `#[pyo3(signature = (paint, width, *, align=None))]`
  (`pytext.rs:1220-1235`).
- `Drawable.stroke` y la macro de medios:
  `(paint, width, *, align=None, scale_with_object=None)`
  (`crates/gaanim_python/src/pydrawable.rs:3742` y `:4810`).
- El stub declara `scale_with_object` (`crates/gaanim_python/gaanim/gaanim_core.pyi`,
  cerca de la línea 4133 [Una lectura]).

`text.stroke(..., scale_with_object=True)` pasa el type-check y lanzaría
`TypeError` en ejecución.

**Qué hacer.**

1. Arreglo mínimo: que `PyText::stroke` acepte `scale_with_object` y delegue en
   `PyDrawable(...).stroke_impl(paint, width, align, scale_with_object)`, como
   hace `pydrawable.rs:4811-4820`.
2. Limpieza: `same_drawable` (`pydrawable.rs:2434`) ya devuelve el receptor
   conservando su subclase. Comprueba qué overrides de `PyText` son puro
   reenvío y elimínalos; conserva los que hacen algo propio del texto.
3. Red de seguridad: `tests/validate_python_api.py` compara listas de
   parámetros entre stub y código solo para los módulos de Python puro
   (`:2252-2272`); para las clases nativas solo hace `hasattr` (`:4306-4317`)
   [Una lectura]. Extiende la comparación de parámetros a las clases de
   `gaanim_core` para que esta clase de divergencia falle en CI.

**Hecho cuando:** existe un test o validación que falla con la firma antigua y
pasa con la nueva; ningún setter de `PyText` duplica uno de `PyDrawable` sin
motivo.

**Validación:** `just python-develop` y `just validate-python-api`.

**Relacionado, menor [Una lectura]:** hay 847 llamadas manuales a
`ensure_authoring_allowed()?` en `crates/gaanim_python/src`; un método nuevo
que la olvide se salta la comprobación en silencio. No lo abordes aquí;
anótalo como issue.

---

## WP2. Código muerto que sigue ejecutándose o compilándose

**Problema.**

- **Motor `Tween` [Comprobado].** Ningún sitio fuera de
  `crates/gaanim_animation/src/tween.rs` construye un `Tween` (búsqueda de
  `Tween::new`, `Tween {` y consultas `&Tween`/`With<Tween>` en todo
  `crates/`). Aun así, `evaluate_tweens_system` (`tween.rs:527-840`),
  `evaluate_line_path_ranges_system` y el sistema exclusivo
  `evaluate_custom_tweens_system` (`tween.rs:1015`) están registrados y corren
  cada fotograma (`crates/gaanim_animation/src/lib.rs:151-153`). Las
  animaciones reales pasan por clips del timeline y `apply_lens_spec`
  (`crates/gaanim_timeline/src/timeline.rs:4597-5472`).
- **`PropertyLensSpec::to_lens` [Comprobado].**
  `crates/gaanim_timeline/src/clip.rs:567-887` no tiene llamadores
  (`grep '\.to_lens('` vacío). `PropertyLens` (46 variantes) es un espejo de
  `PropertyLensSpec` (54) que solo existe para el motor muerto [Una lectura].
- **`SnapshotDiff` [Comprobado].** `crates/gaanim_timeline/src/snapshot.rs:759-853`:
  solo aparece en su definición y en el prelude.
- **Exportador con ventana [Comprobado].** La rama `if !config.headless` de
  `crates/gaanim_api/src/export.rs:57` es su única entrada. `headless` vale
  `true` por defecto y todos los llamadores del repositorio lo ponen a `true`.
  El código asociado en `crates/gaanim_export/src/exporter.rs` (rangos
  215–237, 320–514, 527–543, 553–694 [Una lectura]) usa `Screenshot` de Bevy,
  intercambio BGRA y `imageops::resize` en CPU, y obliga a las features
  `bevy_winit` y `wayland` en `crates/gaanim_export/Cargo.toml:39`.

**Qué hacer.**

1. Confirma con el dueño las decisiones 1 y 2 de la sección 2.
2. Antes de borrar `tween.rs`, comprueba que `custom.rs`
   (`CustomPropertyLens`, `CustomChannel`, `CustomValues`) no depende del
   sistema exclusivo para los lentes personalizados de Python. Si depende,
   conserva solo esa parte.
3. Borra el motor `Tween`, sus tres sistemas y sus tests; borra `to_lens` y,
   si queda sin uso, `PropertyLens`; borra `SnapshotDiff`.
4. Borra el exportador con ventana y las features de ventana de
   `gaanim_export`; revisa si `gaanim_export` sigue necesitando algo de
   `bevy_window`.
5. Actualiza preludes, `docs/content` y stubs si mencionan lo borrado.

**Hecho cuando:** el workspace compila, los tests de los crates tocados pasan
y `grep` no encuentra referencias a los símbolos eliminados.

**Validación:** `just dev test -p gaanim_animation -p gaanim_timeline -p gaanim_export`
y `just check` una vez, porque cambia API entre crates. `just test-exports` si
tocas `exporter.rs`.

---

## WP3. Sacar los tests inline a archivos aparte

**Problema [Comprobado, cifras medidas con un script].** El 23 % del código de
`crates/` son módulos `#[cfg(test)] mod … { … }` inline: unas 83.000 de 357.000
líneas. Los mayores:

| Archivo | Líneas de test | Total | % |
|---|---|---|---|
| `crates/gaanim_api/src/canvas/compile.rs` | 13.716 | 27.997 | 49 % |
| `crates/gaanim_api/src/canvas/canvas_impl.rs` | 5.631 | 15.130 | 37 % |
| `crates/gaanim_api/src/canvas/visualization.rs` | 3.458 | 8.673 | 40 % |
| `crates/gaanim_renderer/src/three_d.rs` | 3.047 | 8.835 | 34 % |
| `crates/gaanim_timeline/src/timeline.rs` | 2.794 | 8.723 | 32 % |
| `crates/gaanim_renderer/src/pipeline.rs` | 2.440 | 10.339 | 24 % |
| `crates/gaanim_animation/src/updaters.rs` | 1.885 | 5.436 | 35 % |
| `crates/gaanim_bundle/src/lib.rs` | 1.534 | 3.702 | 41 % |

Cargo decide si recompila por la fecha de los archivos que rustc leyó. Con el
test en el mismo archivo, editar un test invalida la build normal de la
librería y la de todo lo que depende de ella: tocar un test de `compile.rs`
recompila `gaanim_api` (107.000 líneas a `opt-level = 2`), `gaanim_python`,
`gaanim_editor` y el enlazado final.

**Qué hacer.** Convertir `#[cfg(test)] mod tests { … }` en
`#[cfg(test)] mod tests;` con el contenido en `foo/tests.rs` (para `foo.rs`).
rustc no carga un módulo externo descartado por `cfg`, así que deja de figurar
entre las dependencias de la build normal.

1. Hazlo primero con un solo archivo (sugerencia: `compile.rs`) y **verifica el
   supuesto**: tras `just check-package gaanim_api`, el archivo `.d` de
   `gaanim_api` en `target/debug/deps/` no debe listar `tests.rs`, y tocar
   `tests.rs` no debe recompilar la librería en el siguiente `check`. Si no se
   cumple, detente e informa.
2. Algunos archivos tienen más de un módulo de test (`timeline.rs` en `:3703`
   y `:5928`) y `cfg(test)` sueltos dentro de código de producción
   (`timeline.rs:3941`); estos últimos se quedan donde están.
3. Es un movimiento puro: sin cambios de contenido, `use super::*` sigue
   funcionando. Pasa `cargo fmt`.
4. Prioriza por tamaño: `gaanim_api`, `gaanim_renderer`, `gaanim_timeline`,
   `gaanim_animation`, `gaanim_bundle`. No hace falta hacerlos todos de una vez.

**Hecho cuando:** los mismos tests pasan (mismo recuento que antes) y el
supuesto del paso 1 está verificado y anotado en la descripción de la entrega.

**Validación:** `just test-package <crate>` por cada crate tocado, comparando
el número de tests antes y después.

---

## WP4. Sistemas por fotograma que trabajan aunque nada haya cambiado

Arreglos locales e independientes entre sí. Cada uno puede ser un commit.

1. **`always_redraw_regen_system` escribe siempre [Comprobado].**
   `crates/gaanim_animation/src/signals.rs:2115-2128` asigna `Path2D`,
   `PathSource` y `LocalBounds` con `get_mut` sin comparar. Eso marca los
   componentes como cambiados cada fotograma, también en pausa, e invalida la
   caché de fragmentos del renderer y el conjunto de restauración del seek.
   `write_path` (`crates/gaanim_animation/src/updaters.rs:2768`) ya usa
   `set_if_neq` [Una lectura]; haz lo mismo aquí. Antes, comprueba si los
   regens sin versión llegan siempre a `updates` o si hay un filtro previo.
2. **`PartialEq` de `Path2D`/`PathSource` [Comprobado].**
   `crates/gaanim_scene/src/components.rs:119-127` derivan `PartialEq` sobre
   `Arc<BezPath>`. `BezPath` no es `Eq`, así que la comparación no ataja por
   puntero y recorre todos los elementos. Escribe `PartialEq` a mano con
   `Arc::ptr_eq` primero. Lo mismo en `FragmentRecipe`
   (`crates/gaanim_renderer/src/fragment.rs:17-22`); `same_path` (`fragment.rs:438`)
   ya lo hace bien [Una lectura].
3. **`surrounding_rect_system` [Comprobado].**
   `crates/gaanim_animation/src/updaters.rs:2231-2239` construye un
   `HashMap` de todos los `MobjectId` cada fotograma aunque no exista ningún
   `SurroundingRect`. Añade una salida temprana o `run_if`.
4. **Mapa `ObjectId → Entity` reconstruido en nueve sitios [Una lectura].**
   `timeline.rs:2126, 2301, 2858, 3159, 3602, 5717`; `snapshot.rs:684, 803`;
   `updaters.rs:2234`. Sustitúyelo por un recurso mantenido con hooks de
   componente de `MobjectId`. Es prerrequisito útil para WP7.
5. **Sistemas exclusivos sin condición [Una lectura].** Unos 35 sistemas
   `fn(world: &mut World)` crean `world.query::<…>()` y un `Vec` cada
   fotograma. `crates/gaanim_animation/src/lib.rs:171-206` encadena 24 con
   `.after()` a mano, contra la regla de `AGENTS.md` de ordenar con `SceneSet`.
   Añade `run_if(any_with_component::<T>)` donde aplique y `QueryState` en
   `Local`.
6. **Geometría derivada recalculada siempre [Una lectura].**
   `resolve_dynamic_clip_masks_system`
   (`crates/gaanim_renderer/src/pipeline.rs:2493-2538`) reconstruye la unión
   de fuentes por cada entidad enmascarada; N glifos bajo una máscara hacen N
   uniones idénticas. Los sistemas de booleanas, nivel de relleno y contorno
   hacen la operación completa cada fotograma (`pipeline.rs:2568`, `:2653`) y
   la descartan si el resultado es igual. Guarda una clave de entrada en un
   `Local` y construye una unión por conjunto distinto de fuentes.
7. **Agregado de límites de grupo [Una lectura].** Con cualquier cambio de
   límites, `hierarchical_bounds_system`
   (`crates/gaanim_scene/src/systems.rs:930-999`) recorre desde todas las
   raíces y `world_bounds_fallback_system` (`:903`) visita toda entidad sin
   `LocalBounds`. Recalcula solo los ancestros de `Changed<WorldBounds>`.
8. **Movimiento procedural: aplicar y deshacer [Una lectura].**
   `apply_procedural_motion_system` y su restauración
   (`crates/gaanim_animation/src/procedural.rs:478-522`) escriben
   `SpatialTransform` y `Opacity` dos veces por fotograma, lo que mantiene
   siempre activa la detección de cambios. Alternativa: un componente de
   desplazamiento aditivo compuesto en `computed_global`, como ya hace
   `ShapeDeform` (`systems.rs:244`). Es el más invasivo de la lista; déjalo
   para el final.

**Hecho cuando:** cada punto tiene un test que demuestra que el sistema no
marca cambios (o no trabaja) en un fotograma sin cambios. Ya existe un test
con esa forma: `playback_seeks_only_rebuild_fragments_of_animating_objects`
(`timeline.rs:7948`).

**Validación:** `just dev test -p gaanim_scene -p gaanim_animation -p gaanim_renderer`.
Los puntos 1, 2 y 6 pueden cambiar qué se re-codifica: pasa el diff visual de
un par de ejemplos con texto, máscaras y `always_redraw`.

---

## WP5. Una sola extracción y una sola secuencia de render

**Problema [Comprobado].** En `crates/gaanim_renderer/src/pipeline.rs` el bucle
que convierte entidades en `ExtractedElement` existe dos veces, mantenido a
mano:

- `gaanim_render_system` (`:6997-7858`, unos 55 parámetros de sistema): preview.
- `extract_world` (`:3643-4264`): export, bundle y capturas headless.

El literal `ExtractedElement { … }` de 35 campos aparece en `:4059` y `:7556`.
La cola de composición (`soft_layers` → `divert_mattes` → `divert_effects` →
`divert_glass` → `compose_frame` → `with_group_shadows`) está escrita tres
veces: `:3421-3477`, `:6633-6673`, `:7721-7814`.

**Divergencia ya existente [Comprobado].** En la ruta headless, la raíz de
efecto excluye un HUD bajo desenfoque de profundidad
(`&& !(depth_blur && under_hud)`, `:3890-3903`). En la ruta de preview esa
condición no está: es solo `effect_query.contains(node)` (`:7351-7356`). Falta
confirmar si el filtro de `effect_query` compensa la diferencia; si no, preview
y export dibujan distinto ese caso. Los márgenes de recorte también difieren
(`:3699` frente a `:7170`) [Una lectura].

**Tres cachés de fragmentos con tres invalidaciones [Una lectura].**
`GaanimRenderCache` (siete `HashMap<ObjectId, _>` paralelos y ticks de cambio
listados a mano, `:380-396`, `:7245-7299`), `ExtractedFragments` (igualdad de
receta, `:3968-3990`) y `FragmentStore` (dirección del `Arc`, `fragment.rs:498`).

**La secuencia de render se repite por host [Una lectura].** Escala, sondas,
fondos, `three_d.prepare`, `effects.render`, `render_to_texture` y
post-proceso están escritos en `crates/gaanim_renderer/src/canvas.rs:804-1001`
(`render_canvas`), `crates/gaanim_export/src/gpu.rs:638-797`,
`crates/gaanim_embed/src/lib.rs:537-584` y
`crates/gaanim_renderer/src/offscreen.rs:72-108`. La sonda de desbordamiento
está implementada dos veces (`canvas.rs:429-579`; `gpu.rs:433-442`, `:663-753`).

**Qué hacer, por etapas.**

1. **Primero la divergencia.** Escribe un test que construya un HUD bajo
   desenfoque de profundidad y compare ambas rutas. Ya hay tests de paridad
   entre `compile_scene_from_world` y la ruta en vivo (`pipeline.rs:8654-8737`)
   que sirven de modelo. Si falla, corrígelo como bug aparte.
2. **Unificar la extracción.** Un `#[derive(SystemParam)]` con las consultas y
   una función `extract_elements()`; la ruta headless la llama con un
   `SystemState` en caché. Elimina además unas 12 construcciones de
   `world.query()` por fotograma (`:3712-3795`, `:4454-4463`).
3. **Unificar la cola de composición** en una función compartida por las tres
   llamadas.
4. **Dejar una sola caché**, la de igualdad de receta.
5. **Recorridos de ancestros.** Cada dibujable sube hasta la raíz entre 8 y 10
   veces por separado (recorte, `opacity_group`, `outermost_effect` ×4,
   `stacked_render_order`, `inherited_z_layer`, `side_of`, `outside_segments`,
   `element_lineage`; `:3843-4121`, `:7308-7611`). Memoriza los datos heredados
   por padre en un mapa por fotograma. Hazlo después del paso 2, para escribirlo
   una vez.
6. **`FrameRenderer::render(&ComposedFrame, target)`** en `gaanim_renderer`,
   usado por los cuatro hosts. Mantén la transformación del embed igual a la
   de los exports (`camera_to_output`), como exige `AGENTS.md`.

**Riesgos.** Es la ruta que produce cada píxel. Cualquier diferencia cambia
las líneas base. `gaanim_embed` no usa la app de Bevy ni egui y compila a
wasm32; el playground compila para Emscripten sin el subsistema de render de
Bevy. Lo compartido tiene que seguir compilando en ambos.

**Hecho cuando:** hay una única construcción de `ExtractedElement`, los tests
de paridad pasan y los diffs visuales no muestran cambios (o los cambios están
explicados y aprobados).

**Validación:** `just test-package gaanim_renderer`; diffs visuales de
ejemplos con efectos, mates, cristal, 3D y transiciones; `just test-exports`;
`cargo check -p gaanim_python --target wasm32-unknown-emscripten` y `just web`
si tocas código compartido con wasm.

**Hallazgos del renderer que pueden ir en entregas aparte [Una lectura].**

- **El reposo se rompe con facilidad.** El salto de rasterizado
  (`canvas.rs:868`) se desactiva con `volatile` (`:862-867`) cuando hay
  post-proceso activo, fondo de shader o una capa de efecto no fija. Los
  efectos de shader y el cristal fijan `fixed: false` (`pipeline.rs:5475`,
  `:5872`). Un editor en pausa con bloom o una tarjeta de cristal rasteriza sin
  parar. Alternativa: post-procesar a una segunda textura y marcar fija una
  capa cuya escena y petición no cambian.
- **`ExtractedElement` se clona entero durante la composición.** Sombras
  (`:2146-2157`), fundidos de grupo (`:2121`), vistas de cámara (`:2354-2359`),
  lados de transición (`:6022-6028`), `picked` (`:1930`) y las funciones
  `divert_*`. Cada clon copia `clip_mask`, que posee un `BezPath` y un
  `Vec<Entity>` (`effects.rs:117-128`), y `lineage`. Usa `Arc<ClipMask>` y
  separa una parte estática compartida de una parte pequeña por fotograma.
- **Blancos 3D de un solo tamaño.** `three_d.rs:3928-3935` recrea las texturas
  MSAA cuando cambia el tamaño; un fotograma con dos tamaños (principal más
  una vista `camera_view_3d`) las realoca cada vez. Indexa por tamaño.
- **`retain` cuadrático** en `pipeline.rs:5623-5627` y `:5878-5882`.
- **Estado de composición en `thread_local` y estáticos**: `GROUP_SHADOWS`
  (`soft_effects.rs:246`), `IMAGES` (`object_effects.rs:597`), `BAND`
  (`vello_target.rs:47`). Un `ComposeContext` explícito desaparece con el
  paso 3.

---

## WP6. Bundle: digest por fotograma y clones del códec delta

**Problema.**

- **Digest [Comprobado en la llamada; coste Una lectura].**
  `SinkState::write` (`crates/gaanim_export/src/bundle.rs:473-475`) llama
  siempre a `gaanim_bundle::frame_digest(&frame, background, &mut self.fragments)`.
  Esa función compone el fotograma entero y hashea todos los flujos de la
  codificación y los bytes de cada imagen
  (`crates/gaanim_bundle/src/lib.rs:2075-2147`, `digest.rs:12-77`). Un ráster
  1080p son unos 8 MB de BLAKE3 por fotograma. Solo lo leen `Bundle::verify` y
  el reempaquetado. Es incondicional, también en el sumidero síncrono de
  Pyodide (`bundle.rs:532-558`).
- **Decodificador delta [Comprobado].** `crates/gaanim_bundle/src/model.rs`:
  en `FRAME_DELTA` clona cada `ElementRecord` a un mapa (`:3472-3476`), otra
  vez al ordenar (`:3493-3501`), y luego `self.previous = Some(frame.clone())`
  (`:3519`). Además clona `previous` para los sub-fotogramas aunque no haya
  ninguno (`:3384-3387`) [Una lectura]. El codificador hace el mismo clon
  incondicional (`:3251-3253`) [Una lectura].
- **Por elemento [Una lectura].** `tables.path(&Arc::new(clip.path.clone()))`
  (`model.rs:2677`) crea un `Arc` nuevo y anula la caché por puntero, así que
  cada elemento recortado se re-codifica y se hashea cada fotograma.

**Qué hacer.**

1. Digest: bandera en `BundleConfig` (apagada para playground y previews) y
   hashear imágenes con el hash por blob que `Tables::image_index` ya calcula
   (`model.rs:571-590`). Requiere la decisión 3.
2. Códec: `Arc<ElementRecord>` en el fotograma anterior y en el mapa; saltar
   el sub-decodificador cuando el recuento es 0; `Arc<BezPath>` en `ClipMask`
   (coincide con el arreglo de WP5).

**Hecho cuando:** el test de ida y vuelta (`crates/gaanim_api/tests/bundle_roundtrip.rs`)
pasa, los bundles existentes en `crates/gaanim_bundle/tests/fixtures` siguen
abriendo y verificando, y hay una medición antes/después de grabación y de
decodificación.

**Validación:** `just dev test -p gaanim_bundle -p gaanim_export -p gaanim_api`;
`just web` si cambias el decodificador.

**Menor [Una lectura]:** `model.rs` tiene 98 funciones `write_*`/`read_*` a
mano y 14 compuertas de versión; cada campo de `ElementRecord`/`FrameRecord`
se lista en la estructura, `capture`, `write`, `read` y `resolve`
(`:2580-3217`). Una macro de lista de campos quitaría la desincronización más
fácil. Serde no sirve: perdería el internado y los deltas.

---

## WP7. El seek del timeline cuesta lo que mide la escena

**Problema.**

- **Un seek por fotograma [Comprobado].** `timeline_playback_system`
  (`crates/gaanim_timeline/src/lib.rs:136-178`) fija `seek_request` en cada
  fotograma de reproducción.
- **`seek_with` es proporcional a la escena [Una lectura].**
  `crates/gaanim_timeline/src/timeline.rs:2175-2817`: examina todos los
  objetos con 28 filtros `Changed<>` (`:2041`), reconstruye el mapa
  `ObjectId → Entity` (`snapshot.rs:679-706`), reaplica cada clip desde la
  base (`clips_in_range`, `:2293`, `:2434`), recorre los clips futuros
  (`:2335`) y reconstruye un mapa de arquetipos por entidad (`:2688`).
- **Un `Updater` desactiva los checkpoints [Comprobado].**
  `checkpoint_times` (`timeline.rs:1808-1820`) devuelve `None` si
  `has_simulated_state(world)`. Esas escenas reproducen desde t=0 en cada
  fotograma: coste cuadrático a lo largo de un export.
- **El seek hacia atrás restaura todo [Comprobado].** `dirty_entities`
  (`timeline.rs:1985-1999`) devuelve `None` cuando
  `target_time < baseline.time`. Afecta al arrastre hacia atrás y,
  según el comentario de `:2210`, a las submuestras de motion blur
  [Sospecha: el lado del export no se leyó].
- **Restauración incondicional [Una lectura].** `entity_mut.insert(snap.transform)`
  e `insert(Opacity(..))` en `snapshot.rs:273-282` hacen repropagar toda la
  jerarquía; etiquetas y pinceles se clonan solo para comparar (`:301-331`).

**Qué hacer, por etapas, midiendo entre cada una.**

1. **Medir primero.** Con la skill de rendimiento, obtén tiempos de seek para
   una escena grande estática con poca animación, y para una escena con un
   updater. Sin esto no se puede justificar el resto.
2. **Índice `ObjectId → Entity` como recurso** (WP4.4).
3. **Seek hacia atrás acotado.** Dentro de la misma base, restaurar solo los
   objetivos de los clips que empiezan en `(target, baseline.time]` más el
   conjunto sucio.
4. **Índice por (objetivo, canal).** `absolute_lens_channel` (`timeline.rs:285`)
   ya clasifica los canales absolutos. Para ellos basta evaluar el último clip
   con `start <= t`, sin restaurar ni reproducir. Mantén restauración y
   repetición para `Ungroup`, `Reparent` y lentes dinámicos.
5. **Updaters.** Estudia si las escenas con updaters pueden usar checkpoints
   guardando el estado simulado en ellos. Es diseño abierto; propónlo antes de
   implementarlo.

**Riesgos.** El seek exacto es la base de los snapshots de regresión visual,
del export determinista y del arrastre en el editor. La etapa 4 cambia el
modelo de evaluación; requiere un diseño escrito y la aprobación del dueño.

**Hecho cuando:** los tests de `gaanim_timeline` pasan, los diffs visuales no
cambian y la medición de la etapa 1 muestra la mejora.

**Validación:** `just test-package gaanim_timeline`; diffs visuales de escenas
con segmentos, updaters, cámara y transiciones; `just benchmark standard`
antes y después.

**Funciones más largas del área, como referencia:** `apply_lens_spec`
(`timeline.rs:4597-5472`, 876 líneas), `seek_with` (643), `camera_rig_system`
(`crates/gaanim_timeline/src/lib.rs:372-666`, 295), `evaluate_delayed_states`
(`timeline.rs:2834-3123`, 290).

---

## WP8. `compile.rs` y la capa de autoría

**Capas [Una lectura].** `PyCanvas` (`Arc<Mutex<SceneModel>>`) → `SceneModel`
graba por segmento un `Vec<Op>` y `Arc<Mutex<ObjectSpec>>` (esta es la
representación intermedia) → `compile.rs` reproduce las operaciones contra
`SceneBuilder` → `Commands` y `Timeline`. No hay dos API de autoría vivas:
Python, ejemplos y tests usan `SceneModel`. `SceneBuilder` es solo el backend
de compilación, aunque es público.

**Problema.**

- **Funciones gigantes [Comprobado en `replay_seg`; resto Una lectura].**
  `crates/gaanim_api/src/canvas/compile.rs`: `replay_seg` `:3863-8748` (4.886
  líneas, 25 parámetros, 105 ramas de `Op`; la rama `LayoutTransition` sola
  mide 1.365 líneas) y `spawn_one` `:10901-13020` (2.120 líneas, 64 ramas de
  `SpawnKind`). `crates/gaanim_api/src/builder.rs`: `play_clip` `:3665-4825`
  (1.161 líneas). Con `opt-level = 2` en dev, cualquier edición re-optimiza
  una función de 5.000 líneas.
- **Un concepto en unos 10 sitios [Una lectura].** `Star` aparece en
  `gaanim_objects::primitives::star`, `builder.rs:10161`, `canvas/types.rs`,
  `canvas/canvas_impl.rs:3933`, `canvas/compile.rs:11268`,
  `canvas/measure.rs:518`, `canvas/camera_view.rs:95`, `canvas/theme.rs:1139`
  y `:1232`, `crates/gaanim_python/src/pycanvas.rs:4649` y el stub. La rama
  de compilación es puro reenvío a `builder.star(...)`.
- **`ObjectSpec` se clona entero [Comprobado en `restyle`].**
  `crates/gaanim_api/src/canvas/drawable.rs:1106-1117`: cada setter de estilo
  hace `let before = spec.clone(); f(&mut spec); (before, spec.clone())` para
  comparar seis campos. Cada `Op::Spawn` clona la especificación cuatro veces
  (`compile.rs:3928-3968`) [Una lectura]. Las mallas son `Vec` planos dentro
  de la especificación (`canvas/types.rs:707-737`).

**Qué hacer.**

1. Agrupa los 25 parámetros de `replay_seg` en un `CompileCursor` y mueve cada
   rama a una función en el módulo de su característica. Movimiento puro, sin
   cambio de comportamiento; hazlo después de WP3 para no arrastrar 13.700
   líneas de tests.
2. Sustituye las formas estáticas (unas 25) por una rama
   `SpawnKind::Shape(ShapeSpec)` que llame a `gaanim_objects` directamente. No
   hace falta un registro de traits.
3. Que `restyle` compare una estructura pequeña de estilo en lugar de clonar
   `ObjectSpec`; cargas pesadas como `Arc<[T]>`.
4. Treemap, sankey y grafos ya se construyen sobre la API de canvas sin tocar
   `compile.rs` (commit `2e2e254`). Es el patrón a seguir para lo nuevo.

**Hallazgos de la recarga incremental [mecánica Una lectura; magnitud Sospecha].**

- La granularidad es el segmento (`crates/gaanim_api/src/runtime.rs:771`,
  `.filter(|&index| index > 0)`): un script sin segmentos explícitos siempre
  recompila entero.
- Un checkpoint copia todo el estado de compilación (`compile.rs:3086-3116`,
  `builder.rs:1417`) y se copia otra vez al reanudar (`runtime.rs:895-912`).
- Las huellas pasan por formato `Debug`
  (`crates/gaanim_core/src/fingerprint.rs:63`), así que cada float de una
  malla se formatea en cada recarga.
- Los snapshots de cámara reconstruyen toda la escena en cada recarga con
  cambios (`runtime.rs:485-490`, `:144-156`), solo si la escena tiene
  `Op::SetCameraSnapshot`.

**Hallazgos menores [Una lectura o Sospecha].**

- `freeze_spawn_specs` (`canvas/ops.rs:223`) recorre todos los objetos en cada
  `play` y `wait`. Usa una lista de "creados desde la última congelación".
- `compile.rs:4056-4063` llama a `fade_in_is_entry` (`:9229`) por cada
  objetivo de fundido en cada creación a mitad de segmento.
- Tres `Mutex` anidados para una autoría de un solo hilo (unas 670 llamadas
  `.lock().expect("… poisoned")`), que obligan a `try_lock`
  (`canvas/measure.rs:128`) y a detectar `<locked>`
  (`canvas/incremental.rs:250-294`). Refactor grande, poca urgencia.
- `mesh(vertices: Vec<[f64; 3]>, …)` (`crates/gaanim_python/src/pycanvas.rs:5932`)
  extrae elemento a elemento. `crates/gaanim_python/src/field.rs` ya usa
  bytes de numpy; reutilízalo.

**Validación:** `just test-package gaanim_api`, `just validate-python-api`,
`just check-examples` y diffs visuales de una muestra de ejemplos.

---

## WP9. Rutas de export secundarias

**Problema [Una lectura, salvo donde se indica].**

- **Tres bucles de fotogramas, solo uno optimizado.**
  `crates/gaanim_export/src/exporter.rs`: `export_scene_direct` (`:702-962`),
  `export_bundle` (`:969-1063`), `capture_scene_direct_streaming`
  (`:1490-1661`), más `record_bundle` en `crates/gaanim_export/src/bundle.rs:571-776`.
  El montaje de `FrameJob`, la preparación de la app y el promediado de
  desenfoque están copiados entre ellos.
- **Bundle → MP4 sin NV12 [Comprobado en la bandera].** `export_bundle` usa
  `nv12_input: false` (`exporter.rs:1031`) y una lectura bloqueante con copia
  por fotograma (`crates/gaanim_export/src/gpu.rs:822-827`).
- **Motion blur en CPU.** `exporter.rs:1757-1805` y `LinearAverage`
  (`:1811-1883`) leen cada submuestra como RGBA y suman en `f32`.
  `scene_has_motion_blur` (`:1678-1685` [Comprobado]) desactiva NV12 para todo
  el export aunque solo desenfoque una ventana.
- **Decodificadores de vídeo deterministas indexados por ruta y sin límite.**
  `crates/gaanim_media/src/lib.rs:847`, `:940-968`: dos clips del mismo
  archivo en posiciones distantes reinician FFmpeg en cada fotograma
  exportado. La ruta en tiempo real indexa por entidad y limita a 4
  (`:658-662`).
- **Escritura sin búfer.** `MappedFrame::write_to` (`gpu.rs:967-975`) hace una
  escritura al pipe por fila cuando el ancho no es múltiplo de 64. Envuelve el
  stdin de FFmpeg en un `BufWriter`.

**Qué hacer.** Un único conductor sobre un `FrameSource` (mundo o bundle) que
produce `FrameJob`, con el codificador o un callback como sumidero. Acumular
el desenfoque en un blanco flotante en GPU, con una lectura por fotograma de
salida y NV12 activo. Un mapa de sesiones de vídeo indexado por entidad y con
límite para ambos modos.

**No tocar:** la ruta directa ya está bien afinada (ver sección 5).

**Validación:** `just test-exports` (necesita FFmpeg y ffprobe) y
`just benchmark standard` antes y después.

---

## WP10. Grafo de crates casi lineal

**Problema [Comprobado en los manifiestos y en el recuento de usos].** La
cadena de dependencias es
`core → math → scene → objects → text → animation → {renderer, timeline} → bundle → export → api → python/editor`.
Casi no hay ramas, así que Cargo apenas compila crates del workspace en
paralelo y un cambio abajo recompila todo en serie.

- `gaanim_renderer` depende de `gaanim_animation` por unos 25 símbolos, casi
  todos datos: `PlaybackState` (11 usos), `FloatSignal` (6), `StrokeTip` (5),
  `AmbientClock` (5), `LiveOverlay`, `ScalarSource`, `TrackingScalar`,
  `TrackingEndpoint`, `ParticleEmitter`, `FillDrawProgress`, `EchoGhost`,
  `PathReveal`, y dos sistemas (`tracking_line_system`,
  `surrounding_rect_system`).
- `gaanim_animation` depende de `gaanim_text` (y con él, de Typst) para dar
  forma a lecturas numéricas: `polls.rs`, `rolling_number.rs`, `signals.rs`,
  `text_motion.rs`.
- `gaanim_api` depende de `gaanim_export` por tres llamadas: `export_scene`,
  `record_bundle` y `recording_app` (`crates/gaanim_api/src/runtime.rs:41`,
  `:145`), más `ExportError`.
- `gaanim_embed` (wasm) compila Typst por la vía
  `renderer → animation → text`, aunque no lo use.

**Qué hacer.**

1. Mide primero: `just build-timings` y guarda `target/cargo-timings/cargo-timing.html`
   como referencia.
2. Mueve a `gaanim_scene` los componentes de datos que el renderer lee de
   `gaanim_animation`, y elimina la dependencia `renderer → animation`.
3. Mueve `recording_app` y `SetupCallback` hacia abajo y los envoltorios de
   export hacia los hosts, y elimina `api → export`.
4. Estudia si `animation → text` puede invertirse con un recurso que
   `gaanim_text` registre.

**Nota:** `gaanim_timeline → gaanim_renderer` es solo `dev-dependency` (un test
en `timeline.rs:7951` y un ejemplo). No hay nada que invertir; como mucho,
mover ese test al crate del renderer.

**Restricciones (de `AGENTS.md`).** `gaanim_engine` debe seguir fuera de los
miembros del workspace; `gaanim` y el plugin se compilan en una sola
invocación; no bajes `opt-level` de dev por el límite de 65.535 exportaciones
de una DLL en Windows.

**Validación:** `just check` y `just build`; comparar los tiempos del paso 1.

---

## 4. Riesgo a vigilar, sin acción propuesta

`gaanim_engine` es una `dylib` de Rust que comparten el ejecutable y el plugin
de Python, que se pasan tipos de Rust (`SceneModel`, canales, callbacks) por
esa frontera (`crates/gaanim_editor/src/python_plugin.rs`). El diseño tiene
motivo: `gaanim` arranca sin Python. El coste es que dev no puede bajar de
`opt-level = 2`, y el límite de exportaciones de Windows crece con cada crate
y cada instancia genérica. No se propone cambiarlo; conviene vigilar el
recuento de exportaciones.

## 5. Lo que está bien: no lo rehagas

- **Export directo:** anillo de buffers de staging, pipeline de un fotograma,
  NV12 en GPU, buffer mapeado escrito directo al stdin de FFmpeg por el hilo
  del codificador, canal acotado, reutilización de fotogramas en espera
  (`gpu.rs:562-609`, `:802-866`; `nv12.rs`; `encoder.rs:504`, `:930`).
- **Clave de la caché de fragmentos:** excluye transformación y opacidad, así
  que trasladar, rotar o fundir no re-codifica. Escalar un trazo re-codifica
  por diseño (`stroke.rs:87`).
- **Ruta GPU del 3D:** subidas indexadas por dirección de origen, fotogramas
  saltados por digest, pipelines perezosos.
- **Texto:** la salida de Typst tiene caché LRU
  (`crates/gaanim_text/src/typst_compiler.rs:225`) y las lecturas numéricas
  se componen con glifos en caché.
- **Timeline:** los clips son datos planos en un `SlotMap` con índice
  `BTreeMap`, no una entidad por tween; las escrituras de `apply_lens_spec`
  comparan antes de escribir; los snapshots comparten `Arc<BezPath>`.
- **Bundle:** seek por trozos de 60 fotogramas con tres en caché; zstd nivel
  15 medido; escritor en su propio hilo con cola acotada.
- **Editor:** el modo de reposo ya usa `reactive_low_power`
  (`crates/gaanim_editor/src/idle.rs`).
- **Compilaciones dobles:** las sondas de cámara y de snapshots solo se
  disparan en escenas que usan la característica.
- **Lecturas de GPU en preview:** no hay ninguna bloqueante; la sonda usa
  `PollType::Poll` un fotograma después (`canvas.rs:466`).
