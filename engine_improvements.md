# Gaanim — Roadmap to State of the Art

Este documento compara Gaanim contra **Manim CE** y **Motion Canvas**, identifica las brechas
reales y propone un roadmap para alcanzar (y superar) el estado del arte.

> [!NOTE]
> Última auditoría del codebase: **2026-07-01**. Se actualizó el documento removiendo
> todo lo que ya fue implementado. Solo queda lo que falta.

---

## Estado Actual V2 (`gaanim`) — Lo que ya existe ✅

### Animaciones (portadas y funcionando)
`TranslateTo`, `TranslateBy`, `RotateTo`, `RotateBy`, `ScaleTo`, `ScaleUniform`,
`FadeTo`, `FadeIn`, `FadeOut`, `FillColorTo`, `StrokeColorTo`, `StrokeWidthTo`,
`Write`, `Create`, `Unwrite`, `Uncreate`, `GrowFromCenter`, `ShrinkToCenter`,
`Indicate`, `GrowFromPoint`, `GrowFromEdge`, `GrowArrow`, `SpinInFromNothing`,
`DrawBorderThenFill`, `Wiggle`, `Flash`, `Circumscribe`, `ShowPassingFlash`,
`FadeTransform`, `MoveAlongPath`, `PathCompletion`.
Composición: `parallel` y `sequence` en la línea de tiempo.

### Rate Functions (100% completas)
`Linear`, `Smooth`, `DoubleSmooth`, `EaseIn/Out/InOut` con 10 curvas (Quad→Bounce),
`Spring`, `Steps`, `Mirror`, `ThereAndBack`, `ThereAndBackWithPause`, `Lingering`,
`RunningStart`, `ExponentialDecay`, `NotQuiteThere`, `CubicBezier`, `Custom`.

### Objetos (Mobjects)
*Primitivos:* `Circle`, `Rectangle`, `RoundedRectangle`, `Line`, `DashedLine`,
`Polygon`, `Star`, `Ellipse`, `Dot`, `Square`, `Triangle`, `RegularPolygon`,
`Checkmark`, `Arrow`, `DoubleArrow`, `Arc`, `ArcBetweenPoints`.
*Texto:* `Text` (cosmic-text + HarfBuzz), `Equation`/`TypstDocument` (Typst nativo).
*Datos:* `NumberPlane` (grilla cartesiana), `TangentLine`, `DecimalNumber`.

### Posicionamiento y Layout
`at()`, `shift()`, `scale()`, `rotate()`, `next_to()`, `align_to()`.
Grupos: `arrange()`, `arrange_in_grid()`, `vstack()`, `hstack()`.
Canvas margins: `canvas.margin_all(50)` — inset automático para `to_edge`/`to_corner`.

### Efectos Visuales
Gradientes (Linear, Radial, Conic) via `peniko::Brush`.
Componentes `DropShadow`, `Glow`, `GaussianBlur` definidos (rendering parcial).

### Sistema Reactivo
`ValueTracker` + `FloatSignal` + `DecimalNumber` + updaters (`Bob`, `Rotate`,
`Orbit`, `Pulse`, `Follow`) + `TracedPath` + `AlwaysRedraw` (Rust-side).

### `.animate` Syntax
`mob.animate().shift(x,y).scale(s).fill_color(c).duration(d).smooth()` — fluent chaining completo.

### Boolean Operations
`union()`, `intersection()`, `difference()`, `exclusion()` — implementadas via `i_overlay`.

### Multi-Escena (Engine API)
`PyEngine` con `PySceneBuilder` para múltiples escenas con transiciones.
Transiciones: `Cut`, `CrossFade`, `FadeThrough`, `Slide(dir)`, `ZoomThrough`, `Morph`.

### Modo Presentación (Slides)
`scene.slide()` inserta breakpoints. Navegación interactiva con Space/Enter/Arrows/Click.
`export_slides()` exporta cada segmento como archivo independiente.

### Exportación
GPU-direct headless export (Vello + wgpu), encoders (Nvenc/AMF/QSV/VA-API),
formatos WebP/WebM/MP4/GIF/PNG sequence, segmentos por rango de tiempo.

