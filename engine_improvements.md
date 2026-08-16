# Gaanim — estado actual y plan de evolución

Gaanim es un motor de escenas vectoriales 2D y composiciones 3D acelerado por GPU,
escrito en Rust sobre Bevy ECS, Vello y wgpu. Su objetivo de producto es permitir crear
videos explicativos, contenido educativo, piezas para redes y presentaciones animadas
mediante una API programática en Python, manteniendo un núcleo reutilizable desde Rust.

> [!NOTE]
> Última actualización basada en el repositorio: **2026-08-16**.
> `Scene` es la fachada pública de Python; `scene.canvas` contiene la configuración del
> viewport y `Canvas(...)` se mantiene como constructor de compatibilidad deprecado. La
> API pública ya cubre escenas 2D, una ruta 3D técnica y modelos glTF, pero no toda la
> capacidad interna de Rust. `Transform`, `ReplacementTransform`, matching de formas/texto,
> cámara 3D y acciones glTF tienen implementación y ejemplos/documentación asociados.
> Novedades de este corte: partes de texto/ecuaciones animables (`parts(...)`), coloración
> de partes en `scene.play`, lentes de cámara atómicos (`CameraLookAt`/`CameraOrbit`),
> reveal determinista de paths y líneas 3D nativas, `at()` que acepta otros drawables, y
> un editor simplificado cuyo panel de timeline con tracks/clips fue retirado.

---

## Resumen ejecutivo

**Estado de producto: alfa funcional (`gaanim` 0.1.0).**

Gaanim ya puede construir, previsualizar y exportar animaciones vectoriales 2D y escenas
3D con formas, texto, fórmulas matemáticas, composición temporal, segmentos, transiciones,
assets, cámaras y comportamientos reactivos. La API Python también importa modelos glTF,
selecciona sus nodos y programa sus acciones; el workspace separa escena, animación,
timeline, renderer, texto, layout, exportación, proyectos, editor, API y bindings.

La base técnica es más madura que la experiencia de librería. La API de Python actual
expone solo una parte de lo que existe en Rust, pero `Scene` ya es la fachada pública que
posee mobjects, timeline, reproducción, presentación y exportación. La documentación y
los ejemplos usan `Scene`; `Canvas` queda limitado a la configuración accesible como
`scene.canvas` y a un constructor de compatibilidad deprecado. El flujo principal está
orientado a ejecutar scripts dentro de la aplicación `gaanim`, aunque también existe una
wheel local para exportaciones headless sin el visor interactivo.

En términos prácticos:

- **Sí es viable hoy** para prototipos y piezas vectoriales programáticas: títulos,
  diagramas, fórmulas, explicaciones matemáticas, assets SVG/raster y animaciones cortas
  con audio mezclado al exportar.
- **Es una beta técnica** para visualización 3D: superficies, ejes, polilíneas con
  colormap, cámara perspectiva, HUD/billboards y modelos glTF con acciones animadas.
- **Es beta funcional** para presentaciones interactivas: existen segmentos y paradas
  semánticos, layouts, notas, Presenter View, overview, navegación por monitor y una
  plantilla completa de sustentación.
- **Todavía no está listo** como pipeline general de producción de contenido: faltan
  preview de audio, video embebido, más gráficos de datos, plantillas y una distribución coherente.
- **Todavía no está listo** como librería pública estable: API, documentación, ejemplos,
  stubs, versionado, pruebas de integración y empaquetado deben converger.

### Semáforo por caso de uso

| Caso de uso | Estado | Evaluación actual |
|---|---:|---|
| Animación vectorial 2D programática | 🟢 | Núcleo funcional con preview y exportación |
| Visualización 3D programática | 🟡 | Ejes, superficies, líneas, cámara y transforms públicos; faltan cobertura y pulido |
| Modelos glTF animados | 🟡 | Importación, partes, materiales PBR y acciones; faltan más formatos y E2E |
| Contenido matemático/educativo simple | 🟢 | Texto y ecuaciones Typst con partes animables (`parts`) son la fortaleza diferencial |
| Videos cortos para redes | 🟡 | Viable si audio, imágenes y montaje se hacen fuera de gaanim |
| Presentaciones animadas en vivo | 🟢 | Segmentos semánticos, notas, Presenter View y overview verificados |
| Contenido con código, tablas o datos | 🟡 | `table`, `bar_chart` y `code` existen; faltan charts, highlighting y APIs de datos más ricas |
| Motion graphics con multimedia | 🟡 | Raster, SVG vectorial avanzado y audio de exportación disponibles; falta video embebido |
| Pipeline audiovisual de producción | 🔴 | Faltan preview de audio, video, pruebas E2E, empaquetado y estabilidad de API |

---

## Evidencia de la auditoría

- `cargo check --workspace` finaliza correctamente.
- El workspace contiene 18 crates de gaanim más el crate de documentación (19 miembros).
- El paquete declara versión Python `0.1.0`; varios crates Rust siguen en `0.1.0`.
- La API pública de Python exporta `Scene`, `Drawable`, `Anim`, `Transition`, `Color`,
  `Brush`, `Camera`, `Theme`, layouts, `ValueTracker` y `Segment`, además de primitivas 3D
  y glTF. `Canvas(...)` emite una deprecación y devuelve un `Scene`; la configuración
  visual vive en `scene.canvas`.
