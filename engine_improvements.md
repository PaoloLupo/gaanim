# Gaanim — estado actual y plan de evolución

Gaanim es un motor de animación vectorial 2D acelerado por GPU, escrito en Rust sobre
Bevy ECS, Vello y wgpu. Su objetivo de producto es permitir crear videos explicativos,
contenido educativo, piezas para redes y presentaciones animadas mediante una API
programática en Python, manteniendo un núcleo reutilizable desde Rust.

> [!NOTE]
> Última auditoría del repositorio: **2026-07-19**.
> `Scene` es la fachada pública de Python; `scene.canvas` contiene la configuración del
> viewport y `Canvas(...)` se mantiene como constructor de compatibilidad deprecado. No se
> asume que una capacidad interna de Rust esté disponible desde Python. `Transform` y
> `ReplacementTransform` tienen pruebas y regresión visual.

---

## Resumen ejecutivo

**Estado de producto: alfa funcional (`gaanim` 0.3.0).**

Gaanim ya puede construir, previsualizar y exportar animaciones vectoriales 2D con
formas, texto, fórmulas matemáticas, composición temporal, segmentos, transiciones y
algunos comportamientos reactivos. El workspace compila completo y la arquitectura
separa correctamente escena, animación, timeline, renderer, texto, layout, exportación,
editor, API y bindings.

La base técnica es más madura que la experiencia de librería. La API de Python actual
expone solo una parte de lo que existe en Rust, pero `Scene` ya es la fachada pública que
posee mobjects, timeline, reproducción y exportación. La documentación y los ejemplos
usan `Scene`; `Canvas` queda limitado a la configuración accesible como `scene.canvas` y
a un constructor de compatibilidad deprecado. El flujo principal también está orientado a
ejecutar scripts dentro de la aplicación `gaanim`, no a instalar y usar el módulo como un
paquete Python autónomo.

En términos prácticos:

- **Sí es viable hoy** para prototipos y piezas vectoriales programáticas: títulos,
  diagramas simples, fórmulas, explicaciones matemáticas y animaciones cortas sin audio.
- **Es experimental** para presentaciones interactivas: existen breakpoints y navegación,
  pero faltan las herramientas propias de una solución de slides.
- **Todavía no está listo** como pipeline general de producción de contenido: faltan
  audio, SVG, gráficos de datos, código, plantillas y una distribución coherente.
- **Todavía no está listo** como librería pública estable: API, documentación, ejemplos,
  stubs, versionado, pruebas de integración y empaquetado deben converger.

### Semáforo por caso de uso

| Caso de uso | Estado | Evaluación actual |
|---|---:|---|
| Animación vectorial 2D programática | 🟢 | Núcleo funcional con preview y exportación |
| Contenido matemático/educativo simple | 🟢 | Texto y ecuaciones Typst son una fortaleza |
| Videos cortos para redes | 🟡 | Viable si audio, imágenes y montaje se hacen fuera de gaanim |
| Presentaciones animadas en vivo | 🟡 | Breakpoints básicos; faltan overview, notas y exportación por slide |
| Contenido con código, tablas o datos | 🔴 | No hay mobjects públicos para esos formatos |
| Motion graphics con multimedia | 🟡 | PNG/JPEG/WebP disponibles; faltan SVG, video y audio |
| Pipeline audiovisual de producción | 🔴 | Faltan assets, audio, pruebas E2E, empaquetado y estabilidad de API |

---

## Evidencia de la auditoría

- `cargo check --workspace` finaliza correctamente.
- El workspace contiene 13 crates de gaanim más el crate de documentación.
- El paquete declara versión Python `0.3.0`; varios crates Rust siguen en `0.1.0`.
- La API pública de Python exporta `Scene`, `Drawable`, `Anim`, `Transition`, `Color`,
  `Anchor`, `Direction` y `Updater`. `Canvas(...)` emite una deprecación y devuelve un
  `Scene`; la configuración visual vive en `scene.canvas`.
- Los ejemplos Python no importan `Canvas` como fachada de animación.
- La documentación Typst usa `Scene`, listas en `scene.play([...])` y métodos expuestos
  por los bindings actuales.
- CI ejecuta formato, check, tests y Clippy en Windows y Linux, además de la regresión
  visual de `transform_demo` en Windows.
- `cargo test --workspace --jobs 2` finaliza correctamente.

---