### Z-Index
`RenderOrder { z_index, creation_order }` con tiebreaker monotónico.

### Cámara
Struct `Camera` con proyección ortográfica, posición, zoom. Clips de bajo nivel
para `CameraPosition`, `CameraRotation`, `CameraZoom` via timeline seek.

### Otros
Theming, text roles (Title, Subtitle, Body, Caption, Code), color palette,
snapshot/seek interactivo, `@Scene("name")` segments, editor con timeline widget.

### Color API
Constructor `Color(r, g, b, a?)` + `from_hex()` + `from_rgb()` + constantes nombradas
(`RED`, `BLUE`, `GOLD`, etc.). Acepta strings CSS directamente donde se espera un Color:
`"#FF0000"`, `"#F00"`, `"red"`, `"rgb(255,0,0)"`, `"hsl(0,100%,50%)"`, tuplas `(r,g,b)`.

### Canvas Background
Rectángulo visual del canvas en el renderer (Vello) con color de fondo configurable.
`ClearColor` gris oscuro fuera del canvas para distinguir el área real.

### Editor
- Sin top bar — export y controles en la barra de reproducción
- Pin `📌` para always-on-top
- Speed popup con presets (0.25x–3x) + slider fino + reset
- Canvas background visible (gris oscuro fuera del canvas)

---

## 🔴 Brechas Críticas — Lo que falta para State of the Art

### 1. Animaciones Faltantes

| Animación | Descripción | En Manim | Esfuerzo |
|-----------|-------------|----------|----------|
| `Transform` | Morph de un mobject a otro (interpolación de paths) | ✅ Core | 🔴 12h |
| `ReplacementTransform` | Transform + reemplazar el original | ✅ Core | 🔴 4h (sobre Transform) |
| `TransformMatchingShapes` | Morph inteligente por forma similar | ✅ Transform | 🔴 12h |
| `TransformMatchingTex` | Morph inteligente por LaTeX submobjects | ✅ Transform | 🔴 12h |
| `ApplyWave` | Onda que deforma el path del objeto | ✅ Indication | 🟡 5h |
| `Homotopy` | Deformación continua con función `(x,y,z,t)→(x',y',z')` | ✅ Movement | 🔴 8h |

**Prioridad:**
1. `Transform` + `ReplacementTransform` — fundamental para contenido educativo
2. `TransformMatchingShapes/Tex` — killer feature de Manim
3. `ApplyWave`, `Homotopy` — variedad creativa

---

### 2. Animaciones de Cámara

Las animaciones de alto nivel de cámara no están portadas. Solo existen clips de bajo nivel.

| Animación | Descripción | Esfuerzo |
|-----------|-------------|----------|
| `CameraShake` | Sacudida tipo terremoto | 🟢 3h |
| `CameraPulse` | Zoom in/out rápido | 🟢 2h |
| `CameraFollow` | Seguir un mobject automáticamente | 🟡 4h |
| `CameraFrameTo` | Enfocar un rect/mobject con margin | 🟡 4h |

**Esfuerzo total:** ~13h

---

### 3. Mobjects Faltantes

| Mobject | Descripción | En Manim | Esfuerzo |
|---------|-------------|----------|----------|
| `CurvedArrow` | Flecha con curva | ✅ Core | 🟢 3h |
| `LabeledArrow` | Flecha con label de texto | ✅ Core | 🟡 4h |
| `Vector` | Flecha desde el origen (alias de Arrow) | ✅ Core | 🟢 1h |
| `Axes` | Par de ejes cartesianos con ticks y labels | ✅ Core | 🟡 6h |
| `NumberLine` | Línea numérica con ticks | ✅ Core | 🟡 4h |
| `Brace` | Llave decorativa con label | ✅ Core | 🟡 4h |
| `FunctionGraph` | Gráfica de f(x) sobre Axes | ✅ Core | 🟡 4h |
| `ParametricCurve` | Curva paramétrica | ✅ Core | 🟡 4h |
| `PolarPlane` | Plano polar | ✅ Core | 🟡 6h |
| `ComplexPlane` | Plano complejo | ✅ Core | 🟡 4h |
| `ImplicitCurve` | Curva definida implícitamente | ✅ Core | 🔴 8h |
| `Table` | Tabla con celdas y texto | ✅ Core | 🔴 12h |
| `Matrix` | Representación visual de matrices | ✅ Core | 🔴 8h |
| `BarChart` | Gráfico de barras animable | ✅ Core | 🟡 6h |
| `Code` | Código con syntax highlighting | ✅ Manim + MC | 🔴 16h |
| `ImageMobject` | Mostrar imágenes raster | ✅ Core | 🟡 6h |
| `SvgMobject` | Importar SVG como mobject | ✅ Core | 🔴 12h |