- Los ejemplos Python no importan `Canvas` como fachada de animación.
- La documentación Typst usa `Scene`, listas en `scene.play([...])` y métodos expuestos
  por los bindings actuales.
- CI ejecuta formato, check, tests y Clippy en Windows y Linux, además de regresiones
  visuales de transform, imagen, SVG y cámara en Windows, validación de ejemplos y
  construcción de la wheel Python.
- La auditoría heurística del repositorio no reporta hallazgos objetivos en este corte;
  la verificación final debe volver a ejecutarse después de cada cambio de código.
- El test de aceptación glTF usa un asset real (`Fox.glb` con acciones Survey/Walk/Run)
  y verifica jerarquía, geometría, bounds finitos y duraciones.
- Hay tests que fijan determinismo puntual: el seek de orbit publica la misma pose atómica
  en las cámaras authored/rig/resolved, el reveal de progreso cero produce geometría
  vacía, y `follow_lag` es bitwise-idéntico entre evaluación incremental y rewind.

---

## Decisión de API: deprecar `Canvas` como fachada pública

**Decisión aprobada el 2026-07-13 y aplicada en 0.1.x:** `Scene` es el punto de entrada
público de Python. `Canvas` dejó de representar toda la animación y quedó reservado para
el viewport y su configuración visual.

La responsabilidad objetivo de cada concepto es:

| Concepto | Responsabilidad |
|---|---|
| `Scene` | Mobjects, animaciones, timeline, reproducción, segmentos y exportación |
| `Canvas` | Dimensiones, fondo, márgenes, coordenadas, aspect ratio y safe areas |
| `Video` | Composición futura de múltiples escenas y transiciones |
| `Presentation` | Composición futura de slides, navegación y presenter mode |

API objetivo mínima:

```python
scene = Scene(width=1920, height=1080, background=BLACK)

title = scene.text("Gaanim", role="title")
scene.play([title.write()])
scene.export("video.mp4")
```

Internamente, la relación deberá poder evolucionar hacia:

```text
Video / Presentation
└── Scene
    ├── Canvas
    ├── Mobjects
    └── Timeline
```

No hay releases públicas ni entornos externos de prueba que requieran compatibilidad con
la fachada `Canvas`. El constructor de compatibilidad sigue disponible y emite una
deprecación para facilitar la migración de scripts locales; no debe aparecer en ejemplos
nuevos y debe eliminarse antes de 1.0.

Las menciones a `Canvas` en las secciones de estado actual describen el código que existe
hoy; no representan el diseño objetivo de la API.

---

## Arquitectura actual

| Capa | Crates principales | Estado |
|---|---|---:|
| Tipos base, color y temas | `gaanim_core` | ✅ Funcional |
| Matemática, transforms, cámara y easing | `gaanim_math` | ✅ Funcional |
| ECS, componentes y jerarquía | `gaanim_scene` | ✅ Funcional |
| Tweens, escritura, señales y updaters | `gaanim_animation` | ✅ Funcional |
| Clips, snapshots, seek, escenas y transiciones | `gaanim_timeline` | ✅ Funcional, con coste de seek por optimizar |
| Render 2D vectorial y 3D nativo | `gaanim_renderer`, `gaanim_objects` | 🟡 Vello retained + Bevy/PBR; falta cobertura E2E |
| Primitivas, glTF y texto semántico | `gaanim_objects`, `gaanim_text` | ✅ Base sólida; glTF es reciente |
| Posicionamiento y distribución | `gaanim_layout` | ✅ Funcional en Rust; exposición parcial |
| API canónica | `gaanim_api` | 🟡 Cobertura Python creciente; contrato aún en consolidación |
| Binding Python embebido | `gaanim_python` | 🟡 Funcional dentro de la aplicación |
| Proyectos y detección de entornos | `gaanim_project`, `gaanim_launcher` | ✅ Scaffolds `video`/`slides`, manifiesto y diagnóstico |
| Preview, hot reload y timeline visual | `gaanim_editor` | 🟡 Útil, todavía no es un editor integral |
| Exportación de frames y video | `gaanim_export` | 🟡 Implementada, necesita validación E2E |
| Comparación visual | `gaanim_diff` | ✅ Snapshots deterministas y reportes; cobertura selectiva |

La dirección arquitectónica es correcta: Python construye una descripción diferida de
`Scene`, la aplicación la recompila a ECS y el mismo timeline se usa para reproducción,
seek, snapshots y exportación. Las escenas 2D siguen el camino Vello retained; el contenido
3D activa la ruta nativa de Bevy y debe convivir con overlays Vello como HUD y billboards.
El principal riesgo ya no es la separación de capas, sino mantener una sola API de producto
coherente y una exportación determinista para ambas rutas.

---

## Capacidades disponibles hoy

### 1. Construcción de escenas desde Python

La API pública actual permite:

- crear un `Scene` con tamaño, fondo y margen uniforme;
- crear `circle`, `rect`, `rounded_rect`, `square`, `dot`, `ellipse`, `line` y `arrow`;
- crear `text` unificado (roles, prosa y matemática `$...$`) e `image` (PNG/JPEG/WebP);
- crear paths, arcos, polilíneas, ejes 2D/3D, superficies y primitivas técnicas;
- importar SVG vectorial y modelos glTF/GLB, consultar partes y acciones del modelo;
- colocar objetos en 3D con cámara perspectiva, `billboard` y overlays `hud`;
- agrupar objetos;
- configurar fill, stroke, opacidad y z-index;
- posicionar con `at` (coordenadas o una referencia a otro drawable), `at_anchor`,
  `next_to`, `align_to`, `to_edge` y `to_corner`;
