# Gaanim — Roadmap to State of the Art

Este documento compara Gaanim contra **Manim CE** y **Motion Canvas**, identifica las brechas
reales y propone un roadmap para alcanzar (y superar) el estado del arte.

> [!NOTE]
> Última auditoría del codebase: **2026-06-10**. Se actualizó el documento removiendo
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

## 🟢 Nice to Have — Diferenciadores

### 14. Plugin System
Custom mobjects, animations, rate functions, temas/paletas desde la comunidad.

### 15. WASM Export
Compilar animaciones a WebAssembly para reproducción en el browser sin video.

### 16. SVG/Lottie Export
Exportar animaciones como SVG animado o Lottie JSON para uso en web/mobile.

### 17. AI-Assisted Animation
Integración con LLMs para generar animaciones desde descripción en lenguaje natural.

### 18. Transición Wipe
Falta la transición `Wipe(direction)` (ya existen Cut, CrossFade, FadeThrough,
Slide, ZoomThrough, Morph).

---

## Resumen Comparativo

| Categoría | Manim CE | Motion Canvas | Gaanim | Gap |
|-----------|----------|---------------|----------|-----|
| Rate Functions | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ✅ Superado |
| Animaciones | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ | 🟡 Faltan Transform/Morph |
| Mobjects | ⭐⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | 🔴 Faltan ~17 |
| Posicionamiento | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 🟢 Casi par |
| Cámara | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | 🟡 Faltan anims de alto nivel |
| Efectos | ⭐⭐ | ⭐⭐ | ⭐⭐ | 🟡 Componentes sin render |
| Exportación | ⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐⭐⭐ | ✅ Superado |
| Updaters/Signals | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 🟢 Falta `always_redraw` Python |
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

### Sprint 2 — Transform/Morph (1 semana)
- [ ] `Transform` (interpolación de paths entre dos mobjects)
- [ ] `ReplacementTransform`
- [ ] Cámara de alto nivel: `CameraShake`, `CameraPulse`, `CameraFollow`, `CameraFrameTo`

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