**Prioridad de implementación:**
1. `Vector`, `CurvedArrow` — quick wins
2. `Axes`, `NumberLine`, `FunctionGraph`, `ParametricCurve` — contenido matemático
3. `Brace`, `LabeledArrow` — anotaciones educativas
4. `Table`, `Matrix`, `BarChart` — contenido estructurado
5. `Code` — contenido CS/programming
6. `ImageMobject`, `SvgMobject` — contenido multimedia

---

### 4. Efectos Visuales (Rendering)

Los componentes ECS para `DropShadow`, `Glow` y `GaussianBlur` existen como structs,
pero el renderer tiene TODOs explícitos — el pipeline de Vello no los renderiza.

**Lo que falta:**
- Conectar `DropShadow` al pipeline de Vello (blur + offset del path)
- Implementar `GaussianBlur` en el render pass
- Implementar `Glow` como bloom alrededor del path
- Exponer gradientes en la Python API (actualmente solo disponibles via `peniko::Brush` en Rust)

**Esfuerzo:** 🔴 ~16h (requiere trabajo en el render pipeline de Vello)

---

### 5. Code Mobject (Syntax Highlighting)

**Nivel 1 — Display (Manim parity):**
- Renderizar código con syntax highlighting (via `syntect` o `tree-sitter`)
- Soporte para ~50 lenguajes
- Line numbers opcionales
- Estilos de formatter (Monokai, Dracula, etc.)
- Destacar líneas/rangos específicos

**Nivel 2 — Animation (Motion Canvas parity):**
- `code.edit("new source")` → diff animado automático
- Inserción/eliminación de líneas con animación
- Highlight de líneas que cambian
- Cursor animado

**Esfuerzo:** 🔴 Nivel 1: ~16h, Nivel 2: ~24h adicionales

---

### 6. Graph Theory (Network Visualization)

Manim tiene `Graph` y `DiGraph` con 10+ algoritmos de layout automático.

**Lo que falta:**
- `Graph` / `DiGraph` — grafo no dirigido y dirigido
- Algoritmos de layout: `spring`, `circular`, `kamada_kawai`, `tree`, `planar`, `shell`, `spiral`, `partite`
- Vertex customization (mobjects arbitrarios como nodos)
- Edge labels
- Auto-updaters (edges siguen a vertices al moverlos)
- Animaciones específicas: agregar/remover vértices y aristas con animación

**Esfuerzo:** 🔴 ~20h

---

### 7. 3D Support

Manim tiene 3D completo. Motion Canvas no tiene 3D. Es el gap más grande.

**Nivel 1 — Pseudo-3D (proyección):**
- Transformaciones de perspectiva en paths 2D
- Rotación aparente de objetos planos
- Esfuerzo: 🟡 ~8h

**Nivel 2 — 3D Real:**
- `ThreeDScene` con cámara 3D (phi, theta, focal_distance)
- Primitivos: `Sphere`, `Cube`, `Cylinder`, `Cone`, `Torus`, `Surface`
- `ThreeDAxes`, `Arrow3D`, `Line3D`
- Iluminación básica (ambient + point)
- Render via wgpu 3D pipeline o ray marching en Vello
- Esfuerzo: 🔴🔴 ~80-120h (proyecto mayor)