- dividir el contenido en segmentos y enlazarlos con transiciones;
- ejecutar la escena en la aplicación o exportarla por extensión de archivo.

El núcleo Rust contiene más primitivas y helpers —polígonos, estrellas, gráficas,
braces, operaciones booleanas y layouts de grupo—. La API Python ya expone una
superficie amplia y documentada, pero cada capacidad nueva debe seguir validándose
desde el binding, el stub y un ejemplo ejecutable antes de considerarse estable.

### 2. Animación y composición temporal

Desde Python están expuestas animaciones de:

- movimiento, posición, escala y rotación;
- fade in/out y cambio de opacidad;
- `Write`, `Create`, `Unwrite` y `Uncreate`;
- crecimiento/encogimiento desde el centro;
- `SpinInFromNothing`, `DrawBorderThenFill`, `Indicate` y `Wiggle`;
- `FadeTransform`;
- `Transform` y `ReplacementTransform`;
- `TransformMatchingShapes`, `TransformMatchingText` y `TransformMatchingTex`;
- `move_along_path` y transforms 3D (`move_3d`, `move_to_3d`, `rotate_*_3d`, `scale_to_3d`);
- animaciones de cámara 3D `look_at` y `orbit` compiladas como un único clip atómico
  (`CameraLookAt`/`CameraOrbit`): posición, objetivo y rotación dejan de ser tres clips
  independientes que podían desincronizarse en seek, y el orbit interpola conservando el
  radio eye-target durante toda la animación;
- coloración de partes de texto/ecuaciones desde `scene.play` con `Anim.color(...)`;
- acciones glTF con velocidad, loop, reverse, offset y cross-fade.

`Scene.play()` compone animaciones en paralelo y admite lag; llamadas sucesivas,
`wait()` y delays construyen la secuencia. El motor Rust dispone de más tipos de
animación que el binding público, por lo que la cobertura debe medirse desde Python y
no desde el enum interno.

El núcleo de easing es amplio: curvas ease-in/out/in-out, spring, steps, there-and-back,
Bezier y otras variantes. Python ofrece accesos directos (`linear`, `smooth`, `spring`,
`steps`) y un conjunto de nombres mediante `ease()`/`rate()`, pero no expone aún todos
los parámetros ni las curvas custom del núcleo.

### 3. Texto y matemáticas

Esta es una de las áreas más fuertes del motor:

- shaping de texto a paths vectoriales;
- jerarquía por glifos para animaciones de escritura;
- soporte de fuentes del sistema y registro interno;
- fórmulas y documentos mediante Typst;
- cache de jerarquías compiladas de Typst;
- partes semánticas animables con `parts(...)`: grupos ordenados de contenido donde cada
  parte nombrada es un drawable independiente y, dentro de matemática `$...$`, las partes
  adyacentes se separan implícitamente — escribir, colorear o transformar un fragmento de
  ecuación ya no exige partirla a mano;
- baseline tipográfico unificado para que los `Transform` entre textos no se reduzcan a
  un cambio de posición;
- salida vectorial consistente con el renderer.

Para convertir esta capacidad en una función de producción todavía hacen falta una API
de fuentes estable, control tipográfico desde Python, manejo claro de errores de fuente y
tests de portabilidad entre Windows, Linux y macOS.

### 4. Sistema reactivo

La API de `Drawable` expone updaters predefinidos (`orbit`, `advance_x`, `bob`, `rotate`,
`pulse`), bindings de posición, `tracking_line`, `traced_path` y `traced_path_3d`. También
existe un callback Python por frame para casos como integración de un atractor, aunque
debe considerarse una superficie sensible a rendimiento y a validación de stubs.

`value_tracker()` ya permite leer, modificar y animar el valor desde Python, y las
geometrías reactivas públicas cubren puntos, tangentes, normales, curvatura, arcos,
resortes, cotas y trazas 3D. Siguen pendientes una API reactiva uniforme para cualquier
geometría, callbacks seguros fuera del hilo de render y una superficie completa de signals.

### 5. Multi-escena y transiciones

`Scene.segment()` y `Scene.link()` crean segmentos nombrados. Python expone:

- `Cut`;
- `CrossFade`;
- `FadeThrough`;
- `Slide` en cuatro direcciones.

El binding también expone `ZoomThrough` y `Morph`. `reuse`, `persist` y `release` permiten
controlar qué objetos sobreviven entre segmentos, evitando que una transición automática
los trate como contenido local. Los segmentos ya son suficientes para organizar un video
por capítulos o escenas; aún falta una composición no lineal de alto nivel.

### 6. Presentaciones

`Scene.segment(name, notes=..., layout=...)` define la estructura semántica y
`Scene.stop()` añade únicamente los puntos que requieren input. Durante la reproducción,
el timeline se pausa al alcanzarlos y permite avanzar o retroceder con teclado o mouse. El
editor muestra los stops en la barra temporal y puede iniciarse en pantalla completa con
`--present`.

