# Gaanim — Roadmap to State of the Art

Este documento compara Gaanim contra **Manim CE** y **Motion Canvas**, identifica las brechas
reales y propone un roadmap para alcanzar (y superar) el estado del arte.

> [!NOTE]
> Última auditoría del codebase: **2026-05-29**. Muchos items que antes se listaban como
> "faltantes" ya fueron implementados. Este documento refleja el estado real.

---

## Estado Actual V2 (`gaanim`) — Auditoría de Portado ✅

### 1. Animaciones
*   **Portado/Implementado en V2:** `TranslateTo`, `TranslateBy`, `RotateTo`, `RotateBy`, `ScaleTo`, `ScaleUniform`, `FadeTo`, `FadeIn`, `FadeOut`, `FillColorTo`, `StrokeColorTo`, `StrokeWidthTo`, `Write`.
    *   *Composición:* `parallel` y `sequence` en la base de la línea de tiempo.
*   **Pendiente de portar desde V1:** `Transform`, `ReplacementTransform`, `Create`, `Unwrite`, `GrowFromCenter`, `ShrinkToCenter`, `Uncreate`, `Indicate`, `FillTo`, `StrokeTo`, `StrokeWidthTo`, `ColorTo`.
*   **Cámara en V2:** El timeline soporta interpolar `CameraPosition`, `CameraRotation` y `CameraZoom` a través de seek/playback de clips de bajo nivel. Las animaciones de alto nivel como `CameraShake`, `CameraPulse`, `CameraFollow` y `CameraFrameTo` están pendientes de portar.

### 2. Rate Functions (100% Portado en V2)
*   `Linear`, `Smooth` (Hermite 3t^2 - 2t^3), `DoubleSmooth` (aplicado doble).
*   `EaseIn`, `EaseOut` y `EaseInOut` con curvas: `Quadratic`, `Cubic`, `Quartic`, `Quintic`, `Exponential`, `Sine`, `Circular`, `Back`, `Elastic`, `Bounce`.
*   `Spring { stiffness, damping }` (simulación física de resorte amortiguado).
*   `Steps(n)`, `Mirror`, `ThereAndBack`, `ThereAndBackWithPause`, `Lingering`, `RunningStart`, `CubicBezier` (CSS-style) y `Custom(closure)`.

### 3. Objetos (Mobjects)
*   **Portado/Implementado en V2:** 
    *   *Primitivos:* `Circle`, `Rectangle`, `RoundedRectangle` (como `rounded_rect`), `Line`, `Polygon`, `Star`, `Ellipse`, `Dot`, `Square`, `Triangle`, `RegularPolygon`, `Checkmark` (✓), y `Arrow` (con cabeza triangular sólida).
    *   *Texto:* `Text` (con shaping de cosmic-text y HarfBuzz + ttf-parser) y `Equation` / `TypstDocument` (compilación nativa de fórmulas matemáticas y markup Typst con fuente New Computer Modern Math).
*   **Pendiente de portar desde V1:** 
    *   *Geometría especial:* `DashedLine`, `Arc`, `ArcBetweenPoints`, `DoubleArrow`, `CurvedArrow`, `LabeledArrow`, `Vector`, `Axes`, `NumberLine`, `Brace`, `FunctionGraph`, `ParametricCurve`.
    *   *Estructuras y Multimedia:* `Table`, `Matrix`, `BarChart`, `ImageMobject`, `SvgMobject`.

### 4. Posicionamiento (100% Portado en V2)
*   `at()` (inicializar transform 2D), `shift()` (desplazar coordenadas), `scale()`, `rotate()`.
*   `next_to()` (posicionamiento adyacente centrado y relativo con espaciado).
*   `align_to()` (alineación de bordes y esquinas de cajas delimitadoras).

### 5. Efectos Visuales
*   **Portado/Implementado en V2:** `Gradient` (Linear, Radial, Conic), `DropShadow` (sombra proyectada).
*   **Pendiente de portar desde V1:** `Glow` y `GaussianBlur`.