> [!WARNING]
> 3D es un proyecto enorme. Manim lo hace con CPU rendering (lento). Gaanim
> podría usar wgpu para 3D acelerado por GPU, lo cual sería una ventaja competitiva,
> pero el esfuerzo es significativo.

---

### 8. Audio Integration

Ni Manim ni Gaanim tienen audio nativo robusto. Motion Canvas sí.

**Componentes:**
- `AudioTrack` resource con offset y sync al timeline
- Mezcla de múltiples tracks
- Waveform visualization en el editor
- `on_beat(bpm, beat)` helper
- Audio en exports (muxing con ffmpeg)

**Esfuerzo:** 🔴 ~20h

---

### 9. Interactividad (Eventos)

El manejo de eventos `on_hover`, `on_click` y `on_drag` está pendiente.

**Componentes:**
- `InteractionTarget` component con bounding box hit testing
- `on_hover(callback)`, `on_click(callback)`, `on_drag(callback)` en Python API
- Integración con el event loop de Bevy
- Cursor feedback visual

**Esfuerzo:** 🟡 ~12h

---

### 10. Modo Presentación — Mejoras pendientes

El modo básico (`slide()` + navegación) ya funciona. Falta:

- **Overview mode:** thumbnails miniatura de todas las diapositivas
- **Slide indicator:** barra de progreso o número de slide
- **Presenter notes:** notas visibles solo para el presentador (dual screen)

**Esfuerzo:** 🟡 ~8h

---

## 🟡 Mejoras de Calidad — Polish

### 11. `always_redraw` en Python

El componente `AlwaysRedraw` existe en Rust pero no tiene binding en Python.

```python
# Objetivo
circle = scene.always_redraw(lambda: scene.circle(tracker.get()).fill(BLUE))
```

**Esfuerzo:** 🟢 ~4h

---

### 12. Flexbox Layout Engine (Motion Canvas Style)

Motion Canvas usa Flexbox CSS para layout. Gaanim ya tiene `arrange()`, `vstack()`,
`hstack()`. Un Flexbox completo sería el siguiente nivel.

```python
with flex(direction="column", gap=20, align="center") as container:
    text("Title", role="title").add()
    with flex(direction="row", gap=10) as row:
        rectangle(100, 100).color(RED).add()
        rectangle(100, 100).color(BLUE).add()
```

**Esfuerzo:** 🔴 ~16h

---

### 13. Gradient API en Python

Los gradientes funcionan a nivel de Rust via `peniko::Brush`, pero no hay
API de Python para crearlos en mobjects.

```python
# Objetivo
rect = scene.rectangle(200, 100).gradient(
    type="linear", start=(0, 0), end=(200, 0),
    stops=[(0, RED), (1, BLUE)]
)
```

**Esfuerzo:** 🟡 ~6h

---

## 🟠 Editor & UX — Alto Impacto

### 14. Canvas Zoom/Pan Interactivo

El canvas es estático. Poder hacer scroll wheel para zoom y drag para panear sería
enorme para escenas complejas. El `Camera` ya soporta posición y zoom, solo falta
el input handling en el editor.

- Scroll wheel → zoom in/out centrado en el cursor
- Middle-click drag (o Ctrl+drag) → pan
- `F` → fit canvas en la ventana
- `R` → reset zoom a 1x

**Esfuerzo:** 🟡 ~6h (input handling + Camera transform)

---

### 15. Property Panel en el Editor

Seleccionar un mobject y ver/editar propiedades en tiempo real sin tocar el código.
El picking system ya existe (`editor_picking_system`), falta el panel egui.

- Posición (x, y), escala, rotación
- Fill color, stroke color, stroke width
- Opacidad, z-index
- Edición inline con cambios en tiempo real

**Esfuerzo:** 🟡 ~10h (egui panel + write-back al ECS)

---

### 16. Snap & Alignment Guides

Cuando se arrastra un objeto, mostrar líneas guía cuando se alinea con otros
objetos (centro, bordes). Estilo Figma/PowerPoint. El `WorldBounds` ya está
calculado para todos los mobjects.