La Presenter View abre una segunda ventana con segmento/parada actual, siguiente parada, notas,
cronómetro, navegación, overview consultable por nombre, miniaturas y salto directo a
segmentos o paradas. Las miniaturas se capturan de forma asíncrona en un mundo ECS/Vello
aislado, se invalidan con hot reload y nunca hacen seek sobre el timeline público. Sus atajos locales
son `←`, `→`, `Espacio`, `O` (overview), `B` (pantalla negra) y `W` (pantalla blanca).
El panel principal mantiene previews clicables del segmento anterior, actual y siguiente; el
siguiente usa su estado de entrada para no revelar anticipadamente animaciones, mientras
el overview conserva la composición completa de cada segmento.
`gaanim check <script.py>` ejecuta además un preflight de formato 16:9, notas, pasos,
duración y placeholders; `--strict` permite usarlo como gate antes de una sustentación.

Presenter View usa una interfaz oscura de alto contraste, jerarquía tipográfica ampliada
y progreso visual del timeline. El tema público `presentation` unifica fondo,
roles de texto y defaults de title cards, bullets, captions, callouts, bar charts, tablas
y bloques de código; los bar charts muestran también el valor de cada barra.

La configuración visual ya no está limitada a nombres fijos: `Theme` reúne colores
semánticos, tipografías, tamaños y archivos TTF/OTF embebidos. Puede construirse desde
cero, derivarse de otro tema o de esquemas conocidos como Nord, Dracula, Solarized,
Gruvbox, Tokyo Night y Catppuccin; los componentes consumen el mismo palette.
Los tokens también se consultan con `theme.color(...)`/`scene.canvas.color(...)`, y
`scene.canvas.validate_theme()` audita contraste y tipografía antes de una presentación.

Este es un **modo de presentación beta para slides generales**. El comando
`gaanim init slides` genera un deck 16:9 con segmentos semánticos, notas y paradas.
La identidad visual se configura con temas, `scene.brand(...)` y las regiones de
cada slide, sin APIs ni recursos institucionales incorporados al motor.

Para convertirlo en una solución general de slides todavía faltan:

- enlaces internos y navegación no lineal de presentación;
- integración de imágenes, tablas, charts y código.

### 7. Render y efectos

El renderer usa Vello/wgpu, mantiene orden por `z_index` y `creation_order`, y conserva
fragmentos renderizables en cache. Admite fill/stroke vectorial y `peniko::Brush` en la
capa Rust. `Scene.image(path)` decodifica PNG/JPEG/WebP a RGBA, conserva la transparencia,
participa en las transformaciones y opacidad normales, admite tamaño objetivo con `contain`,
`cover` o `stretch`, crop en píxeles de fuente y reutiliza por proceso la textura decodificada
para rutas repetidas.

Estado de efectos:

- `DropShadow`, `GaussianBlur` y `Glow` se dibujan mediante muestras vectoriales suaves
  conservadas en la caché de fragmentos;
- Python expone la superficie compacta `.shadow(...)`, `.blur(...)`, `.glow(...)` y
  `.no_effects()`, compatible con fills y strokes sólidos o gradientes;
- `Brush.linear`, `Brush.radial` y `Brush.sweep` exponen gradientes para fill y stroke
  en Python, con ramps `pad`, `repeat` y `reflect`.

La ruta 3D nativa usa Bevy para meshes y materiales PBR, mientras Vello conserva el
render vectorial de overlays. Hoy soporta:

- `axes_3d` con tres planos, ticks, números y labels billboard o HUD;
- `surface` triangulada, `polyline_3d` con colores por vértice y colormaps, ahora con
  `Create`/`Write` real: la geometría fuente se retiene (`LineListSource`) y el reveal
  recorta segmentos interpolando vértices y colores en el segmento parcial;
- cámara perspectiva con `look_at`, `orbit`, `dolly` y transforms 3D de objetos;
- modelos glTF/GLB con jerarquía seleccionable, skins, morph targets, materiales PBR,
  texturas y acciones muestreadas desde el timeline;
- overlays `hud` y etiquetas `billboard` que se mantienen legibles sobre la escena.

El render es determinista en los bordes de progreso: un reveal en progreso cero produce
geometría realmente vacía —el subpath en 0 ya no deja un `MoveTo` suelto, el renderer
sustituye el path por uno vacío y las líneas 3D sin segmentos se ocultan en el límite
ECS—, eliminando píxeles de caps/antialias que contaminaban snapshots exactos. El seek
además pre-siembra el valor `from` de animaciones de dibujo futuras, de modo que un
objeto cuyo `Create` empieza más tarde permanece en su estado inicial durante seeks
previos, y la pose de cámara se resuelve como una sola unidad publicada de forma
idéntica en las cámaras authored, rig y resolved.

Esta ruta es funcional pero reciente: requiere ampliar pruebas visuales y E2E, documentar
los límites de materiales/extensiones glTF y medir el coste de exportar escenas híbridas.

### 8. Exportación

El crate de exportación implementa:

- MP4/H.264, WebM/VP9, WebP animado, GIF y secuencia PNG;
- render por frames y seek determinista del timeline;
- exportación headless directa mediante GPU;
- fallback por CPU con libx264 y detección de NVENC, AMF, QSV y VA-API;
- rango temporal, transparencia y presets de resolución/calidad en Rust.

La API Python pública usa `scene.export(path, fps=None, ...)`: selecciona el formato por
extensión y permite transparencia, calidad, aspect ratio, rango temporal, segmento, CRF,
encoder y velocidad. La exportación depende de FFmpeg y aún necesita smoke tests
automatizados por plataforma y formato. Las escenas 3D usan la ruta híbrida nativa de
Bevy; el editor debe mantener aislado ese proceso cuando la escena contiene recursos
3D nativos para no contaminar el preview interactivo.