### 6. Updaters, Interactividad y Exportación (Pendientes de portar a V2)
*   **Updaters:** En V2 solo existen las directivas `Signal`, `SignalBinding` y `AlwaysRedraw` a bajo nivel en ECS. Los modificadores visuales continuos como `Bob`, `Rotate`, `Orbit`, `Pulse`, `Follow`, `AnchorPinTo...` y `TracedPath` aún no han sido portados.
*   **Interactividad:** El manejo de eventos `on_hover`, `on_click` y `on_drag` está pendiente de estructuración en un crate ECS reactivo.
*   **Exportación:** El previewer en tiempo real con Vulkan/Vello está funcional en V2 (`scene.render()`), pero el pipeline de exportación directa a archivos de video (`gaanim_export` con codificadores Nvenc/AMF/QSV) está pendiente de desarrollo en V2.

### 7. Otros
*   Theming, text roles (Title, Subtitle, Body, Caption, Math), color palette extensa, `ZIndex` con creation order como tiebreaker, snapshot/seek system interactivo en el timeline, `@Scene("name")` segments.

---

## 🔴 Brechas Críticas — Lo que falta para State of the Art

### 1. Mobjects Faltantes

| Mobject | Descripción | En Manim | Esfuerzo |
|---------|-------------|----------|----------|
| `Checkmark` | Marca ✓ | ❌ | 🟢 1h |
| `NumberPlane` | Plano cartesiano con grid | ✅ Core | 🟡 6h |
| `PolarPlane` | Plano polar | ✅ Core | 🟡 6h |
| `ComplexPlane` | Plano complejo | ✅ Core | 🟡 4h |
| `ImplicitCurve` | Curva definida implícitamente | ✅ Core | 🔴 8h |
| `Table` | Tabla con celdas y texto | ✅ Core | 🔴 12h |
| `Matrix` | Representación visual de matrices | ✅ Core | 🔴 8h |
| `BarChart` | Gráfico de barras animable | ✅ Core | 🟡 6h |
| `Code` | Código con syntax highlighting | ✅ Manim + MC | 🔴 16h |
| `TangentLine` | Línea tangente a una curva | ✅ Core | 🟡 4h |

**Prioridad de implementación:**
1. `Checkmark`, `TangentLine` — quick wins
2. `NumberPlane`, `Table`, `Matrix` — contenido educativo
3. `Code` — contenido CS/programming

---

### 2. Animaciones Faltantes

| Animación | Descripción | En Manim | Esfuerzo |
|-----------|-------------|----------|----------|
| `MoveAlongPath` | Mover objeto siguiendo una curva Bézier | ✅ Core | 🟡 6h |
| `Wiggle` | Sacudida horizontal/angular | ✅ Indication | 🟢 3h |
| `Circumscribe` | Círculo/rect que resalta un objeto y desaparece | ✅ Indication | 🟡 4h |
| `Flash` | Líneas radiales que salen de un punto | ✅ Indication | 🟡 4h |
| `ShowPassingFlash` | Destello que recorre un path | ✅ Indication | 🟡 5h |
| `SpinInFromNothing` | Rotar + escalar desde 0 al aparecer | ✅ Growing | 🟢 2h |
| `GrowFromEdge` | Crecer desde un borde específico | ✅ Growing | 🟢 3h |
| `GrowFromPoint` | Crecer desde un punto arbitrario | ✅ Growing | 🟢 3h |
| `GrowArrow` | Animación especializada para flechas | ✅ Growing | 🟢 2h |
| `DrawBorderThenFill` | Trazar contorno y luego rellenar | ✅ Creation | 🟡 5h |
| `FadeTransform` | Fade out source + fade in target (sin morph) | ✅ Transform | 🟢 3h |
| `TransformMatchingShapes` | Morph inteligente por forma similar | ✅ Transform | 🔴 12h |
| `TransformMatchingTex` | Morph inteligente por LaTeX submobjects | ✅ Transform | 🔴 12h |
| `ApplyWave` | Onda que deforma el path del objeto | ✅ Indication | 🟡 5h |
| `Homotopy` | Deformación continua con función `(x,y,z,t)→(x',y',z')` | ✅ Movement | 🔴 8h |
| `ChangeDecimalToValue` | Animar un número cambiando su valor | ✅ Numbers | 🟡 4h |

**Prioridad:**
1. Exponer animaciones existentes en Python API (1h)
2. `MoveAlongPath`, `SpinInFromNothing`, `GrowFromEdge` — alto impacto visual
3. `Circumscribe`, `Flash`, `Wiggle` — indication animations populares
4. `DrawBorderThenFill`, `FadeTransform` — variedad creativa
5. `TransformMatchingShapes/Tex` — killer feature de Manim