- Snap to center (horizontal/vertical)
- Snap to edges (left, right, top, bottom)
- Líneas guía visuales (dashed, color configurable)
- Snap distance configurable

**Esfuerzo:** 🔴 ~12h (spatial queries + overlay rendering + drag interaction)

---

### 17. Hot-reload Inteligente (Incremental)

Ahora re-ejecuta todo el script en cada save. Podría hacer diff incremental —
si solo cambió una posición, no recompilar toda la escena.

- Hash de cada op en el segmento
- Solo re-compilar ops que cambiaron
- Preservar estado ECS entre re-loads
- Reducir latencia de hot-reload de ~200ms a ~20ms

**Esfuerzo:** 🔴 ~16h (sistema de dependencias + diff engine)

---

### 18. Animation Preview sin Recompilar

Mostrar una preview de la animación actual sin re-ejecutar el script.
El timeline ya tiene los clips, solo falta poder reproducir desde el estado
actual del ECS.

- Play/pause sin re-ejecutar script
- Seek a cualquier punto del timeline
- Preview de transiciones entre escenas

**Esfuerzo:** 🟡 ~8h (desacoplar playback de script execution)

---

### 19. Object Tree / Scene Graph Panel

Panel lateral que muestre la jerarquía de mobjects (grupos, hijos).
Click para seleccionar, drag para reordenar z-index.

- Árbol expandible con indentación
- Click → seleccionar mobject
- Drag → reordenar z-index
- Iconos por tipo (circle, rect, text, group)
- Hide/lock por objeto

**Esfuerzo:** 🟡 ~8h (egui tree widget + ECS query)

---

### 20. Error Recovery Visual

Cuando el script falla, mostrar el error inline en el canvas (overlay rojo
con el traceback) en vez de solo en la consola. Que el editor no se cierre.

- Overlay semi-transparente con el traceback
- Click para dismiss
- Auto-dismiss al re-ejecutar exitosamente
- Preservar la última escena válida como fondo

**Esfuerzo:** 🟢 ~4h (overlay egui + error channel)

---

### 21. Template/Preset System

Templates reutilizables que generan animaciones completas con un solo call.

```python
from gaanim.templates import title_card, bullet_list, code_block

scene.play(title_card("Mi tema", subtitle="Subtítulo"))
scene.play(bullet_list(["Punto 1", "Punto 2", "Punto 3"]))
```

**Esfuerzo:** 🟡 ~12h (library de templates + API de composición)

---

### 22. Screen Recording Integrado

Un botón en el editor que grabe la pantalla del canvas como GIF/MP4
directamente, sin pasar por el export formal. Útil para iterar rápido
y compartir previews.

- Grabar desde el frame actual
- Stop → genera archivo temporal
- Copy to clipboard / save as file
- Configurable: fps, resolution, format

**Esfuerzo:** 🟡 ~8h (capture pipeline + encoding)

---

### 23. Clipboard de Animaciones

Copiar una animación de un objeto y pegarla en otro.

```python
circle.copy_animation_from(rect)  # copia fade_in, stroke, etc.
```

**Esfuerzo:** 🟢 ~4h (serialización de AnimationBuilder)

---

## 🟢 Quick Wins (< 2h cada uno)

### 24. Keyboard Shortcuts en el Editor

| Shortcut | Acción |
|----------|--------|
| `F` | Fit/fill canvas en la ventana |
| `G` | Toggle grid overlay |
| `R` | Reset zoom a 1x |
| `Ctrl+E` | Export rápido (settings por defecto) |
| `I` | Toggle info overlay (dimensiones, zoom, fps) |
| `H` | Toggle help overlay con shortcuts |

**Esfuerzo:** 🟢 ~2h (input handling en `global_playback_keys_system`)

---

### 25. Grid Overlay Opcional

Mostrar una grilla de referencia sobre el canvas (toggle con `G`).
Ayuda al posicionamiento manual.

- Grid cada 50px o configurable
- Color sutil (no interferir con el contenido)
- Toggle con shortcut `G`

**Esfuerzo:** 🟢 ~2h (Vello scene overlay)

---

### 26. Canvas Info Overlay