`scene.audio(...)` declara pistas relativas al timeline; el exportador las mezcla y
muxea con FFmpeg en MP4 (AAC) y WebM (Opus). PNG, GIF y WebP no aceptan audio.
Todavía no hay reproducción, waveform ni scrubbing de audio en el editor.

### 9. Editor y flujo de iteración

La aplicación incluye:

- ejecución embebida de scripts Python;
- hot reload al guardar;
- preview GPU con play/pause y seek;
- barra de transporte con scrub, timecode copiable, velocidad, navegación entre escenas y
  loop por segmento (el panel de timeline con tracks/clips, zoom y edición de tiempos fue
  retirado en la simplificación de UI de 2026-08-14);
- selección básica de objetos por bounds;
- pin always-on-top y diálogo de exportación;
- Inicio de proyectos con `gaanim init`, proyectos recientes y diagnóstico de Python/uv;
- validación `gaanim check` para proyectos de video/slides;
- picking 2D/3D básico, interacción de cámara perspectiva y soporte de exportación híbrida;
- conservación del proceso gráfico durante recargas.

Limitaciones actuales:

- el hot reload vuelve a ejecutar el script completo;
- la selección no tiene panel de propiedades ni manipulación visual;
- el zoom/pan de cámara existe para preview 2D/3D, pero la edición directa de composición
  sigue siendo limitada;
- los errores se reportan principalmente por consola/estado de recarga y faltan paneles
  de diagnóstico más ricos;
- la edición temporal del timeline fue retirada junto con el widget: debe volver como
  un formato de proyecto persistente y bidireccional con el script, no como un panel
  acoplado al playback;
- audio, glTF y exportación 3D necesitan más pruebas automatizadas en el flujo del editor.

---

## Brechas críticas para generación de contenido

### P0 — Consolidar el motor como librería coherente

La primera convergencia de API ya está aplicada; el trabajo restante es de contrato y
distribución, no de escoger otra fachada:

1. [x] Declarar `Scene` como API canónica y deprecar `Canvas`.
2. [x] Separar el viewport en `scene.canvas`.
3. [x] Migrar ejemplos y documentación nuevas a `Scene`.
4. [~] Mantener `gaanim_core.pyi`, binding nativo, `__init__.py` y documentación alineados;
   la validación actual comprueba que los miembros declarados existan, pero no detecta
   automáticamente todo miembro nativo aún no documentado.
5. [x] Documentar la aplicación con Python embebido y la wheel local para exportación
   headless.
6. [ ] Alinear versiones Rust/Python y establecer una política de compatibilidad.
7. [x] Publicar README, quickstart, instalación y referencia Typst inicial.
8. [x] Añadir CI para formato, check, tests, Clippy, wheel y regresiones visuales selectivas.

**Criterio de salida:** un usuario nuevo puede crear un proyecto, ejecutar sus ejemplos,
usar la API pública sin importar internals y exportar una pieza sin encontrar APIs obsoletas.

### P0 — Validación de render y exportación

La compilación por sí sola no garantiza el resultado visual. El estado actual es:

1. [x] tests unitarios de timeline, interpolación, layout y transforms;
2. [~] tests de integración de la API Python; existe validación del stub y de ejemplos,
   pero falta una matriz amplia de escenarios públicos;
3. [x] snapshots visuales de transform, imagen, SVG, cámara y otros ejemplos;
4. [~] smoke tests headless de los cinco formatos; faltan ejecuciones automatizadas de
   todos los formatos y de audio por plataforma;
5. [~] validación de fuentes/Typst en Windows y Linux; falta una matriz macOS declarada;
6. [~] pruebas de seek, segmentos y stops; hot reload y escenas 3D requieren más E2E;
7. [ ] benchmarks reproducibles de preview, timeline y exportación.

**Criterio de salida:** cada release puede demostrar que API, frames, escenas 3D y exports
siguen siendo correctos, no solo que el workspace compila.

### P1 — Assets y multimedia para contenido real

1. [x] asset manager con rutas relativas, manifiesto de proyecto, precarga y recarga;
2. [x] SVG con grupos direccionables, clipping, gradientes, texto outlineado y filtros
   comunes;
3. [~] `AudioTrack`, offsets, volumen, fade, mezcla y muxing con FFmpeg; falta preview,
   waveform, scrubbing y editor de pistas;
4. [~] glTF/GLB con jerarquía, PBR, texturas, skins, morph targets y acciones; faltan
   pruebas de compatibilidad por asset, extensiones y un flujo E2E estable.

Sin el cierre de audio, video embebido y validación de assets, gaanim seguirá dependiendo
de herramientas externas para una parte importante de la producción comercial o social.

### P1 — Diagramas técnicos, mecanismos y primera ruta 3D

Las figuras de física, ingeniería y matemáticas deben poder construirse como
geometría animable, no como una imagen plana. Este bloque pasa por delante de
los componentes editoriales generales porque también desbloquea gráficas,
diagramas de procesos y explicaciones educativas.

Capacidades completadas:

1. [x] `path` unificado y `polyline`/`curve` explícitos para rieles, resortes,
   trayectorias y contornos;