---

### 3. Sistema de Valores Reactivos (ValueTracker)

Manim tiene `ValueTracker` + `always_redraw()`. Motion Canvas tiene **signals**.
Gaanim no tiene ninguno de los dos.

```python
# Manim pattern
tracker = ValueTracker(0)
circle = always_redraw(lambda: Circle(radius=tracker.get_value()))
self.play(tracker.animate.set_value(3))

# Propuesta Gaanim
tracker = value_tracker(0)
circle = always_redraw(lambda: circle(tracker.get()).add())
play(tracker.animate_to(3, duration=2.0))
```,StartLine:131,TargetContent:

**Componentes necesarios:**
- `ValueTracker` — contenedor animable de un valor numérico
- `always_redraw(fn)` — reconstruir mobject cada frame
- `DecimalNumber` — texto que muestra un número animable
- `ComplexValueTracker` — para números complejos

**Esfuerzo:** 🔴 ~16h (incluye engine + Python API + DecimalNumber)

---

### 4. Code Mobject (Syntax Highlighting)

Tanto Manim como Motion Canvas tienen un objeto `Code` para mostrar código con
syntax highlighting. Motion Canvas va más allá con **animación de edición** (diffs automáticos).

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

### 5. Graph Theory (Network Visualization)

Manim tiene `Graph` y `DiGraph` con 10+ algoritmos de layout automático.

**Lo que falta:**
- `DiGraph` — grafo dirigido
- Algoritmos de layout: `spring`, `circular`, `kamada_kawai`, `tree`, `planar`, `shell`, `spiral`, `partite`
- Vertex customization (mobjects arbitrarios como nodos)
- Edge labels
- Auto-updaters (edges siguen a vertices al moverlos)
- Animaciones específicas: agregar/remover vértices y aristas con animación

**Esfuerzo:** 🔴 ~20h (incluye múltiples algoritmos de layout)

---

### 6. 3D Support

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

### 7. Modo Presentación (Slides)

Motion Canvas tiene `beginSlide()` con navegación por teclado.
Manim tiene el plugin `manim-slides`.

**Propuesta:**
```python
@Scene("Intro")
def intro():
    title = text("My Talk", role="title").add().show()
    play(title.write(0.5))
    slide()  # ← pausa, espera input del usuario para continuar
    
    subtitle = text("Chapter 1", role="subtitle").add().show()
    play(subtitle.fade_in(0.3))
    slide()  # ← siguiente slide
```

**Componentes:**
- `slide()` — inserta breakpoint en el timeline
- Navegación: flechas, spacebar, click
- Overview mode: thumbnails de todos los slides
- Export: un video por slide o video completo

**Esfuerzo:** 🟡 ~12h

---

### 8. Audio Integration

Ni Manim ni Gaanim tienen audio nativo robusto. Motion Canvas sí.

**Propuesta:**
```python
add_audio("music.mp3", offset=0.0)
add_audio("narration.wav", offset=2.5)

# Sync con timeline
play(circle.create(), start_time=on_beat(120, 4))

# Waveform en el editor UI
```

**Componentes:**
- `AudioTrack` resource con offset y sync al timeline
- Mezcla de múltiples tracks
- Waveform visualization en el editor
- `on_beat(bpm, beat)` helper
- Audio en exports (muxing con ffmpeg)

**Esfuerzo:** 🔴 ~20h

---

### 9. Boolean Operations on Shapes

Manim tiene `Union`, `Intersection`, `Difference`, `Exclusion` para combinar geometrías.

```python
# Propuesta
result = union(circle, square)
result = intersection(circle, square)  
result = difference(circle, square)    # circle - square
result = exclusion(circle, square)     # XOR
```

**Implementación:** Usar una crate como `geo` o `i_overlay` para operaciones booleanas
en paths 2D, luego convertir el resultado a `BezPath`.

**Esfuerzo:** 🟡 ~8h (con crate existente)

---

### 10. Mejoras al Z-Index / Rendering Order