Mostrar en una esquina las dimensiones del canvas, zoom actual, y fps.
Toggle con `I`.

- Canvas: 1280×720
- Zoom: 1.0x
- FPS: 60
- Scene: "intro"

**Esfuerzo:** 🟢 ~1h (egui overlay, similar al fps_overlay existente)

---

### 27. Export Rápido (One-Click)

Un botón `⬇` que exporte con settings por defecto (MP4, 60fps, production
quality) sin abrir el diálogo. Ya existe el botón de export, agregar
shift+click para export rápido.

**Esfuerzo:** 🟢 ~1h (shortcut en el export button)

---

## 🟢 Nice to Have — Diferenciadores

### 28. Plugin System
Custom mobjects, animations, rate functions, temas/paletas desde la comunidad.

### 29. WASM Export
Compilar animaciones a WebAssembly para reproducción en el browser sin video.

### 30. SVG/Lottie Export
Exportar animaciones como SVG animado o Lottie JSON para uso en web/mobile.

### 31. AI-Assisted Animation
Integración con LLMs para generar animaciones desde descripción en lenguaje natural.

### 32. Transición Wipe
Falta la transición `Wipe(direction)` (ya existen Cut, CrossFade, FadeThrough,
Slide, ZoomThrough, Morph).

---

## Resumen Comparativo

| Categoría | Manim CE | Motion Canvas | Gaanim | Gap |
|-----------|----------|---------------|----------|-----|
| Rate Functions | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ✅ Superado |
| Animaciones | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ | 🟡 Faltan Transform/Morph |
| Mobjects | ⭐⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | 🔴 Faltan ~17 |
| Posicionamiento | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 🟢 Casi par (+ margins) |
| Cámara | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | 🟡 Faltan anims de alto nivel |
| Efectos | ⭐⭐ | ⭐⭐ | ⭐⭐ | 🟡 Componentes sin render |
| Color API | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ | ✅ Hex/RGB/CSS strings |
| Exportación | ⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐⭐⭐ | ✅ Superado |
| Updaters/Signals | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 🟢 Falta `always_redraw` Python |
| Editor | ⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 🟢 Casi par |
| 3D | ⭐⭐⭐⭐ | ❌ | ❌ | 🔴 No hay |
| Code Highlight | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ❌ | 🔴 No hay |
| Graph Theory | ⭐⭐⭐⭐ | ❌ | ❌ | 🔴 No hay |
| Multi-Scene | ❌ | ❌ | ⭐⭐⭐⭐ | ✅ Superado |
| Presentación | ⭐⭐ (plugin) | ⭐⭐⭐⭐ | ⭐⭐⭐ | 🟡 Falta overview |
| Audio | ❌ | ⭐⭐⭐ | ❌ | 🟡 No hay |
| Performance | ⭐⭐ (CPU) | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ (GPU) | ✅ Superado |

---

## 🗺️ Roadmap Propuesto

### Sprint 1 — Mobjects Matemáticos (1 semana) [Completado ✅ 2026-06-10]
- [x] `Vector` (alias de Arrow desde origen)
- [x] `CurvedArrow`
- [x] `Axes` + `NumberLine` + ticks + labels
- [x] `FunctionGraph` (f(x) sobre Axes)
- [x] `ParametricCurve`
- [x] `Brace` + `LabeledArrow`

### Sprint 1.5 — Editor & API Polish [Completado ✅ 2026-07-01]
- [x] Canvas background visual (rectángulo Vello + ClearColor gris oscuro)
- [x] Canvas margins (`margin_all()`, `margin(Margin::hv())`)
- [x] Color API: hex strings, CSS colors, tuplas via `FromPyObject`
- [x] Editor: top bar eliminada, controles en playback bar
- [x] Editor: pin always-on-top (`📌`)
- [x] Editor: speed popup con presets + slider fino
- [x] Editor: export button en playback controls
- [x] `EditorQueries` SystemParam bundle (Bevy 16-param limit)