2. [x] arcos, flechas curvas, ángulos y cotas (`arc`, `curved_arrow`, `dimension`);
3. [x] `ValueTracker` desde Python, con animación de valor y bindings;
4. [x] geometría reactiva nativa para arcos, resortes, etiquetas, tangentes,
   normales, curvatura y cotas;
5. [x] grupos y drawables con pivote de transformación para mecanismos rotatorios.
6. [x] `axes_3d`, `surface` y `polyline_3d` con perspectiva, colores por vértice y
   colormaps públicos.
7. [x] overlays `hud`/`billboard`, transforms 3D y callbacks de updater para demos
   dinámicas; falta estabilizar su contrato de tipos y rendimiento.
8. [~] modelos glTF con selección de partes y acciones reproducibles desde el timeline;
   falta una matriz visual/E2E que cubra múltiples exporters y materiales.

**Criterio de salida:** una escena Python puede animar un mecanismo como un
disco con resorte y masa: el conjunto rota, la masa se desplaza, el resorte se
deforma y las cotas o ecuaciones siguen el estado sin depender de SVG externo.

### P1 — Tema científico y coordenadas configurables (parcialmente completado)

La escena debe tener una identidad técnica coherente por defecto y no depender
de que cada ejemplo configure tipografías o ejes manualmente.

1. [x] temas públicos `technical`/`scientific` y `paper`, con New Computer Modern para texto técnico y New Computer Modern Math para ecuaciones;
2. [x] el runtime aplica el `TextConfig` del tema antes de compilar la escena: `paper` usa fondo blanco y fill negro para los roles `title`, `subtitle`, `body`, `math`, `caption` y `code` de `Text`; un `.fill(...)` explícito conserva prioridad;
3. [x] fuentes registradas por el motor para la composición técnica actual;
4. [x] `scene.axes(...)` y `scene.number_plane(...)` desde Python, con rangos, ticks, números, grilla, estilos y etiquetas configurables;
5. [x] `scene.axes_3d(...)` con tres rangos, planos de grilla, labels billboard/HUD y
   estilos separados;
6. [x] estilos separados para ejes, grilla, ticks, números y rótulos, con
   visibilidad global o independiente por eje desde `scene.axes(...)`;
7. [x] aliases semánticos de composición (`cover`, `content`, `comparison`,
   `divider`, `conclusion`) y branding global reutilizable con logo, regla,
   footer y numeración sobre el sistema de temas.

**Criterio de salida:** un diagrama científico puede crear ejes numerados y una
grilla legible con una sola llamada; texto y ecuaciones mantienen una familia
tipográfica consistente en preview y export, tanto en fondo oscuro como en `paper`.

### P1 — Componentes de contenido reutilizables

La base ya incluye varios componentes; el siguiente trabajo es ampliar cobertura y
consistencia visual:

- [x] listas, bullets, callouts, captions, title cards y end cards;
- [~] `Table` y `Matrix`: tabla pública y matrices Typst disponibles, falta una API
  nativa de matriz más rica;
- [~] `BarChart`: disponible; faltan line chart, pie/donut y escalas/configuración avanzada;
- [~] `Code`: bloque monoespaciado disponible; faltan highlighting por token y diff animado;
- [x] plantillas 16:9, 9:16 y 1:1 con safe areas;
- [ ] conectores semánticos, lower thirds y componentes responsive.

La API debe favorecer componentes semánticos y plantillas, no obligar al usuario a
componer cada pieza desde primitivas.

### P1 — Cámara y composición

- [x] controlador semántico Python `scene.camera` para posición, zoom y rotación;
- [x] `scene.camera.frame_to(mobject, margin=...)` con pan y zoom paralelos;
- [x] cámara follow, pan, zoom, rotación y shake como animaciones de alto nivel;
- [x] clipping/masks vectoriales públicos con `.clip(mask)` para drawables y grupos;
- [x] gradientes y efectos vectoriales de sombra/blur/glow con caché retained;
- [x] soporte explícito de aspect ratios y safe areas en `scene.canvas`;
- [x] perspectiva 3D, `look_at`, `orbit`, `dolly`, HUD y billboards;
- [x] clips de cámara atómicos (`CameraLookAt`/`CameraOrbit`): una animación = una pose,
  publicada de forma idéntica en authored/rig/resolved y verificada en seek exacto;
- [ ] encuadre, picking y composición responsive que combinen 2D y 3D de forma uniforme.

### P1 — Presentaciones completas

- [x] Overview por nombre, indicador de segmento/parada, presenter notes y presenter view;
- [x] Caché de miniaturas en mundo ECS/Vello aislado, publicada como texturas egui sin
  mutar el `Timeline` de reproducción;
- [x] previews persistentes anterior/actual/siguiente en Presenter View, con navegación
  por clic y estado de entrada separado para el siguiente segmento;
- [x] exportación continua, por un segmento nombrado o por todos los segmentos con una plantilla
  de ruta desde la misma función `scene.export(...)`;
- [x] navegación directa por nombre de segmento y por nombre/índice de parada;
- [x] plantillas semánticas, temas y branding global expuestos desde Python;
- [ ] control remoto o protocolo simple de navegación.

### P2 — Animación avanzada

- [x] terminar y validar `Transform`/`ReplacementTransform`;
- [x] `TransformMatchingShapes`, `TransformMatchingText` y `TransformMatchingTex`;
- [ ] `ApplyWave`, deformaciones y homotopías;
- [~] animación de propiedades tipográficas: baseline unificado y coloración de partes en
  `scene.play` existen; falta animar familia, tamaño y peso;