El `ZIndex` component existe pero hay bugs reportados: texto creado después de un
rectángulo puede renderizarse debajo. El sort actual usa `entity.index()` como fallback,
lo cual no respeta el orden de `add()` en Python cuando hay entidades intermedias
(glyphs de texto).

**Fix necesario:**
- Asignar un "creation order" monotónico (`u64`) a cada entidad visible en `scene.add()`
- Usar ese order como tiebreaker en lugar de `entity.index()`
- Propagar el order a hijos (glyphs de texto) basado en el parent

**Esfuerzo:** 🟡 ~4h

---

## 🟡 Mejoras de Calidad — Polish

### 11. Python API — Exponer animaciones del engine

Estas animaciones **ya existen en el engine** pero no están expuestas como métodos
de `MobjectHandle` en Python:

| Animación | Engine | Python API |
|-----------|--------|------------|
| `grow_from_center()` | ✅ `AnyAnimation::GrowFromCenter` | ❌ No expuesto |
| `shrink_to_center()` | ✅ `AnyAnimation::ShrinkToCenter` | ❌ No expuesto |
| `uncreate()` | ✅ `AnyAnimation::Uncreate` | ❌ No expuesto |
| `indicate()` | ✅ `AnyAnimation::Indicate` | ❌ No expuesto |

**Esfuerzo:** 🟢 ~1h

---

### 12. `.animate` Syntax (Manim Compatibility)

El patrón `.animate` de Manim es extremadamente ergonómico:

```python
# Manim
self.play(circle.animate.shift(RIGHT).set_color(RED).scale(2))

# Propuesta Gaanim  
play(circle.animate().shift(2, 0).color(RED).scale(2).build(duration=1.0))
```

Requiere un `AnimateBuilder` que acumule transformaciones y genere un `Composite` animation.

**Esfuerzo:** 🟡 ~8h

---

### 13. Flexbox Layout Engine (Motion Canvas Style)

Motion Canvas usa Flexbox CSS para layout, lo cual es mucho más intuitivo que
coordenadas manuales para UIs complejas.

```python
# Propuesta
with flex(direction="column", gap=20, align="center") as container:
    text("Title", role="title").add()
    with flex(direction="row", gap=10) as row:
        rectangle(100, 100).color(RED).add()
        rectangle(100, 100).color(BLUE).add()
    text("Caption", role="caption").add()
```

> [!TIP]
> Gaanim ya tiene `arrange()`, `arrange_in_grid()`, `stack()`, `vstack()`, `hstack()`,
> `grid()`, `vbox()`, `hbox()`, `gridbox()`. Un Flexbox completo sería el siguiente nivel.

**Esfuerzo:** 🔴 ~16h

---

### 14. Rate Functions adicionales de Manim

Aunque Gaanim tiene todas las easing estándar, Manim tiene algunas funciones
únicas que faltan:

| Rate Function | Descripción |
|---------------|-------------|
| `double_smooth` | Smooth aplicado dos veces (más suave) |
| `lingering` | Se queda más tiempo al final |
| `exponential_decay` | Decaimiento exponencial |
| `running_start` | Retrocede antes de avanzar (diferente de BackIn) |
| `not_quite_there` | No llega completamente al final |
| `there_and_back_with_pause` | Ida y vuelta con pausa en el pico |

**Esfuerzo:** 🟢 ~2h

---

## 🟢 Nice to Have — Diferenciadores

### 15. Plugin System

Permitir que la comunidad extienda Gaanim con plugins:
- Custom mobjects
- Custom animations
- Custom rate functions
- Temas/paletas

### 16. WASM Export

Compilar animaciones a WebAssembly para reproducción en el browser sin video.
Esto sería un diferenciador único vs Manim.

### 17. SVG/Lottie Export

Exportar animaciones como SVG animado o Lottie JSON para uso en web/mobile.

### 18. AI-Assisted Animation

Integración con LLMs para generar animaciones desde descripción en lenguaje natural.

---

## Resumen Comparativo