### Sprint 2 — Transform/Morph (1 semana)
- [ ] `Transform` (interpolación de paths entre dos mobjects)
- [ ] `ReplacementTransform`
- [ ] Cámara de alto nivel: `CameraShake`, `CameraPulse`, `CameraFollow`, `CameraFrameTo`

### Sprint 2.5 — Editor & UX (1 semana)
- [ ] Canvas zoom/pan interactivo (scroll + drag + shortcuts F/R)
- [ ] Grid overlay opcional (shortcut `G`)
- [ ] Canvas info overlay (shortcut `I`)
- [ ] Keyboard shortcuts (`Ctrl+E` export rápido, `H` help)
- [ ] Error recovery visual (overlay con traceback)
- [ ] Export rápido one-click (shift+click)

### Sprint 3 — Contenido Educativo (1-2 semanas)
- [ ] `Table` y `Matrix` (visualización estructurada)
- [ ] `BarChart` (gráficos de barras dinámicos)
- [ ] `PolarPlane`, `ComplexPlane`
- [ ] `ImageMobject`

### Sprint 4 — Code & Efectos (1-2 semanas)
- [ ] `Code` mobject con syntax highlighting (syntect/tree-sitter)
- [ ] Conectar rendering de `DropShadow`, `Glow`, `GaussianBlur` en Vello
- [ ] Gradient API en Python
- [ ] `always_redraw` binding en Python

### Sprint 5 — Avanzado (2-4 semanas)
- [ ] Graph theory layouts (`Graph`, `DiGraph`, spring, circular, tree, etc.)
- [ ] `TransformMatchingShapes` / `TransformMatchingTex`
- [ ] Audio integration
- [ ] Interactividad (`on_hover`, `on_click`, `on_drag`)
- [ ] `Homotopy`, `ApplyWave`
- [ ] `SvgMobject`

### Sprint 6 — Presentación Polish
- [ ] Overview mode (thumbnails de slides)
- [ ] Slide indicator / progress bar
- [ ] Presenter notes (dual screen)
- [ ] Transición `Wipe`

### Futuro — Editor Avanzado
- [ ] Property panel en editor (editar propiedades en tiempo real)
- [ ] Object tree / scene graph panel
- [ ] Snap & alignment guides (estilo Figma)
- [ ] Hot-reload inteligente (incremental, solo re-compilar lo que cambió)
- [ ] Animation preview sin recompilar
- [ ] Template/preset system
- [ ] Screen recording integrado
- [ ] Clipboard de animaciones

### Futuro — Diferenciadores
- [ ] 3D support (proyecto mayor, ~2-3 meses)
- [ ] WASM export para web
- [ ] SVG/Lottie export
- [ ] Plugin system
- [ ] Flexbox layout engine

---

## Ventajas Competitivas de Gaanim (ya existentes)

1. **GPU-accelerated rendering** via Vello + wgpu — Manim usa CPU (Cairo), MC usa browser canvas
2. **Exportación GPU** con múltiples encoders (Nvenc, AMF, QSV, VA-API) — Manim usa ffmpeg CPU
3. **Rate functions completas** — Más opciones que Manim incluyendo Spring physics
4. **Multi-escena nativa** — Engine con DAG de escenas + transiciones (Cut, CrossFade, Slide, Morph, etc.)
5. **Modo presentación** — Slide breakpoints con navegación interactiva + export por slide
6. **TracedPath avanzado** — Fading, smoothing, dissipation, dash patterns
7. **Sistema de temas** — Color + text themes con roles semánticos
8. **Boolean operations** — Union, Intersection, Difference, Exclusion sobre shapes
9. **Snapshots + seek** — Timeline bidireccional para el editor
10. **Typst math** — Rendering de fórmulas sin instalar LaTeX
11. **ValueTracker + updaters** — Sistema reactivo completo con señales, bob, orbit, pulse, follow
12. **`.animate` fluent API** — Encadenamiento ergonómico estilo Manim
13. **Color API ergonómica** — Hex strings, CSS colors, tuplas directamente en cualquier método
14. **Canvas margins** — Layout operations respetan insets configurables del canvas
15. **Canvas background visual** — Área del canvas distinguible del fondo de ventana en el editor