- [ ] callbacks/eventos seguros sin bloquear el renderer;
- [~] API reactiva para trackers, updaters, señales y `always_redraw`.

### Fuera del foco inmediato

WASM, Lottie, plugins y generación asistida por IA pueden ser diferenciadores, pero no
deben desplazar la estabilización 2D/3D, los assets, el audio y el flujo de publicación.

---

## Aportes diferenciadores hacia el estado del arte

Manim domina en volumen de usuarios, Motion Canvas en la interactividad de curvas y
Remotion en el ecosistema React. Ninguno combina render vectorial GPU, un timeline con
seek exacto y un solo núcleo reutilizable desde Rust y Python. Los aportes siguientes
explotan exactamente esa intersección: cada uno parte de una palanca que ya existe en el
código y nombra el salto que falta para convertirlo en ventaja competitiva medible.

### 1. Determinismo frame-exact como contrato de producto

**Palanca actual:** el reveal de progreso cero produce geometría vacía en 2D y 3D; el
seek pre-siembra estados `from` de clips futuros; `follow_lag` es bitwise-idéntico entre
evaluación incremental y rewind; la pose de cámara se publica atómica e idéntica en
authored/rig/resolved; `gaanim_diff` ya compara snapshots exactos.

**Salto:** convertir coincidencias puntuales en contrato continuo.

- Un "render oracle" de CI que re-exporte frames arbitrarios y los compare contra
  fixtures bendecidos en Windows y Linux, para preview y export, 2D y 3D.
- Un audit/lint que rechace nuevas rutas de render que consulten tiempo de wall-clock o
  estado fuera del timeline.
- El contrato documentado: mismo proyecto + mismo seek = mismos píxeles, sin excepciones.

Ningún competidor promete esto hoy, y es la base de regresión visual barata y confiable.

### 2. Seek y export de coste acotado

**Palanca actual:** el timeline replaya clips desde keyframes con índices BTree; el costo
de seek está marcado como pendiente de optimizar; la exportación ya es seek determinista
por frames.

**Salto:** snapshots jerárquicos por intervalo (uno cada N segundos) para que el seek
arbitrario cueste O(clips activos) y no O(historia), con benchmarks reproducibles (hoy
ausentes en P0). El export paralelo por frames depende directamente de esto: con seeks
baratos y deterministas, N workers renderizan frames desordenados sin artefactos — algo
que un motor imperativo como Manim no puede prometer por diseño.

### 3. `parts()` como primitiva universal de semántica animable

**Palanca actual:** `parts(...)` agrupa texto/ecuaciones en partes nombradas con
separadores implícitos en `$...$`; existen `TransformMatchingShapes/Text/Tex`, jerarquía
por glifos y partes seleccionables de glTF; los componentes técnicos ya exponen partes
independientes (`dimension`, `vector`, soportes mecánicos).

**Salto:** generalizar el mismo contrato a SVG (los grupos direccionables ya existen),
charts (ejes/series/puntos como parts) y grupos nativos, de modo que cualquier mobject
exponga `parts()` consultable y animable con la misma API. El diferenciador no es
"tener transforms": es que declarar la parte una vez alcance para escribirla,
colorearla, transformarla y hacer matching con ella siempre.

### 4. Rig de cámara declarativo uniforme 2D/3D

**Palanca actual:** el pipeline authored→rig→resolved publica una sola pose atómica; la
cámara semántica 2D (`frame_to`, follow, pan, zoom, shake) y la 3D (`look_at`, `orbit`,
`dolly`) ya son de alto nivel.

**Salto:** exponer el rig como objeto declarativo combinable —orbit + shake + follow
superpuestos con pesos— con la misma superficie en 2D y 3D, y un `frame_to` que encuadre
contenido mixto 2D/3D (hoy pendiente en P1). Una sola mente de cámara para toda la
escena es un argumento de producto que Manim no tiene.

### 5. Hot reload incremental sobre el modelo diferido

**Palanca actual:** la construcción de escenas es diferida (`Arc<Mutex<MobjectSpec>>`)
pero se re-ejecuta completa al guardar; el proceso gráfico sobrevive recargas y los
artefactos visuales de reload fueron corregidos.

**Salto:** diff de specs entre recargas para preservar entidades estables —y la selección
del editor— cuando solo cambia un parámetro, con presupuesto medido de tiempo de reload
(indicador ya definido). Es la base sobre la que la edición temporal retirada del editor
debe volver como edición bidireccional sobre el script.

### 6. Audio de primera clase en el editor

**Palanca actual:** pistas con offsets, volumen y fade ya se mezclan y muxean en export;
no hay preview.

**Salto:** waveform en la barra de transporte, scrub de audio sincronizado por seek (el
timeline ya es seekable; falta el reloj de audio) y marcadores de beats como snaps de
animación. Es el bloque P1 más visible para contenido de redes y cierra la dependencia
de herramientas externas de montaje.

### 7. Matriz de assets reales como evidencia pública

**Palanca actual:** el test de aceptación glTF usa un asset real (`Fox.glb`) con
jerarquía, bounds y acciones verificadas.

**Salto:** una matriz versionada de assets representativos (exporters Blender/Maya,
KhronosSample, glTF Pipeline) con capturas bendecidas por exporter y límites de
materiales/extensiones documentados; el mismo patrón para fuentes/Typst por plataforma.
"Compatible con glTF" deja de ser una afirmación y pasa a ser un artefacto verificable.