## Decisión de API: deprecar `Canvas` como fachada pública

**Decisión aprobada el 2026-07-13:** `Scene` será nuevamente el punto de entrada público
de Python. `Canvas` dejará de representar toda la animación y quedará reservado para el
viewport y su configuración visual.

La responsabilidad objetivo de cada concepto es:

| Concepto | Responsabilidad |
|---|---|
| `Scene` | Mobjects, animaciones, timeline, reproducción, slides y exportación |
| `Canvas` | Dimensiones, fondo, márgenes, coordenadas, aspect ratio y safe areas |
| `Video` | Composición futura de múltiples escenas y transiciones |
| `Presentation` | Composición futura de slides, navegación y presenter mode |

API objetivo mínima:

```python
scene = Scene(width=1920, height=1080, background=BLACK)

title = scene.title("Gaanim")
scene.play(title.write())
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
la fachada `Canvas`. Por ello, no se necesita un ciclo largo de deprecación: el cambio se
hará directamente durante 0.3.x. Si resulta útil para migrar ejemplos locales, podrá
existir temporalmente un alias `Canvas = Scene`, marcado como deprecado y sin aparecer en
la documentación nueva. Debe eliminarse antes de 1.0.

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
| Render vectorial GPU | `gaanim_renderer` | ✅ Funcional; efectos avanzados parciales |
| Primitivas y texto semántico | `gaanim_objects`, `gaanim_text` | ✅ Base sólida |
| Posicionamiento y distribución | `gaanim_layout` | ✅ Funcional en Rust; exposición parcial |
| API canónica | `gaanim_api` | 🟡 Dos superficies con distinta cobertura |
| Binding Python embebido | `gaanim_python` | 🟡 Funcional dentro de la aplicación |
| Preview, hot reload y timeline visual | `gaanim_editor` | 🟡 Útil, todavía no es un editor integral |
| Exportación de frames y video | `gaanim_export` | 🟡 Implementada, necesita validación E2E |

La dirección arquitectónica es correcta: Python construye una descripción diferida
(actualmente `Canvas`, próximamente `Scene`), la aplicación la recompila a ECS y el mismo
timeline se usa para reproducción, seek y exportación. El principal riesgo ya no es la
separación de capas, sino mantener una sola API de producto coherente sobre ellas.

---

## Capacidades disponibles hoy

### 1. Construcción de escenas desde Python

La API pública actual permite:

- crear un `Canvas` con tamaño, fondo y margen uniforme;
- crear `circle`, `rect`, `rounded_rect`, `square`, `dot`, `ellipse`, `line` y `arrow`;
- crear `text`, `title`, `subtitle`, `equation` e `image` (PNG/JPEG/WebP);
- agrupar objetos;
- configurar fill, stroke, opacidad y z-index;
- posicionar con `at`, `at_anchor`, `next_to`, `align_to`, `to_edge` y `to_corner`;
- dividir el contenido en segmentos y enlazarlos con transiciones;
- ejecutar la escena en la aplicación o exportarla por extensión de archivo.

El núcleo Rust contiene más primitivas y helpers —polígonos, estrellas, gráficas,
braces, operaciones booleanas y layouts de grupo—. La API Python ya expone
`polyline`, `arc`, `curved_arrow`, `dimension` y ejes; el resto no debe anunciarse
como función de usuario hasta que esté enlazado, documentado y probado.

### 2. Animación y composición temporal

Desde Python están expuestas animaciones de:

- movimiento, posición, escala y rotación;
- fade in/out y cambio de opacidad;
- `Write`, `Create`, `Unwrite` y `Uncreate`;
- crecimiento/encogimiento desde el centro;
- `SpinInFromNothing`, `DrawBorderThenFill`, `Indicate` y `Wiggle`;
- `FadeTransform`;
- `Transform` y `ReplacementTransform` en la rama de trabajo actual.

`Canvas.play()` compone animaciones en paralelo y admite lag; llamadas sucesivas,
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
- salida vectorial consistente con el renderer.

Para convertir esta capacidad en una función de producción todavía hacen falta una API
de fuentes estable, control tipográfico desde Python, manejo claro de errores de fuente y
tests de portabilidad entre Windows, Linux y macOS.

### 4. Sistema reactivo

La API `Canvas` expone updaters predefinidos (`orbit`, `advance_x`, `bob`, `rotate`,
`pulse`), bindings de posición, `tracking_line` y `traced_path`. Esto permite demos
reactivas útiles sin callbacks Python por frame.

`value_tracker()` existe, pero el objeto retornado no tiene todavía una interfaz pública
completa para leer, modificar, enlazar o animar su valor desde Python. `AlwaysRedraw`,
señales avanzadas y callbacks arbitrarios siguen siendo capacidades internas o de la API
Rust anterior.

### 5. Multi-escena y transiciones

`Canvas.segment()` y `Canvas.link()` crean segmentos nombrados. Python expone:

- `Cut`;
- `CrossFade`;
- `FadeThrough`;
- `Slide` en cuatro direcciones.

El timeline Rust también define `ZoomThrough` y `Morph`, pero no están expuestos por el
binding `Transition` actual. Los segmentos ya son suficientes para organizar un video
por capítulos o escenas, aunque aún no existe una API de proyecto, asset management o
composición no lineal de alto nivel.

### 6. Presentaciones

`Canvas.slide()` inserta breakpoints. Durante la reproducción, el timeline se pausa al
alcanzarlos y permite avanzar o retroceder con teclado o mouse. El editor muestra los
breakpoints en la barra temporal.

Este es un **modo de presentación básico**, no una solución completa de slides. Faltan:

- overview con miniaturas;
- número de slide y progreso visible;
- notas del presentador y vista secundaria;
- exportación independiente por slide;
- enlaces internos y navegación no lineal de presentación;
- plantillas de título, agenda, dos columnas, cierre y branding;
- integración de imágenes, tablas, charts y código.

### 7. Render y efectos

El renderer usa Vello/wgpu, mantiene orden por `z_index` y `creation_order`, y conserva
fragmentos renderizables en cache. Admite fill/stroke vectorial y `peniko::Brush` en la
capa Rust. `Scene.image(path)` decodifica PNG/JPEG/WebP a RGBA, conserva la transparencia,
participa en las transformaciones y opacidad normales, admite tamaño objetivo con `contain`,
`cover` o `stretch`, crop en píxeles de fuente y reutiliza por proceso la textura decodificada
para rutas repetidas.

Estado de efectos:

- `DropShadow` sí se dibuja en el pipeline actual;
- `GaussianBlur` y `Glow` están definidos, pero el renderer contiene un TODO explícito
  y todavía no producen el efecto visual;
- gradientes pueden representarse internamente con `Brush`, pero no hay constructores
  de gradiente en la API de Python.

No existe todavía un pipeline 3D. Los transforms usan tipos preparados para z, pero el
producto actual debe presentarse como **2D vectorial**.

### 8. Exportación

El crate de exportación implementa:

- MP4/H.264, WebM/VP9, WebP animado, GIF y secuencia PNG;
- render por frames y seek determinista del timeline;
- exportación headless directa mediante GPU;
- fallback por CPU con libx264 y detección de NVENC, AMF, QSV y VA-API;
- rango temporal, transparencia y presets de resolución/calidad en Rust.

La API Python pública simplifica esto a `canvas.export(path, fps=None)`: selecciona el
formato por extensión y usa el tamaño del canvas. Transparencia, presets, encoder,
calidad y rangos no están expuestos de forma completa. La exportación depende de FFmpeg
y aún necesita smoke tests automatizados por plataforma y formato.

No hay audio: los exports son video o secuencias de imagen sin mezcla ni muxing de pistas.

### 9. Editor y flujo de iteración

La aplicación incluye:

- ejecución embebida de scripts Python;
- hot reload al guardar;
- preview GPU con play/pause y seek;
- barra de transporte, velocidad y navegación entre escenas;
- timeline expandible con tracks, clips, zoom, scroll, snap y edición de tiempos;
- selección básica de objetos por bounds;
- pin always-on-top y diálogo de exportación;
- conservación del proceso gráfico durante recargas.

Limitaciones actuales:

- el hot reload vuelve a ejecutar el script completo;
- la selección no tiene panel de propiedades ni manipulación visual;
- el zoom/pan disponible corresponde al timeline, no al canvas de la escena;
- los errores se reportan principalmente por consola/estado de recarga;
- los cambios hechos en el timeline no constituyen todavía un formato de proyecto
  persistente y bidireccional con el script.

---

## Brechas críticas para generación de contenido

### P0 — Convertir el motor en una librería coherente

Antes de ampliar el catálogo visual, la versión 0.3 necesita una sola superficie pública:

1. Declarar `Scene` como API canónica y deprecar la fachada pública `Canvas`.
2. Separar la configuración visual en un `Canvas` interno o accesible como
   `scene.canvas`.
3. Migrar los 18 ejemplos y la documentación a código ejecutable con `Scene`.
4. Regenerar y validar `gaanim_core.pyi` a partir de los bindings reales.
5. Decidir y documentar dos modos de distribución:
   - aplicación `gaanim <script.py>` con Python embebido;
   - paquete Python instalable, si se desea soportarlo, con extensión `cdylib` válida.
6. Alinear versiones Rust/Python y establecer una política de compatibilidad.
7. Añadir README, guía de instalación, tutorial mínimo y referencia de API.
8. Añadir CI para check, fmt, clippy, tests y construcción en las plataformas soportadas.

**Criterio de salida:** un usuario nuevo puede instalar gaanim, ejecutar todos los ejemplos
y exportar una pieza sin encontrar APIs obsoletas.

### P0 — Validación de render y exportación

La compilación por sí sola no garantiza el resultado visual. Hace falta:

1. tests unitarios de timeline, interpolación, layout y morph;
2. tests de integración de la API Python;
3. golden tests o snapshots de frames representativos;
4. smoke tests headless para MP4, WebM, WebP, GIF y PNG;
5. validación de fuentes/Typst en Windows, Linux y macOS;
6. pruebas de seek, hot reload y breakpoints;
7. benchmarks reproducibles de preview y exportación.

**Criterio de salida:** cada release puede demostrar que API, frames y exports siguen
siendo correctos, no solo que el workspace compila.

### P1 — Assets necesarios para contenido real

Orden recomendado:

1. asset manager con rutas relativas, precarga y recarga de assets;
2. ampliar SVG: preservar grupos origen, clipping, gradientes y texto;
3. `AudioTrack`, offsets, volumen, fade, mezcla y muxing con FFmpeg.

Sin estas capacidades, gaanim seguirá dependiendo de un editor externo para la mayor
parte de las piezas de contenido comercial o social.

### P1 — Diagramas técnicos y mecanismos dinámicos (prioridad inmediata)

Las figuras de física, ingeniería y matemáticas deben poder construirse como
geometría animable, no como una imagen plana. Este bloque pasa por delante de
los componentes editoriales generales porque también desbloquea gráficas,
diagramas de procesos y explicaciones educativas.

Orden de implementación:

1. `polyline`/`path` públicos para rieles, resortes, trayectorias y contornos abiertos;
2. arcos, flechas curvas, ángulos y cotas (`arc`, `curved_arrow`, `dimension`);
3. `ValueTracker` completo desde Python, con animación de valor y bindings;
4. geometría reactiva/`always_redraw` para que resortes, etiquetas y cotas respondan a un tracker;
5. grupos con pivote de transformación para mecanismos rotatorios.

**Criterio de salida:** una escena Python puede animar un mecanismo como un
disco con resorte y masa: el conjunto rota, la masa se desplaza, el resorte se
deforma y las cotas o ecuaciones siguen el estado sin depender de SVG externo.

### P1 — Tema científico y coordenadas configurables

La escena debe tener una identidad técnica coherente por defecto y no depender
de que cada ejemplo configure tipografías o ejes manualmente.

1. tema `Scientific` con New Computer Modern para texto técnico y New Computer Modern Math para ecuaciones;
2. fuentes empaquetadas y registradas por el motor para resultados idénticos en todas las plataformas;
3. `scene.axes(...)` y `scene.number_plane(...)` desde Python, con rangos, ticks, números, grilla, estilos y etiquetas configurables;
4. estilos separados para ejes, grilla, ticks y rótulos, además de una API para ocultar elementos individuales.

**Criterio de salida:** un diagrama científico puede crear ejes numerados y una
grilla legible con una sola llamada, y texto/equaciones mantienen una familia
tipográfica consistente en preview y export.

### P1 — Componentes de contenido reutilizables

Priorizar objetos de alto impacto editorial:

- listas y bullets animados;
- `Table` y `Matrix`;
- `BarChart`, line chart y pie/donut chart;
- `Code` con syntax highlighting, highlights y diff animado;
- callouts, labels, braces y conectores;
- lower thirds, title cards, captions y end cards;
- plantillas para 16:9, 9:16 y 1:1 con safe areas.

La API debe favorecer componentes semánticos y plantillas, no obligar al usuario a
componer cada pieza desde primitivas.

### P1 — Cámara y composición

- API Python para posición, zoom y framing de cámara;
- `camera.frame_to(mobject, margin=...)`;
- cámara follow, pan, zoom y shake como animaciones de alto nivel;
- clipping/masks públicos;
- gradientes y efectos de sombra/blur/glow completos;
- soporte explícito de aspect ratios y safe areas en `scene.canvas`.

### P1 — Presentaciones completas

- overview con miniaturas;
- slide indicator y barra de progreso;
- presenter notes y presenter view;
- exportación por slide y exportación continua;
- navegación directa por nombre/id de slide;
- plantillas y temas expuestos desde Python;
- control remoto o protocolo simple de navegación.

### P2 — Animación avanzada

- terminar y validar `Transform`/`ReplacementTransform`;
- `TransformMatchingShapes` y matching semántico para texto/math;
- `ApplyWave`, deformaciones y homotopías;
- animación de propiedades tipográficas;
- callbacks/eventos seguros sin bloquear el renderer;
- API reactiva completa para trackers, signals y `always_redraw`.

### Fuera del foco inmediato

3D, WASM, Lottie, plugins y generación asistida por IA pueden ser diferenciadores, pero
no deben desplazar la estabilización 2D, los assets, el audio y el flujo de publicación.

---

## Roadmap propuesto por releases

### 0.3.x — Convergencia y confiabilidad

- [x] Terminar y probar el morph con regresión visual.
- [x] Recuperar `Scene` como fachada pública y deprecar `Canvas` como punto de entrada.
- [x] Separar el viewport/configuración visual en `scene.canvas`.
- [x] Migrar la documentación y los ejemplos que usaban la fachada `Canvas` a `Scene`.
- [ ] Completar la cobertura Python de las capacidades que ya son estables en Rust.
- [x] Corregir stubs, versiones y estrategia de empaquetado.
- [x] Añadir CI y regresión visual headless.
- [x] Publicar un quickstart reproducible.

### 0.4 — Producción de contenido vectorial

- [x] Imágenes raster PNG/JPEG/WebP con transform, alpha y cache de textura.
- [x] SVG vectorial básico: paths, formas, fills/strokes sólidos, CSS,
  transforms, `viewBox` y `<use>`.
- [ ] SVG avanzado: grupos origen direccionables, clipping, gradientes, filtros y texto.
- [ ] Audio con muxing en exportación.
- [ ] API de fuentes, gradientes y cámara.
- [ ] Diagramas técnicos dinámicos: paths, arcos/cotas y trackers reactivos.
- [ ] Componentes editoriales: listas, callouts, captions y title cards.
- [x] Presets 16:9, 9:16 y 1:1 con safe areas.
- [x] Perfil de exportación completamente accesible desde Python.

### 0.5 — Datos, código y plantillas

- [ ] Charts, tablas y matrices.
- [ ] Code mobject con highlighting y edición animada.
- [ ] Sistema de plantillas/temas público.
- [ ] Asset manager y formato mínimo de proyecto.
- [ ] Cache y hot reload incremental donde sea medible.

### 0.6 — Presentaciones

- [ ] Overview, indicadores y navegación por slide.
- [ ] Presenter view y notas.
- [ ] Exportación independiente por slide.
- [ ] Biblioteca de layouts de presentación.
- [ ] Interacción/picking útil para demos en vivo.

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
3. Timeline con clips, seek, snapshots, escenas y breakpoints.
4. Texto y matemáticas convertidos a geometría vectorial, con Typst integrado.
5. Modelo diferido de construcción de escenas, adecuado para hot reload y exportación
   determinista.
6. Separación entre API, runtime, editor y exportación.
7. Tipos 3D-ready sin comprometer el foco actual en 2D.
8. Uso directo de tipos gráficos (`peniko`, `kurbo`, `glam`) sin wrappers innecesarios.

La oportunidad de gaanim no está en copiar todo Manim o Motion Canvas. Está en convertir
esta base Rust/GPU en un flujo especialmente rápido y confiable para crear, iterar,
presentar y exportar contenido vectorial desde Python. El siguiente salto de calidad
depende más de coherencia de producto y capacidades audiovisuales básicas que de aumentar
el número bruto de efectos.