| Categoría | Manim CE | Motion Canvas | Gaanim | Gap |
|-----------|----------|---------------|----------|-----|
| Rate Functions | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ✅ Superado |
| Animaciones | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ | 🟡 Faltan ~15 |
| Mobjects | ⭐⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | 🔴 Faltan ~20 |
| Posicionamiento | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 🟢 Casi par |
| Cámara | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐⭐⭐ | ✅ Superado |
| Efectos | ⭐⭐ | ⭐⭐ | ⭐⭐⭐ | ✅ Superado |
| Exportación | ⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐⭐⭐ | ✅ Superado |
| Updaters | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 🟡 Falta ValueTracker |
| 3D | ⭐⭐⭐⭐ | ❌ | ❌ | 🔴 No hay |
| Code Highlight | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ❌ | 🔴 No hay |
| Graph Theory | ⭐⭐⭐⭐ | ❌ | ⭐ | 🔴 Básico |
| Interactividad | ⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐ | 🟡 Falta slides |
| Audio | ❌ | ⭐⭐⭐ | ❌ | 🟡 No hay |
| Performance | ⭐⭐ (CPU) | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ (GPU) | ✅ Superado |

---

## 🗺️ Roadmap Propuesto

### Sprint 1 — Portabilidad V2 y Quick Wins
- [x] Portar `RoundedRectangle` y `Ellipse` primitivos a V2 (`gaanim`).
- [x] Corregir orden de creación del Z-Index (evita superposición incorrecta de texto y rectángulos) en V2.
- [x] Portar todas las rate functions avanzadas e interpolaciones de Manim a V2.
- [x] Exponer animaciones auxiliares (`grow_from_center`, `shrink_to_center`, `uncreate`, `indicate`) en la API de Python de V2.

### SPRINT 2: Mobjects Core (Completado en V1, Pendiente portar a V2)
- [ ] Portar primitivas: `DashedLine`, `Sector`, `Annulus`.
- [ ] Portar objetos matemáticos y flechas: `DoubleArrow`, `CurvedArrow`, `ArcBetweenPoints`.
- [ ] Portar envolventes y referencias: `SurroundingRectangle`, `BackgroundRectangle`, `Cross`, `RightAngle`, `LabeledDot`, `LabeledArrow`.
- [ ] Exponerlos en el crate `gaanim_python` y documentar.

### Sprint 3: Animaciones de Alto Impacto (Completado en V1, Pendiente portar a V2)
- [ ] Portar `MoveAlongPath` (seguimiento de curvas Bézier).
- [ ] Portar implementaciones de animaciones:
  - [ ] `GrowFromPoint`, `GrowFromEdge`
  - [ ] `GrowArrow`
  - [ ] `SpinInFromNothing`
  - [ ] `DrawBorderThenFill`
  - [ ] `Wiggle`
  - [ ] `Flash`
  - [ ] `Circumscribe`
  - [ ] `ShowPassingFlash`
  - [ ] `FadeTransform`
  - [ ] Interfaz fluida `.animate` para animaciones encadenadas en la API de Python.

### Sprint 4: Contenido Educativo y Reactividad (Completado en V1, Pendiente portar a V2)
- [ ] Portar `NumberPlane` (grilla cartesiana interactiva).
- [ ] Portar `Table` y `Matrix` (visualización estructurada).
- [ ] Portar `BarChart` (gráficos de barras dinámicos).
- [ ] Portar `ValueTracker` + `always_redraw()` + `DecimalNumber` (reactividad de alto nivel).
- [ ] Portar animación `ChangeDecimalToValue`.

### Sprint 5 — Code & Presentación (1-2 semanas)
- [ ] `Code` mobject con syntax highlighting (syntect/tree-sitter)
- [ ] Modo presentación (`slide()` breakpoints + navegación)
- [ ] `TransformMatchingShapes` / `TransformMatchingTex`

### Sprint 6 — Avanzado (2-4 semanas)
- [ ] Graph theory layouts (spring, circular, tree, etc.)
- [ ] Boolean operations (`union`, `intersection`, `difference`)
- [ ] Audio integration
- [ ] `Homotopy`, `ApplyWave`

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
4. **Cámara superior** — Shake, Pulse, Follow, FrameTo con margin — más expresiva que Manim
5. **TracedPath avanzado** — Fading, smoothing, dissipation, dash patterns
6. **Sistema de temas** — Color + text themes con roles semánticos
7. **Viewport pinning** — HUD elements que no se mueven con la cámara
8. **Interactividad nativa** — Click, hover, drag sin necesitar plugin
9. **Snapshots + seek** — Timeline bidireccional para el editor
10. **Typst math** — Rendering de fórmulas sin instalar LaTeX