---

## Roadmap propuesto por releases

### 0.1.x — Baseline actual (2026-08-16)

Este bloque describe capacidades ya presentes en la versión de trabajo `0.1.0`, no una
promesa de release separada:

- [x] `Scene` como fachada pública, `scene.canvas` como viewport y `Canvas` deprecado.
- [x] Imágenes raster, SVG vectorial avanzado, gradientes, efectos retained y audio
  export-only.
- [x] Paths, geometría reactiva, diagramas técnicos, `TransformMatching*` y pivotes.
- [x] Temas, branding, layouts, componentes editoriales y presets de aspect ratio.
- [x] Proyectos `video`/`slides`, `gaanim.toml`, assets relativos, `gaanim check` y
  detección side-effect-free de Python/uv.
- [x] Segmentos semánticos, notas, stops, overview, Presenter View y exportación por segmento.
- [x] Primera ruta 3D pública: ejes, superficies, polilíneas, cámara perspectiva,
  HUD/billboards y transforms 3D.
- [x] Partes semánticas de texto/ecuaciones animables (`parts(...)`) con coloración desde
  `scene.play` y baseline tipográfico para transforms de texto.
- [x] Cámara 3D atómica (`CameraLookAt`/`CameraOrbit`) y determinismo de reveal en el
  borde cero para paths 2D y líneas 3D nativas.
- [~] glTF/GLB: importación, jerarquía, materiales, skins/morphs y acciones, con test de
  aceptación sobre un asset real; falta endurecer compatibilidad y exportación E2E.
- [~] layouts persistentes: reflow anidable disponible; faltan overflow, spans y
  variantes responsive.

### Próxima entrega — Hardening de contenido y 3D (0.2/0.3)

- [ ] Completar la matriz Python/API/stubs/docs y una política explícita de compatibilidad.
- [ ] Ejecutar smoke tests de MP4, WebM, WebP, GIF y PNG, con audio donde corresponda,
  en las plataformas soportadas.
- [ ] Añadir regresiones visuales de `axes_3d`, `surface`, `polyline_3d` y glTF, además
  de casos de seek, hot reload y exportación híbrida.
- [ ] Probar materiales, extensiones y assets glTF representativos; documentar límites.
- [ ] Añadir preview/scrubbing de audio, waveform y editor de pistas.
- [ ] Añadir watcher de assets, presets consumidos desde `gaanim.toml` y hot reload
  incremental donde el perfil lo justifique.
- [ ] Completar line/pie charts, syntax highlighting y diff animado de código.
- [ ] Mejorar picking, panel de propiedades, encuadre 2D/3D y composición responsive.
- [ ] Render oracle de determinismo frame-exact en CI (re-export de frames arbitrarios
  comparados contra fixtures) y snapshots jerárquicos para seek de coste acotado, que
  habilitan export paralelo por frames.

### 1.0 — Librería estable

- [ ] API Python y Rust versionadas y documentadas.
- [ ] Compatibilidad de proyectos entre versiones menores.
- [ ] Matriz de plataformas soportadas con CI y releases reproducibles.
- [ ] Suite visual y E2E estable.
- [ ] Performance presupuestada para preview y exportación.
- [ ] Documentación de despliegue y troubleshooting.

---

## Indicadores de madurez recomendados

El avance no debería medirse solo por cantidad de mobjects o animaciones. Para cada
release conviene registrar:

| Indicador | Objetivo antes de 1.0 |
|---|---|
| Ejemplos que ejecutan con la API pública | 100% |
| API Python cubierta por stubs y documentación | 100% |
| Formatos de exportación con smoke test | Todos los soportados |
| Plataformas con CI | Todas las declaradas como soportadas |
| Regresiones visuales cubiertas | Escenas representativas de cada subsistema |
| Tiempo de hot reload | Medido y con presupuesto definido |
| Exportación determinista | Mismo frame para mismo proyecto/configuración |
| Releases instalables | Aplicación y/o wheel según la estrategia elegida |

---

## Ventajas técnicas que conviene preservar

1. Renderer vectorial GPU con Vello/wgpu.
2. ECS y orden de sistemas centralizado mediante `SceneSet`.
3. Timeline con clips, seek, snapshots, escenas y stops explícitos.
4. Texto y matemáticas convertidos a geometría vectorial, con Typst integrado.
5. Modelo diferido de construcción de escenas, adecuado para hot reload y exportación
   determinista.
6. Separación entre API, runtime, editor y exportación.
7. Tipos 3D-ready que permiten ampliar el producto sin perder el foco en composición 2D.
8. Uso directo de tipos gráficos (`peniko`, `kurbo`, `glam`) sin wrappers innecesarios.
9. Clips de cámara atómicos y pipeline authored→rig→resolved con publicación determinista
   de una sola pose.
10. Reveal frame-exact: progreso cero es geometría vacía en 2D y 3D, y el seek pre-siembra
    los estados iniciales de clips futuros.

La oportunidad de gaanim no está en copiar todo Manim o Motion Canvas. Está en convertir
esta base Rust/GPU en un flujo especialmente rápido y confiable para crear, iterar,
presentar y exportar contenido vectorial desde Python. El siguiente salto de calidad
depende más de coherencia de producto y capacidades audiovisuales básicas que de aumentar
el número bruto de efectos.
