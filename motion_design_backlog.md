# Backlog de motion design para Gaanim

> Investigación del **2026-09-24**. Es una lista de **propuestas**. Nada de lo que
> describe está implementado salvo que la columna *Hoy en Gaanim* lo diga. Antes de
> implementar un ítem, verifica el estado real contra el código y las pruebas, igual que
> con `engine_improvements.md`.

Este documento reúne APIs nuevas que harían los videos de Gaanim más expresivos y
atractivos. Sale de comparar la superficie actual de Gaanim con:

- librerías de animación programática: GSAP, anime.js v4, Motion (ex Framer Motion),
  Motion Canvas/Revideo, Remotion, Manim CE/ManimGL, Theatre.js, Lottie y Rive;
- herramientas de motion design: After Effects, Cavalry, Jitter, Figma Smart Animate
  y Keynote Magic Move;
- técnicas actuales: tipografía cinética, acabados de shader, transiciones y
  anotaciones.

Cada ítem tiene un ID estable para convertirlo en issue o PR. La tabla de resumen
propone un orden de integración por **olas**. Las fichas de cada área detallan la API
propuesta, cómo encaja en la arquitectura y cuándo se considera terminado.

## Cómo usar este backlog

1. Toma el siguiente ítem pendiente de la ola más baja. Sus dependencias deben estar
   cerradas.
2. Impleméntalo como una rebanada vertical completa con la skill
   `plugins/gaanim-dev/skills/gaanim-add-feature`: crate dueño, `gaanim_api`, PyO3,
   docstring en `gaanim_core.pyi`, export en `__init__.py`, página Typst según
   `plugins/gaanim-dev/references/api-doc-map.md` y un ejemplo ejecutable.
3. Los ítems visuales llevan un ejemplo con `GAANIM_SNAPSHOTS` y un baseline en
   `tests/visual/`. Solo se hace `--bless` después de aprobar el cambio visual.
4. Al cerrar un ítem, marca la casilla, enlaza el PR y actualiza *Hoy en Gaanim* en
   su ficha.

**Leyenda**

| Símbolo | Significado |
|---|---|
| `S` | Pequeño: un PR acotado, casi siempre sobre infraestructura existente |
| `M` | Mediano: toca varias capas o necesita un algoritmo nuevo |
| `L` | Grande: requiere infraestructura de render nueva o un subsistema completo |
| ★★★ | Impacto alto: cambia notablemente cómo se ven los videos o desbloquea muchos otros ítems |
| ★★ | Impacto medio |
| ★ | Pulido o nicho |

## Reglas transversales

Estas reglas aplican a todos los ítems. Son la diferencia entre copiar una API web y
hacerla encajar en un motor **determinista y seekable**.

1. **Determinismo.** Toda aleatoriedad debe ser una función pura de
   `(seed, índice, tiempo)`. Nada acumula estado entre frames. Un `seek(t)` tiene que
   producir exactamente el mismo frame que la reproducción continua, y eso incluye
   preview, snapshots, `gaanim_diff` y export.
2. **Forma cerrada antes que simulación.** Springs, balística, partículas, rebotes y
   ruido se evalúan analíticamente en `t`. Si algo exige integrar pasos, se hornea al
   construir la escena con paso fijo. El precedente es `add_updater_fn(fixed_dt=...)`.
3. **Vector primero.** Si un efecto puede producir geometría (trim, máscaras,
   metaballs, anotaciones, repeaters), se implementa como `BezPath` o `clip`. Así
   funciona también en export SVG, en HUD 3D y a cualquier resolución. Los shaders se
   reservan para looks que solo existen en raster: grain, bloom, blur y distorsión.
   Recuerda que el post-proceso hoy se omite en SVG y en 3D con perspectiva.
4. **Evaluación nativa.** Campos, ruido, osciladores y distribuciones se evalúan en
   Rust. No se invoca Python por objeto y por frame. Python solo describe specs
   diferidos.
5. **Vocabulario de Gaanim.** Setters fluidos inmediatos (`obj.trim(...)`), el mismo
   vocabulario bajo `.animate` y composición con `parallel`/`sequence`/`stagger`. Se
   usan `peniko`/`kurbo`/`glam` sin wrappers y los sistemas nuevos van en su fase de
   `SceneSet`.
6. **Presets con nombre.** Los valores por defecto deben verse bien sin ajustar nada.
   Motion Canvas, Figma y Jitter demuestran que los presets con nombre (`BOUNCY`,
   `GENTLE`) son lo que más eleva la calidad media.

## Resumen y orden de integración

La columna *Depende* lista los ítems que conviene cerrar antes.

### Ola 0: quick wins (el código ya existe en Rust o a medias)

| ☐ | ID | Ítem | Coste | Impacto | Depende |
|---|---|---|---|---|---|
| ☐ | [QW-01](#qw-01--honrar-writeby-order-stagger) | Honrar `write(by=, order=, stagger=)` | S | ★★★ | — |
| ☐ | [QW-02](#qw-02--exponer-grow_from_point-y-grow_from_edge) | Exponer `grow_from_point` / `grow_from_edge` | S | ★ | — |
| ☐ | [QW-03](#qw-03--exponer-reveal-brace-y-annotate-en-selecciones-de-texto) | Exponer `reveal`/`brace`/`annotate` en selecciones de texto | S | ★★ | — |
| ☐ | [QW-04](#qw-04--exponer-transitionmorph) | Exponer `Transition.morph` | S | ★★ | — |
| ☐ | [QW-05](#qw-05--contrato-de-animpulsewavehighlightfocuscancel) | Contrato de `Anim.pulse/wave/highlight/focus/cancel` | S | ★ | — |
| ☐ | [QW-06](#qw-06--easingcustomcallable-muestreado) | `Easing.custom(callable)` muestreado | S | ★★ | — |

### Ola 1: cimientos de ritmo y movimiento

| ☐ | ID | Ítem | Coste | Impacto | Depende |
|---|---|---|---|---|---|
| ☐ | [TM-01](#tm-01--springs-perceptuales-y-presets) | Springs perceptuales (`bounce`) y presets | S | ★★★ | — |
| ☐ | [TM-02](#tm-02--easings-paramétricos-y-expresivos) | Easings paramétricos y expresivos | S | ★★ | QW-06 |
| ☐ | [TM-03](#tm-03--repeat-yoyo-y-loop) | `repeat` / `yoyo` / `loop` para `Anim` y `Composition` | S | ★★★ | — |
| ☐ | [PR-01](#pr-01--stagger-con-origen-rejilla-y-distribución) | Stagger con origen, rejilla y distribución | M | ★★★ | — |
| ☐ | [PR-02](#pr-02--aleatoriedad-con-semilla-y-ruido-coherente) | Aleatoriedad con semilla y ruido coherente | S | ★★★ | — |
| ☐ | [PR-03](#pr-03--updaters-procedurales-wiggle-y-osciladores) | `Updater.wiggle` y `Updater.oscillate` | S | ★★ | PR-02 |
| ☐ | [FX-01](#fx-01--efectos-animables-glow-blur-shadow) | `glow`/`blur`/`shadow` animables | S | ★★★ | — |
| ☐ | [TR-01](#tr-01--trim-paths-animable) | Trim paths animable (`start`/`end`/`offset`) | S | ★★★ | — |
| ☐ | [TR-03](#tr-03--movimiento-orientado-sobre-trayectoria-y-en-arco) | `move_along` orientado y `path_arc` | S | ★★ | — |

### Ola 2: tipografía cinética, transiciones y cámara

| ☐ | ID | Ítem | Coste | Impacto | Depende |
|---|---|---|---|---|---|
| ☐ | [TX-02](#tx-02--animador-de-rango-de-texto) | Animador de rango de texto (motor por glifo) | M | ★★★ | QW-01 |
| ☐ | [TX-01](#tx-01--revelados-con-máscara-por-línea-palabra-o-carácter) | Revelados con máscara por línea, palabra o carácter | M | ★★★ | TX-02 |
| ☐ | [TX-03](#tx-03--máquina-de-escribir-con-cursor) | Máquina de escribir con cursor | S | ★★ | — |
| ☐ | [TX-04](#tx-04--scramble--decode) | Scramble / decode | M | ★★ | PR-02 |
| ☐ | [TX-05](#tx-05--blur-in-y-tracking) | Blur-in y tracking | S | ★★ | FX-01, TX-02 |
| ☐ | [TS-01](#ts-01--wipes-iris-push-y-blinds-con-easing) | Wipes, iris, push y blinds, con easing | M | ★★★ | — |
| ☐ | [TS-04](#ts-04--overlays-sobre-el-corte) | Overlays sobre el corte (flash, light leak) | S | ★ | — |
| ☐ | [TM-05](#tm-05--etiquetas-posiciones-relativas-y-marcadores) | Etiquetas, posiciones relativas y marcadores | M | ★★★ | — |
| ☐ | [CA-01](#ca-01--shake-por-trauma-con-ruido) | Shake por trauma con ruido | S | ★★ | PR-02 |
| ☐ | [CA-02](#ca-02--zoom-perceptualmente-uniforme) | Zoom perceptualmente uniforme | S | ★★ | — |
| ☐ | [AN-02](#an-02--resaltador-tipo-marcador) | Resaltador tipo marcador | S | ★★ | — |

### Ola 3: acabado visual (shaders y composición)

| ☐ | ID | Ítem | Coste | Impacto | Depende |
|---|---|---|---|---|---|
| ☐ | [FX-02](#fx-02--uniforms-enlazables-y-cadena-de-post-procesos) | Uniforms enlazables y cadena de post-procesos | M | ★★★ | — |
| ☐ | [FX-03](#fx-03--presets-de-acabado) | Presets de acabado: grain, viñeta, aberración, grading, LUT | S | ★★★ | FX-02 |
| ☐ | [FX-09](#fx-09--fondos-vivos) | Fondos vivos (mesh/noise gradient, aurora, rejilla) | S | ★★ | — |
| ☐ | [FX-07](#fx-07--modos-de-fusión-por-objeto) | Modos de fusión por objeto | S | ★★ | — |
| ☐ | [FX-04](#fx-04--bloom-multipaso) | Bloom multipaso | M | ★★★ | FX-02 |
| ☐ | [FX-05](#fx-05--motion-blur-por-sub-frames) | Motion blur por sub-frames | M | ★★★ | — |
| ☐ | [FX-06](#fx-06--echo-y-estelas) | Echo y estelas | M | ★★ | — |
| ☐ | [TS-02](#ts-02--transiciones-por-shader) | Transiciones por shader (estilo gl-transitions) | L | ★★ | FX-02 |

### Ola 4: sistemas procedurales, formas y tiempo avanzado

| ☐ | ID | Ítem | Coste | Impacto | Depende |
|---|---|---|---|---|---|
| ☐ | [PR-04](#pr-04--duplicador-y-repeater-con-distribuciones) | Duplicador/Repeater con distribuciones | M | ★★★ | — |
| ☐ | [PR-05](#pr-05--campos-y-falloffs) | Campos y falloffs estilo Cavalry | M | ★★★ | PR-02, PR-04 |
| ☐ | [PR-06](#pr-06--conexiones-tipo-plexus) | Conexiones tipo plexus | S | ★ | PR-04 |
| ☐ | [PR-07](#pr-07--emisor-de-partículas-determinista) | Emisor de partículas determinista | M | ★★★ | PR-02 |
| ☐ | [PR-08](#pr-08--física-analítica-ligera) | Física analítica ligera (`throw`, `inertia`) | M | ★★ | — |
| ☐ | [TR-02](#tr-02--dash-offset-animado) | Dash offset animado | S | ★★ | — |
| ☐ | [TR-04](#tr-04--modificadores-de-path) | Modificadores de path no destructivos | M | ★★ | — |
| ☐ | [TR-05](#tr-05--trazo-con-grosor-variable) | Trazo con grosor variable (taper) | M | ★★ | — |
| ☐ | [TM-04](#tm-04--keyframes-multicanal) | Keyframes multicanal | M | ★★ | — |
| ☐ | [TM-06](#tm-06--rampas-de-velocidad-y-time-remap) | Rampas de velocidad y time remap | M | ★★ | — |
| ☐ | [TM-07](#tm-07--follow-through-settle-y-cadenas-con-retardo) | Follow-through: `settle` y `follow(delay=)` | M | ★★ | — |
| ☐ | [TM-08](#tm-08--squash-and-stretch-por-velocidad) | Squash & stretch por velocidad | S | ★ | — |

### Ola 5: narrativa, audio y explicación

| ☐ | ID | Ítem | Coste | Impacto | Depende |
|---|---|---|---|---|---|
| ☐ | [AU-01](#au-01--marcadores-para-voz-en-off) | Marcadores para voz en off (`wait_until` + sidecar) | M | ★★★ | TM-05 |
| ☐ | [AU-02](#au-02--análisis-de-audio-como-señales-reactivas) | Análisis de audio como señales reactivas | M | ★★★ | — |
| ☐ | [AU-03](#au-03--waveform-y-espectro-como-drawables) | Waveform y espectro como drawables | S | ★★ | AU-02 |
| ☐ | [AU-04](#au-04--efectos-de-sonido-anclados) | Efectos de sonido anclados | S | ★★ | — |
| ☐ | [TX-07](#tx-07--subtítulos-karaoke) | Subtítulos karaoke (SRT/VTT/Whisper) | M | ★★★ | TX-02 |
| ☐ | [TX-09](#tx-09--animación-de-código-por-diff) | Animación de código por diff | M | ★★★ | — |
| ☐ | [TX-06](#tx-06--texto-sobre-trayectoria) | Texto sobre trayectoria | M | ★★ | TR-03 |
| ☐ | [TS-03](#ts-03--magic-move-por-claves) | Magic move por claves | M | ★★★ | QW-04 |
| ☐ | [AN-01](#an-01--anotaciones-a-mano-alzada) | Anotaciones a mano alzada | M | ★★★ | TR-01, PR-02 |
| ☐ | [AN-03](#an-03--puntas-de-flecha-en-cualquier-trazo) | Puntas de flecha en cualquier trazo | S | ★ | TR-01 |
| ☐ | [AN-04](#an-04--énfasis-adicionales) | Énfasis adicionales (broadcast, spotlight…) | S | ★★ | — |
| ☐ | [AN-05](#an-05--carrera-de-barras) | Carrera de barras (bar chart race) | M | ★★ | — |
| ☐ | [AN-07](#an-07--anillos-de-progreso-y-temporizadores) | Anillos de progreso y temporizadores | S | ★ | TR-01 |

### Ola 6: apuestas grandes

| ☐ | ID | Ítem | Coste | Impacto | Depende |
|---|---|---|---|---|---|
| ☐ | [FX-10](#fx-10--capas-offscreen-por-objeto) | Capas offscreen por objeto (infraestructura) | L | ★★★ | FX-02 |
| ☐ | [FX-08](#fx-08--mates-alpha-y-luma) | Mates alpha y luma | M | ★★ | FX-10 |
| ☐ | [FX-11](#fx-11--metaballs--gooey) | Metaballs / gooey | M | ★★ | — |
| ☐ | [FX-12](#fx-12--vidrio-y-backdrop-blur) | Vidrio y backdrop blur | L | ★★ | FX-10 |
| ☐ | [CA-03](#ca-03--cámara-sobre-trayectoria-y-whip-pan) | Cámara sobre trayectoria y whip pan | M | ★★ | TR-03, FX-05 |
| ☐ | [CA-04](#ca-04--capas-de-parallax-25d) | Capas de parallax 2.5D | M | ★★ | — |
| ☐ | [CA-05](#ca-05--profundidad-de-campo) | Profundidad de campo | L | ★ | CA-04, FX-10 |
| ☐ | [CA-06](#ca-06--extrusión-3d-de-formas-y-texto) | Extrusión 3D de formas y texto | L | ★★ | — |
| ☐ | [TX-08](#tx-08--fuentes-variables-animadas) | Fuentes variables animadas | L | ★ | TX-02 |
| ☐ | [AN-06](#an-06--rutas-y-mapas) | Rutas y mapas | L | ★ | TR-01, CA-03 |
| ☐ | [AS-01](#as-01--lottie-segmentos-slots-y-reverse) | Lottie: segmentos por marker, slots y reverse | M | ★ | — |

---

## QW · Quick wins

### QW-01 · Honrar `write(by=, order=, stagger=)`

`S` · ★★★

- **Qué:** El binding valida `by="grapheme"|"word"|"line"|"part"` y
  `order="forward"|"reverse"|"center"|"random"`, pero solo `by="part"` y `stagger`
  (convertido a `lag_ratio`) cambian el resultado. Los demás valores comparten el
  schedule vectorial (`crates/gaanim_python/src/pydrawable.rs:544`,
  `docs/content/api/text.typ:525`). Honrarlos da escritura por palabra o línea y
  órdenes desde el centro o aleatorios.
- **Referencias:** Manim `AddTextLetterByLetter`/`LaggedStart`, Jitter "animate by
  line/word/letter".
- **API:** No cambia. Solo pasa a tener efecto.

  ```python
  title.animate.write(by="word", order="center", stagger=0.08).duration(1.2)
  ```

- **Implementación:** El schedule de write en `gaanim_api/src/builder.rs` debe agrupar
  por `graphemes`/`words`/`lines`, que ya existen como selecciones. `random` debe ser
  determinista con una semilla estable, por ejemplo derivada del ID del objeto.
- **Hecho cuando:** Cada combinación `by × order` tiene un baseline y el texto de
  `text.typ` deja de decir que los valores se ignoran.

### QW-02 · Exponer `grow_from_point` y `grow_from_edge`

`S` · ★

- **Hoy:** `AnimationType::GrowFromPoint`/`GrowFromEdge` existen en
  `crates/gaanim_api/src/anim.rs:389`, pero no tienen binding Python.
- **API:**

  ```python
  bar.animate.grow_from_edge(Direction.DOWN)
  badge.animate.grow_from_point(2, 1)
  ```

- **Hecho cuando:** Existen el binding, el stub, la sección en `animations.typ` y un
  ejemplo.

### QW-03 · Exponer `reveal`, `brace` y `annotate` en selecciones de texto

`S` · ★★

- **Hoy:** `TextSelectionEffect::{RevealFade, RevealWipe, RevealFromBelow, Brace,
  Annotate}` y el método Rust `reveal(FragmentRevealStyle)` existen
  (`crates/gaanim_api/src/canvas/drawable.rs:2140`). `TextSelectionAnimation` en
  Python no los ofrece.
- **API:**

  ```python
  eq["rhs"].animate.reveal("from_below")
  eq["mass"].animate.brace("masa")
  eq["c"].animate.annotate("velocidad de la luz", offset=(0, 0.6))
  ```

- **Hecho cuando:** Los efectos tienen stub, ejemplo y baseline de ecuación.

### QW-04 · Exponer `Transition.morph`

`S` · ★★

- **Hoy:** `TransitionType::Morph { mappings }` existe en
  `crates/gaanim_timeline/src/transition.rs:12`. Python solo ofrece `cut`,
  `cross_fade`, `fade_through`, `slide` y `zoom_through`.
- **API:**

  ```python
  scene.segment("detalle", transition=Transition.morph(0.8, pairs=[(card_a, card_b)]))
  ```

- **Hecho cuando:** Existe una transición entre segmentos que conserva objetos
  emparejados, con baseline. Es la base de TS-03.

### QW-05 · Contrato de `Anim.pulse/wave/highlight/focus/cancel`

`S` · ★

- **Hoy:** Estos métodos están ligados en `PyCanvasAnim`
  (`crates/gaanim_python/src/pydrawable.rs:749-790`), pero el stub solo los declara en
  `TextSelectionAnimation`.
- **Qué:** Hay que decidir si son API pública de cualquier `Drawable`. Si lo son, se
  documentan en el stub `Anim` y en `animations.typ`. Si no, se restringen a
  selecciones de texto con un error claro.
- **Hecho cuando:** La auditoría del plugin y `validate_python_api` cubren la decisión.

### QW-06 · `Easing.custom(callable)` muestreado

`S` · ★★

- **Hoy:** `RateFunc::Custom(Arc<Fn>)` existe solo en Rust.
- **API:**

  ```python
  Easing.custom(lambda t: 1 - (1 - t) ** 4, samples=256)
  ```

- **Implementación:** Se muestrea el callable **al construir la escena** en una LUT,
  con una variante nueva `RateFunc::Sampled(Arc<[f64]>)` e interpolación lineal. Así
  no hay GIL en el render, el resultado es serializable y es determinista. La misma
  variante sirve para TM-02 (`from_svg`, `rough`).
- **Hecho cuando:** Hay tests de monotonía y extremos. `NaN` o valores fuera de
  `[-1, 2]` producen `ValueError`.

---

## TM · Tiempo, easing y ritmo

### TM-01 · Springs perceptuales y presets

`S` · ★★★

- **Qué:** Hoy `Easing.spring(stiffness=90, damping=12)` obliga a razonar en
  parámetros físicos. Las librerías modernas exponen un spring *perceptual*: una
  duración visual más un `bounce` en `[0, 1)`. Además ofrecen presets con nombre.
- **Referencias:**
  - Motion: `spring(visualDuration, bounce)`.
  - anime.js v4: `spring({bounce, duration})`.
  - Remotion: `spring({damping: 200})` para un movimiento suave sin rebote.
  - Motion Canvas: `PlopSpring`, `SmoothSpring`, etc.
  - Figma: Gentle, Quick, Bouncy y Slow.
- **API:**

  ```python
  logo.animate.scale_to(1.0).duration(0.6).easing(Easing.spring(bounce=0.35))
  card.animate.move_to(0, 0).easing(Easing.BOUNCY)  # GENTLE, QUICK, BOUNCY, SNAPPY, SMOOTH_SPRING
  dot.animate.move_to(3, 0).easing(Easing.spring(stiffness=180, damping=14, mass=1.0, velocity=2.0))
  ```

- **Implementación:**
  - `RateFunc::Spring` gana `mass` y `initial_velocity`.
  - El constructor perceptual fija ζ = 1 − bounce y elige ω para que la envolvente
    caiga bajo ε al final de la duración normalizada.
  - Se mantienen las ramas cerradas sub, crítica y sobreamortiguada, así que el seek
    es exacto.
  - `spring(stiffness, damping)` conserva su contrato actual.
- **Hecho cuando:** Hay tests de continuidad en `t = 0` y `t = 1`, y el overshoot
  máximo coincide con `bounce` dentro de una tolerancia. Los presets aparecen en
  «Tiempo y easing» de `animations.typ`.

### TM-02 · Easings paramétricos y expresivos

`S` · ★★

- **Qué:** `EasingCurve.BACK`, `ELASTIC` y `BOUNCE` existen pero sin parámetros.
  Faltan además easings de carácter.
- **Referencias:**
  - GSAP: `back.out(1.7)`, `elastic.out(amp, period)`, `CustomEase`, `CustomWiggle`,
    `RoughEase`, `SlowMo` y `ExpoScaleEase`.
  - Manim: `squish_rate_func` y `wiggle`.
  - CSS: `steps(n, jump-*)`.
- **API:**

  ```python
  Easing.back(overshoot=1.7)
  Easing.elastic(amplitude=1.0, period=0.3)
  Easing.bounce(strength=0.6)
  Easing.slow_mo(linear_ratio=0.7, power=0.7)   # rápido → cámara lenta → rápido
  Easing.rough(strength=1.0, points=20, seed=4)  # parpadeo y jitter deterministas
  Easing.squish(Easing.SMOOTH, 0.2, 0.8)         # actúa solo en [0.2, 0.8]
  Easing.from_svg("M0,0 C0.3,0 0.2,1.2 1,1")     # curva dibujada en cualquier editor
  Easing.steps(6, jump="end")
  ```

- **Implementación:** Son variantes nuevas de `RateFunc` en
  `crates/gaanim_math/src/easing.rs`. `from_svg` y `rough` se muestrean a
  `RateFunc::Sampled` (QW-06).
- **Hecho cuando:** Cada easing tiene tests de valor en `0`, `1` y puntos de control,
  y aparece en una tabla visual de curvas en la documentación.

### TM-03 · `repeat`, `yoyo` y `loop`

`S` · ★★★

- **Qué:** Hoy solo video, Lottie, glTF y algunos `Updater` pueden repetirse. Los
  pulsos, latidos, spinners e indicadores de "idle" necesitan repetición en cualquier
  `Anim` o `Composition`.
- **Referencias:**
  - GSAP: `repeat`, `yoyo` y `repeatDelay`.
  - anime.js: `loop` y `alternate`.
  - After Effects: `loopOut("cycle"|"pingpong"|"offset")`.
- **API:**

  ```python
  spinner.animate.rotate_by(math.tau).duration(1.2).repeat(3)
  badge.animate.scale_to(1.08).duration(0.4).repeat(4, yoyo=True, delay=0.1)
  arrow.animate.shift_by(0.3, 0).duration(0.5).loop("pingpong", until="segment")
  parallel(a, b).repeat(2)
  ```

- **Implementación:**
  - Se envuelve el tiempo local del clip con `cycle`, `pingpong` u `offset`. En
    `offset` cada ciclo suma el delta del anterior.
  - Duración total = n·d + (n − 1)·delay.
  - `until="segment"` o `until=<segundos>` acota los bucles infinitos para que el
    timeline siga siendo finito y seekable.
- **Hecho cuando:** `Composition.schedule()` refleja la duración expandida y los
  snapshots en mitad de un ciclo coinciden con la reproducción.

### TM-04 · Keyframes multicanal

`M` · ★★

- **Qué:** Un solo clip con varias paradas y un easing por tramo. Así se describe un
  movimiento de varios tiempos sin encadenar `sequence`s del mismo objeto.
- **Referencias:**
  - GSAP: `keyframes` (formas de array y porcentaje, `easeEach`).
  - anime.js y Motion: keyframes.
  - Remotion: `interpolate(frame, [0, 20, 40], [0, 1.2, 1])`.
  - After Effects: keyframes espaciales.
- **API:** Los canales son los mismos que ya usa `Anim.custom(channels=...)`.

  ```python
  ball.animate.keyframes(
      times=[0.0, 0.35, 0.7, 1.0],
      position=[(-4, 0), (0, 2.5), (2.5, 0), (4, 0)],
      scale=[1.0, 1.0, (1.25, 0.8), 1.0],
      easing=[Easing.ease_out(EasingCurve.QUADRATIC), Easing.ease_in(EasingCurve.QUADRATIC), Easing.SMOOTH],
      spatial="catmull_rom",
  ).duration(1.6)
  progress.animate.keyframes(times=[0, 0.5, 1], values=[0, 80, 100])  # también en Parameter
  ```

- **Implementación:** Es una lente por tramos en `gaanim_animation`. Con `spatial`, la
  posición recorre una curva suave que pasa por los puntos, como el bezier espacial de
  After Effects.
- **Hecho cuando:** Hay validación de longitudes y tiempos crecientes, y un baseline
  con varios canales.

### TM-05 · Etiquetas, posiciones relativas y marcadores

`M` · ★★★

- **Qué:** `sequence(gap=-x)` ya permite solapar. Falta **nombrar instantes** y colocar
  animaciones relativas a ellos. Es lo que hace fluida una coreografía y permite
  retocar tiempos sin recalcular offsets.
- **Referencias:**
  - GSAP: el position parameter (`"<"`, `">"`, `"-=0.3"`, `"label+=0.2"`) y
    `addLabel`.
  - Motion: `animate([...], {at})`.
  - anime.js: `timeline.label`.
- **API:**

  ```python
  intro = sequence(
      title.animate.write().duration(0.8),
      label("golpe"),
      subtitle.animate.fade_in().duration(0.4),
  ).insert(logo.animate.grow_from_center(), at="golpe+0.15") \
   .insert(glow_pulse, at="<")          # arranca junto con el hijo anterior
  scene.play(intro)
  scene.marker("clímax")                # instante con nombre en el timeline global
  ```

- **Implementación:** Las posiciones se resuelven a tiempos absolutos en
  `Composition.schedule()`. Los marcadores de escena se exponen en `scene.stops`/el
  editor y se aceptan en `gaanim export --from clímax --to fin`, que ya soporta rangos
  de tiempo.
- **Hecho cuando:** El `Schedule` lista las etiquetas resueltas, hay errores claros
  para etiquetas inexistentes y el editor permite saltar a un marcador.

### TM-06 · Rampas de velocidad y time remap

`M` · ★★

- **Qué:** Cámara lenta dentro de una animación, o una aceleración global de una
  composición. Es la base de los *speed ramps* de los cortes modernos.
- **Referencias:** Manim `ChangeSpeed(speedinfo={...})`, anime.js `playbackEase`,
  GSAP `timeScale`/`SlowMo`, time remap de After Effects.
- **API:**

  ```python
  scene.play(sequence(...).speed_ramp({0.0: 1.0, 0.4: 0.15, 0.6: 0.15, 1.0: 1.0}))
  scene.play(parallel(...).time_remap(Easing.ease_in_out(EasingCurve.CUBIC)))
  ```

- **Implementación:** Se integra la curva de velocidad al construir la composición y
  se mapea el tiempo local de cada hijo. El resultado sigue siendo una función pura de
  `t`.
- **Hecho cuando:** La duración resultante es correcta y los hijos conservan su orden
  relativo.

### TM-07 · Follow-through: `settle` y cadenas con retardo

`M` · ★★

- **Qué:** El *follow-through* y el *overlapping action* son dos de los 12 principios
  de animación. Esto añade un rebote inercial al terminar un movimiento, y seguidores
  que copian a un líder con retardo (estelas y colas).
- **Referencias:** Expresión *inertial bounce* de After Effects, `valueAtTime(time -
  d)`, Cavalry Spring, Motion (retargeting con velocidad).
- **API:**

  ```python
  card.animate.move_to(0, 0).duration(0.5).settle(overshoot=0.12, frequency=3.0, decay=6.0)
  for i, dot in enumerate(dots):
      dot.follow(leader, delay=0.06 * (i + 1))
  ```

- **Implementación:**
  - `settle` suma, tras el último valor, v·amp·sin(2π·f·t)/e^(decay·t), donde v es la
    velocidad final evaluada. El clip se extiende hasta que la oscilación cae bajo ε.
  - `follow(delay=)` evalúa la transformación del líder en `t − delay`. Hoy
    `Drawable.follow` no acepta retardo y la cámara solo tiene `lag`.
- **Hecho cuando:** Un seek a cualquier `t` reproduce la estela sin estado acumulado.

### TM-08 · Squash and stretch por velocidad

`S` · ★

- **Qué:** Es otro de los 12 principios: estirar en la dirección del movimiento y
  aplastar en la perpendicular, conservando el área.
- **Referencias:** GSAP `CustomBounce` (con su ease de squash emparejado) y la
  técnica clásica de 2D.
- **API:**

  ```python
  ball.squash_stretch(amount=0.25, max_ratio=1.6)
  ```

- **Implementación:** Es un modificador reactivo. La velocidad sale de una diferencia
  finita de la posición evaluada en `t ± h`, que es una función pura. Luego se aplica
  sx = 1 + k·|v|, sy = 1/sx, alineado con la velocidad.
- **Hecho cuando:** El objeto parado no se deforma y el área se conserva.

---

## PR · Coreografía, procedural y generativo

### PR-01 · Stagger con origen, rejilla y distribución

`M` · ★★★

- **Qué:** Hoy el retardo de `stagger(*items, each=0.1)` es índice × `each`. Las
  "ondas" desde el centro, los bordes o un punto convierten una rejilla de objetos en
  coreografía.
- **Referencias:**
  - GSAP: el objeto `stagger` (`from`, `grid`, `axis`, `ease`, `amount`) y
    `utils.distribute`.
  - anime.js: `stagger(v, {grid, from, reversed, modifier})`.
  - Motion: `stagger(0.1, {from})`.
- **API:**

  ```python
  scene.play(stagger(*[d.animate.grow_from_center() for d in dots],
                     each=0.03, origin="center", grid="auto",
                     easing=Easing.ease_in(EasingCurve.QUADRATIC)))
  scene.play(stagger(*anims, total=1.2, origin="random", seed=7))
  scene.play(stagger(*anims, each=0.05, origin=(0.0, -3.0)))  # desde un punto de la escena
  distribute(dots, "scale", 0.4, 1.4, origin="edges")          # reparte valores, no tiempos
  ```

- **Implementación:**
  - delay_i = ease(dist_i / dist_max) · span.
  - La distancia se mide desde el centro de los bounds de cada destino, que Rust ya
    conoce tras el layout. Con `grid=`, se usa el índice de rejilla.
  - `grid="auto"` detecta filas y columnas agrupando posiciones.
  - `random` usa un shuffle con semilla.
  - `each=` conserva su significado actual.
- **Hecho cuando:** Hay baselines para `center`, `edges`, `random` y un punto, sobre
  una rejilla de 12×7.

### PR-02 · Aleatoriedad con semilla y ruido coherente

`S` · ★★★

- **Qué:** No hay ruido Perlin ni simplex en ningún crate, y tampoco una API de
  aleatoriedad reproducible. Este ítem es prerrequisito de wiggle, shake, scramble,
  partículas y anotaciones a mano alzada.
- **Referencias:** Motion Canvas `useRandom(seed)`, anime.js
  `utils.createSeededRandom`, Remotion `@remotion/noise` (`noise2D/3D/4D(seed, ...)`)
  y el `wiggle()` de After Effects.
- **API:**

  ```python
  rng = scene.random(seed=42)  # uniform, gauss, choice, shuffle, integers
  positions = [(rng.uniform(-6, 6), rng.uniform(-3, 3)) for _ in range(40)]

  drift = scene.noise(frequency=0.6, amplitude=0.3, octaves=3, seed=5)  # ScalarSource nativo
  scene.camera.bind_2d(rotation=computed(lambda v: 0.02 * v, inputs=[drift]))
  ```

- **Implementación:** Se añaden `gaanim_math::noise` (simplex con fBm y seed) y un PRNG
  estable, como PCG o SplitMix. La misma implementación se expone a Python para que la
  autoría y el runtime coincidan. `scene.noise(...)` es un nodo reactivo que se evalúa
  en Rust sin callback Python por frame.
- **Hecho cuando:** Hay tests de reproducibilidad con semilla fija, de rango, y de que
  la reproducción continua coincide con un seek.

### PR-03 · Updaters procedurales: wiggle y osciladores

`S` · ★★

- **Qué:** Dar "vida" a objetos quietos: un jitter orgánico y oscilaciones periódicas
  sumadas sobre sus animaciones. El `wiggle()` actual es otra cosa: un énfasis tipo
  Manim.
- **Referencias:** After Effects `wiggle(freq, amp, octaves)`, Cavalry Oscillator y
  Noise behaviours, GSAP `CustomWiggle`.
- **API:**

  ```python
  logo.add_updater(Updater.wiggle(position=0.08, rotation=0.03, frequency=2.0, octaves=2, seed=1))
  light.add_updater(Updater.oscillate("opacity", waveform="triangle", frequency=0.5, low=0.4, high=1.0))
  ```

- **Implementación:** Son presets nuevos junto a `Updater.orbit/bob/rotate/pulse`
  (`crates/gaanim_api/src/canvas/ops.rs`). Son funciones puras del tiempo que se aplican
  como capa aditiva. Las formas de onda son `sine`, `square`, `triangle` y `saw`.
- **Hecho cuando:** Un seek es exacto y el efecto se combina bien con
  `animate.move_to`.

### PR-04 · Duplicador y Repeater con distribuciones

`M` · ★★★

- **Qué:** Crear N copias de una forma con una transformación acumulada o una
  distribución (rejilla, círculo, trayectoria, aleatoria, filotaxis), con hijos
  indexados. Con eso se hacen explosiones radiales, patrones, túneles y fondos.
- **Referencias:** Repeater de After Effects y Lottie, Cavalry Duplicator con
  Distributions, Manim `Broadcast`.
- **API:**

  ```python
  petals = scene.geometry.repeat(petal, count=12, rotate=math.tau / 12, scale=0.96, opacity=(1.0, 0.25))
  grid = scene.geometry.duplicate(dot, Distribution.grid(12, 7, spacing=0.5))
  ring = scene.geometry.duplicate(icon, Distribution.circle(16, radius=2.5, orient=True))
  trail = scene.geometry.duplicate(chevron, Distribution.along(path, 20, orient=True))
  cloud = scene.geometry.duplicate(star, Distribution.random(60, bounds=(-6, -3, 6, 3), seed=3))
  scene.play([petals.animate.count(24).duration(1.0)])
  ```

- **Implementación:** El resultado es un `Group` con hijos indexables. Las instancias
  comparten geometría, así que el cache de fragmentos retenidos del renderer las hace
  baratas. `Distribution.points` reutiliza `scene.geometry.points`. Un `count`
  fraccionario anima la opacidad de la última copia.
- **Hecho cuando:** Hay un baseline por distribución y un benchmark con 1 000
  instancias.

### PR-05 · Campos y falloffs

`M` · ★★★

- **Qué:** Una "brocha de influencia": un valor por instancia calculado a partir de su
  índice, su distancia a un objetivo o ruido, y conectado a cualquier canal. Un solo
  keyframe mueve cientos de elementos. Es lo que hace potente a Cavalry.
- **Referencias:** Cavalry Behaviours (Stagger, Noise, Apply Distance, Look At) y
  Falloffs (radial, lineal, ruido, con modos de combinación). GSAP `distribute`.
- **API:**

  ```python
  cursor = scene.geometry.dot().move_to(-6, 0)
  near = Field.distance(cursor, radius=2.0, falloff="smooth")
  grid.drive("scale", near.remap(1.0, 1.8))
  grid.drive("fill", near.gradient(BLUE, GOLD))
  grid.drive("rotation", Field.noise(frequency=0.4, seed=2).remap(-0.3, 0.3))
  grid.drive("opacity", Field.index(easing=Easing.SMOOTH).remap(0.2, 1.0))
  grid.look_at(cursor)
  scene.play([cursor.animate.move_to(6, 0).duration(3)])
  ```

- **Implementación:** Es un nodo reactivo nativo por instancia. Los campos se combinan
  (`a * b`, `max(a, b)`, `a + b`) como los modos de falloff de Cavalry. Hoy se podría
  hacer con N `computed` en Python, pero eso invoca Python por hijo y por frame.
- **Hecho cuando:** Un grid de 20×12 barrido por un cursor corre en tiempo real en el
  preview, y existe un baseline.

### PR-06 · Conexiones tipo plexus

`S` · ★

- **API:**

  ```python
  links = scene.geometry.connect(cloud, max_distance=1.4, mode="range", fade_by_distance=True)
  ```

- **Referencias:** Cavalry Connect Shape (range, nearest, secuencial).
- **Implementación:** Es geometría viva. Se recalcula cuando se mueven los puntos, con
  un orden determinista de aristas.

### PR-07 · Emisor de partículas determinista

`M` · ★★★

- **Qué:** Chispas, confeti, polvo, humo y estallidos. Hoy solo existen
  `VectorField.particles` (advección) y `FlowParticles`.
- **Referencias:** CC Particle World de After Effects, GSAP Physics2D y Remotion.
- **API:**

  ```python
  sparks = scene.fx.particles(
      emitter=Emitter.circle(radius=0.2).at(logo),
      rate=60, lifetime=(0.6, 1.2), speed=(2.0, 4.0), spread=math.tau,
      gravity=(0, -3), drag=0.8, size=(0.03, 0.08), shape="circle",
      color=Brush.linear(GOLD, RED), seed=9,
  )
  scene.play([sparks.animate.burst(80)])
  confetti = scene.fx.confetti(origin=(0, -4), count=120, seed=2)
  ```

- **Implementación:**
  - Cada partícula tiene forma cerrada: birth_i = i/rate y age = t − birth_i.
  - Con arrastre lineal, la posición es p = p0 + v·(1 − e^(−k·age))/k más el término
    de gravedad.
  - Los atributos salen de hash(seed, i).
  - Un seek cuesta O(partículas vivas).
  - `scene.fx` sería un namespace nuevo para partículas y presets de efectos.
- **Hecho cuando:** El seek es exacto, existe un preset `confetti` y hay benchmark con
  2 000 partículas.

### PR-08 · Física analítica ligera

`M` · ★★

- **API:**

  ```python
  ball.animate.throw(velocity=(3, 6), gravity=9.8, floor=-3, restitution=0.55)
  coin.animate.inertia(velocity=4.0, friction=3.0, snap=[-2, 0, 2])
  ```

- **Referencias:** GSAP Physics2D e Inertia (desaceleración hacia puntos de snap).
- **Implementación:** `throw` es balística por tramos analíticos con un número finito
  de rebotes hasta el reposo. `inertia` es una desaceleración exponencial que termina
  en el snap más cercano. La física de cuerpos rígidos con colisiones queda fuera: se
  hornea con `add_updater_fn(fixed_dt=...)`.

---

## TR · Trazos, trayectorias y formas

### TR-01 · Trim paths animable

`S` · ★★★

- **Qué:** Es la columna vertebral de los reveals de line art, iconos y logos: dibujar
  un trazo desde el centro, un segmento que viaja por la ruta, o dibujar y borrar a la
  vez.
- **Referencias:** After Effects y Lottie Trim Paths (start/end/offset,
  simultaneous/individual), GSAP DrawSVG (`"20% 80%"`), Remotion `evolvePath` y anime.js
  `createDrawable`.
- **Hoy:** `create`/`write` usan una lente `PathCompletion`, y ya existe
  `get_subpath_range` (`crates/gaanim_math/src/path.rs:297`). No hay propiedad pública.
- **API:**

  ```python
  logo.trim(end=0.0)
  scene.play([logo.animate.trim(end=1.0).duration(1.2)])
  ring.trim(start=0.5, end=0.5)
  scene.play([ring.animate.trim(start=0.0, end=1.0)])                     # desde el centro
  scene.play([orbit.animate.trim(start=0.0, end=0.15, offset=1.0).duration(2)])  # segmento viajero
  icon.trim(end=0.0, mode="sequential")  # subpaths uno tras otro; "simultaneous" en paralelo
  ```

- **Implementación:** Una propiedad `Trim { start, end, offset, mode }` y su lente. El
  `offset` da la vuelta en paths cerrados.
- **Hecho cuando:** Hay baselines de dibujar desde el centro, del segmento viajero y
  del modo `sequential`.

### TR-02 · Dash offset animado

`S` · ★★

- **Qué:** Hormigas en marcha, flujo por tuberías y bordes activos. `StrokeStyle(dashes,
  dash_offset)` existe, pero es estático.
- **API:**

  ```python
  pipe.stroke_style(StrokeStyle(dashes=[0.2, 0.15]))
  pipe.add_updater(Updater.dash_flow(speed=0.8))
  scene.play([border.animate.dash_offset(2.0).duration(2)])
  ```

- **Implementación:** Una lente de `dash_offset` en `crates/gaanim_animation/src/tween.rs`.

### TR-03 · Movimiento orientado sobre trayectoria y en arco

`S` · ★★

- **Qué:** Hoy `move_along` solo traslada y no gira con la tangente
  (`crates/gaanim_api/src/anim.rs:411`). Los movimientos en línea recta se ven
  mecánicos. Los arcos son un principio básico de la animación.
- **Referencias:**
  - GSAP MotionPath: `autoRotate`, `alignOrigin`, `start`/`end`.
  - anime.js `createMotionPath`.
  - Manim `path_arc`.
- **API:**

  ```python
  plane.animate.move_along(route, orient=True, rotate_offset=0.0, start=0.0, end=1.0)
  ball.animate.move_to(4, 0).path_arc(math.pi / 3)
  a.animate.transform_to(b).path_arc(-math.pi / 2)
  ```

- **Implementación:**
  - La rotación sale de la tangente muestreada. Ya existen los helpers
    `tangent_on_curve`/`point_on_curve`.
  - `path_arc` interpola la posición por un arco circular.
- **Hecho cuando:** Existe un baseline de un avión que recorre una curva orientado.

### TR-04 · Modificadores de path

`M` · ★★

- **Qué:** Una pila no destructiva de operadores con parámetros animables. Convierte
  formas simples en orgánicas, eléctricas o decorativas.
- **Referencias:** Operadores de shape de After Effects y Lottie (Offset, Round
  Corners, Zig Zag, Pucker & Bloat, Twist, Wiggle Paths) y Remotion `warpPath`.
- **API:**

  ```python
  star = scene.geometry.star(5, 1.5, 0.7)
  zz = star.modifiers.zigzag(size=0.0, ridges=24, smooth=True)
  star.modifiers.round_corners(0.15)
  scene.play([zz.animate.size(0.12).duration(1.0)])
  blob.modifiers.wiggle_path(size=0.1, detail=8, frequency=1.5, seed=3)  # vivo en el tiempo
  ring.modifiers.offset(0.1, join="round", copies=3)
  ```

- **Implementación:** Todos los operadores se evalúan en Rust sobre `BezPath`:
  - offset usa el offset de curvas de `kurbo`;
  - pucker y bloat mueven vértices y manejadores respecto al centroide;
  - twist rota cada punto en proporción a su radio;
  - zigzag y wiggle subdividen y desplazan a lo largo de la normal con ruido (PR-02).

  El morph actual sigue funcionando sobre el path resultante.
- **Hecho cuando:** Cada operador tiene un test geométrico y hay un baseline de la
  pila.

### TR-05 · Trazo con grosor variable

`M` · ★★

- **Qué:** Trazos que se afinan en los extremos, como pinceladas o flechas
  caligráficas. Un `show_passing_flash` afinado se ve mucho más fino.
- **Referencias:** ManimGL `VShowPassingFlash` (afinado) y los perfiles de ancho de
  Illustrator y Cavalry.
- **API:**

  ```python
  stroke.stroke_profile([(0.0, 0.1), (0.5, 1.0), (1.0, 0.0)])
  stroke.stroke_taper(start=0.2, end=0.4)
  ```

- **Implementación:** El trazo se convierte en un contorno relleno cuyo semiancho varía
  a lo largo de la longitud de arco. Se integra con TR-01.

---

## TX · Tipografía cinética y código

### TX-02 · Animador de rango de texto

`M` · ★★★

- **Qué:** Un **motor por glifo**: un selector de rango (inicio, fin, offset, forma y
  orden) modula desplazamiento, opacidad, escala, rotación, blur, tracking y color de
  cada unidad. Con esa sola primitiva salen cascadas, olas, blur-ins y la mayoría de
  presets de texto. Se implementa **antes** que TX-01, TX-05 y TX-07, que serían presets
  sobre él.
- **Referencias:** Text Animators de After Effects (Range Selector con forma
  square/ramp/triangle/round y Wiggly Selector), GSAP SplitText y Cavalry Sub-Mesh.
- **API:**

  ```python
  wave = title.animator(by="grapheme", shape="smooth", order="forward", seed=0)
  wave.set(offset=(0, -0.4), opacity=0.0, scale=0.6, rotation=0.2)  # estado "fuera"
  scene.play([wave.animate.sweep().duration(1.2)])  # recorre el rango 0 → 1
  ```

- **Implementación:** Se usan las selecciones `graphemes`/`words`/`lines`, que ya
  existen. Rust evalúa la influencia de cada unidad según su índice normalizado y la
  forma del selector, y aplica transformaciones locales. No hay callbacks Python.
- **Hecho cuando:** QW-01 y TX-01 pueden reexpresarse como presets del animador.

### TX-01 · Revelados con máscara por línea, palabra o carácter

`M` · ★★★

- **Qué:** Es *el* movimiento editorial de la tipografía cinética: cada línea o palabra
  sube desde detrás de una máscara invisible, escalonada.
- **Referencias:** GSAP SplitText (`mask: "lines"`), anime.js `splitText({lines:
  {wrap: "clip"}})` y Jitter.
- **API:**

  ```python
  title.animate.reveal(by="line", style="slide_up", mask=True, stagger=0.06)
  quote.animate.reveal(by="word", style="blur", stagger=0.04)  # también: fade, scale, slide_down
  headline.animate.conceal(by="line", style="slide_up")         # salida simétrica
  ```

- **Implementación:** Es un preset de TX-02. La máscara es un `clip` vectorial por
  unidad, que ya existe, así que funciona en SVG.
- **Hecho cuando:** Hay baseline para cada estilo con `by="line"` y `by="word"`.

### TX-03 · Máquina de escribir con cursor

`S` · ★★

- **Referencias:** Manim `TypeWithCursor`, Motion+ `Typewriter` y Motion Canvas (tween
  de strings).
- **API:**

  ```python
  prompt.animate.typewriter(cps=18, cursor="▍", blink=2.0, jitter=0.2, seed=0)
  prompt.animate.backspace(6)
  prompt.animate.retype("gaanim export --from clímax")
  ```

- **Implementación:** Se revelan ⌊t·cps⌋ grafemas con jitter determinista. El cursor
  sigue el avance del texto ya compuesto.

### TX-04 · Scramble / decode

`M` · ★★

- **Qué:** Un revelado tipo "decodificación" en el que los caracteres ciclan antes de
  fijarse. Da un tono técnico.
- **Referencias:** GSAP ScrambleText y Motion+ `ScrambleText`.
- **API:**

  ```python
  label.animate.scramble(charset="upper", reveal_delay=0.3, speed=20, seed=0)
  label.animate.scramble_to("LANZAMIENTO", charset="01")
  ```

- **Implementación:**
  - Cada glifo se elige con hash(seed, índice, ⌊t·speed⌋) y se fija de izquierda a
    derecha.
  - Para no rehacer el shaping en cada frame, se pre-shapea el charset en una caché de
    glifos y se reserva el ancho del texto final.
- **Hecho cuando:** El seek es exacto y el ancho no salta.

### TX-05 · Blur-in y tracking

`S` · ★★

- **API:**

  ```python
  title.animate.blur_in(sigma=0.3, by="grapheme", stagger=0.02)
  title.animate.tracking(0.4).duration(0)  # estado inicial
  title.animate.tracking(0.0)              # el espaciado se cierra
  ```

- **Implementación:** Son presets de TX-02 con FX-01. El tracking desplaza los glifos
  sin volver a hacer el layout del párrafo.

### TX-06 · Texto sobre trayectoria

`M` · ★★

- **API:**

  ```python
  ring_label = scene.text.on_path("GAANIM · MOTION · ", circle, align="start", orient=True)
  scene.play([ring_label.animate.path_offset(1.0).duration(4).easing(Easing.LINEAR)])
  ```

- **Referencias:** Text on path de After Effects y Rive (follow path).
- **Implementación:** Cada glifo se coloca por longitud de arco y se rota según la
  tangente (TR-03). El `path_offset` es animable.

### TX-07 · Subtítulos karaoke

`M` · ★★★

- **Qué:** Captions con la palabra activa resaltada. Es el estilo dominante en video
  corto y en la accesibilidad.
- **Referencias:** Remotion `@remotion/captions` (`createTikTokStyleCaptions`) y
  timestamps por palabra de Whisper.
- **API:**

  ```python
  subs = scene.captions("voz.srt", style="highlight", max_words=4, position="bottom")
  subs = scene.captions("voz.words.json", style="pill", active=GOLD)  # JSON por palabra de Whisper
  ```

- **Implementación:** Un parser SRT/VTT/JSON en Python puro que genera un spec con
  páginas y tokens `{text, start, end}`. Rust evalúa el estado activo por tiempo con
  TX-02. Encaja con `scene.media.audio(...)`.
- **Hecho cuando:** El ejemplo sincroniza con un audio de prueba y el export MP4
  incluye audio.

### TX-08 · Fuentes variables animadas

`L` · ★

- **API:**

  ```python
  title.axes(wght=300)
  scene.play([title.animate.axes(wght=900, wdth=110)])
  ```

- **Referencias:** Tipografía variable en motion (ejes `wght`, `wdth`, `slnt`).
- **Implementación:** Requiere soporte de variaciones en el shaping
  (`cosmic-text`/`swash`) y reshaping por frame con una caché por valor cuantizado. Hay
  que reservar el ancho para evitar reflujo.

### TX-09 · Animación de código por diff

`M` · ★★★

- **Qué:** Transformar un bloque de código en otro. Los tokens que se conservan se
  desplazan, los que se eliminan se desvanecen y los nuevos aparecen. También se pueden
  seleccionar líneas atenuando el resto. Es clave para explainers técnicos.
- **Referencias:** Motion Canvas `Code` (`code.edit`, `code.replace`, `selection`,
  `findAllRanges`) y Revideo.
- **API:**

  ```python
  snippet = scene.text.code(src_v1, language="python")
  scene.play([snippet.animate.edit(src_v2).duration(0.8)])
  scene.play([snippet.animate.select(lines=(2, 4), dim=0.3)])
  scene.play([snippet.animate.highlight(snippet.find("dt"))])
  ```

- **Implementación:** Diff por tokens (LCS) sobre el highlighting existente. Se
  reutiliza el matching de `transform_to`.
- **Hecho cuando:** Hay baselines de inserción, eliminación y movimiento.

---

## FX · Efectos, shaders y composición

### FX-01 · Efectos animables: `glow`, `blur`, `shadow`

`S` · ★★★

- **Qué:** Los efectos por objeto existen (`crates/gaanim_renderer/src/effects.rs`),
  pero son estáticos. Animarlos habilita blur-ins, pulsos de glow y sombras que crecen
  al "levantar" una tarjeta.
- **API:**

  ```python
  card.shadow(BLACK, 0, -0.05, 0.05)
  scene.play([card.animate.shadow(BLACK, 0, -0.25, 0.4).scale_to(1.04)])
  scene.play([orb.animate.glow(CYAN, radius=0.5, intensity=2.0).repeat(3, yoyo=True)])
  scene.play([hero.animate.blur(0.0).duration(0.6)])  # desde blur(0.3)
  ```

- **Implementación:** Lentes nuevas en `crates/gaanim_animation/src/tween.rs`, con
  interpolación de parámetros y colores.
- **Hecho cuando:** Hay un baseline de cada efecto en mitad de la animación.

### FX-02 · Uniforms enlazables y cadena de post-procesos

`M` · ★★★

- **Qué:** Hoy hay un único `PostProcess.shader` por escena o segmento, y sus únicos
  uniforms son `uv`, `resolution` y `time`. Hacen falta parámetros animables y varios
  pases encadenados.
- **API:**

  ```python
  amount = scene.viz.parameter(0.0)
  scene.canvas.post = [
      PostProcess.shader(ca_src, uniforms={"amount": amount}),
      PostProcess.grain(0.08),
  ]
  scene.play([amount.animate.set(1.0).duration(0.3).repeat(1, yoyo=True)])
  ```

- **Implementación:** Se extiende `GaanimPostParams`
  (`crates/gaanim_renderer/src/post_process.rs:15`) con un bloque de uniforms
  declarado y se hace ping-pong entre texturas en `post_process_gpu.rs` y en
  `crates/gaanim_export/src/gpu.rs`.
- **Hecho cuando:** El preview y el export dan el mismo resultado, con un baseline de
  cadena de 2 pases.

### FX-03 · Presets de acabado

`S` · ★★★

- **Qué:** Un acabado "cinematográfico" sin escribir WGSL. Hoy
  `examples/post_process_demo.py` implementa aberración cromática y viñeta a mano.
- **Referencias:** Tendencias 2024–2026 (grano analógico, gradientes, viñeta),
  `@remotion/effects` y los efectos Noise & Grain de After Effects.
- **API:**

  ```python
  PostProcess.grain(amount=0.06, size=1.0, animated=True)
  PostProcess.vignette(strength=0.35, softness=0.6)
  PostProcess.chromatic_aberration(amount=0.004)
  PostProcess.color_grade(exposure=0.0, contrast=1.1, saturation=1.05, temperature=0.1)
  PostProcess.lut("grade.cube")
  PostProcess.halftone(dot=6)
  PostProcess.dither(levels=4)
  PostProcess.crt()
  PostProcess.pixelate(8)
  PostProcess.glitch(intensity=glitch_amount, seed=3)  # acepta un Parameter animable
  ```

- **Implementación:** Son WGSL empaquetados sobre `PostProcessShader`. El grano es
  ruido con hash por frame, así que es determinista.

### FX-04 · Bloom multipaso

`M` · ★★★

- **API:**

  ```python
  PostProcess.bloom(threshold=0.8, intensity=0.6, radius=0.5)
  ```

- **Referencias:** Deep Glow de After Effects y el bloom físico de LearnOpenGL (cadena
  de mips dual-Kawase).
- **Implementación:** Pase de brillo, cadena de downsample y upsample, y composición
  aditiva con tonemapping. Usa FX-02. Complementa el `glow` por objeto, que es
  vectorial.

### FX-05 · Motion blur por sub-frames

`M` · ★★★

- **Qué:** El desenfoque de movimiento es la diferencia más visible entre una
  animación "de código" y una de estudio. Gaanim tiene una ventaja: el timeline es
  seekable y exacto, así que se pueden promediar sub-frames sin aproximaciones.
- **Referencias:** Remotion `<CameraMotionBlur shutterAngle samples>` y el shutter
  angle y phase de After Effects.
- **API:**

  ```python
  scene.motion_blur(shutter_angle=180, samples=8)  # por defecto solo en export
  hero.motion_blur(False)                          # exclusión por objeto
  ```

- **Implementación:** Se renderizan N sub-frames en
  [t + phase, t + phase + angle/360/fps], se promedian en espacio lineal y se evitan
  los sub-frames que caen en un cambio de segmento. El coste es N× render, así que el
  preview debería ofrecer un modo reducido.
- **Hecho cuando:** Los baselines de export con y sin blur son deterministas.

### FX-06 · Echo y estelas

`M` · ★★

- **API:**

  ```python
  ball.echo(count=5, delay=0.04, decay=0.6)
  ```

- **Referencias:** Echo de After Effects, Remotion `<Trail>` y Manim `TracedPath`
  (Gaanim ya tiene `traced_path`).
- **Implementación:** Se dibujan copias del objeto evaluado en t − k·delay. Como es
  vectorial, funciona en SVG.

### FX-07 · Modos de fusión por objeto

`S` · ★★

- **Qué:** El pipeline siempre usa `BlendMode::default()`
  (`crates/gaanim_renderer/src/pipeline.rs:381`). `screen` y `add` sirven para luces y
  destellos; `multiply` para resaltadores.
- **API:**

  ```python
  leak.blend("screen")
  marker.blend("multiply")
  ```

- **Implementación:** Un `push_layer` de Vello con el `peniko::BlendMode`
  correspondiente. Hay que documentar qué modos preserva el export SVG.

### FX-08 · Mates alpha y luma

`M` · ★★

- **Qué:** `clip(mask, invert)` es vectorial y binario. Los mates con degradado
  (alpha o luma) permiten revelados suaves y texturas dentro de texto.
- **Referencias:** Track mattes de After Effects y Lottie (alpha, alpha invertido,
  luma, luma invertido).
- **API:**

  ```python
  photo.matte(title, mode="alpha")
  scene_bg.matte(gradient_rect, mode="luma")
  ```

- **Implementación:** Primero hay que evaluar qué máscaras soporta Vello 0.9 dentro de
  una capa. Si no bastan, se usa una capa offscreen (FX-10).

### FX-09 · Fondos vivos

`S` · ★★

- **API:**

  ```python
  Background.mesh_gradient([NAVY, PURPLE, TEAL], speed=0.1, seed=4)
  Background.noise_gradient(colors, scale=1.5, seed=1)
  Background.aurora(colors, speed=0.2)
  Background.dot_grid(spacing=0.4)
  ```

- **Referencias:** Gradientes estilo Stripe (fBm con UV deformadas) y tendencias de
  mesh gradients.
- **Implementación:** Presets WGSL sobre `Background.shader`, que ya existe.

### FX-10 · Capas offscreen por objeto

`L` · ★★★

- **Qué:** Es la infraestructura para renderizar un subárbol a una textura y aplicarle
  un shader. Desbloquea shaders por objeto (distorsión, liquid, displacement), mates
  luma, vidrio y profundidad de campo por capa.
- **API:**

  ```python
  logo.shader_effect(PostProcess.shader(ripple_src, uniforms={"t": p}))
  ```

- **Implementación:** Requiere bounds con margen de efecto, una caché por fragmento
  retenido y composición de vuelta en orden de dibujo. Solo aplica a 2D raster; en SVG
  hay un fallback documentado.

### FX-11 · Metaballs / gooey

`M` · ★★

- **API:**

  ```python
  blob = scene.geometry.metaballs(circles, threshold=1.0, smoothness=0.4)
  ```

- **Referencias:** El efecto gooey (blur + umbral) y el smooth-min de SDF.
- **Implementación:** **Vector primero**: se evalúa un campo SDF con smooth-min, se
  extrae el contorno con marching squares y se ajusta a `BezPath`. Es nítido,
  exportable a SVG y reacciona al mover las bolas.

### FX-12 · Vidrio y backdrop blur

`L` · ★★

- **Qué:** Tarjetas de glassmorphism y *liquid glass* (refracción, borde Fresnel y
  saturación).
- **Implementación:** Necesita muestrear lo que hay detrás del objeto (FX-10), aplicar
  blur y opcionalmente refracción.

---

## TS · Transiciones

### TS-01 · Wipes, iris, push y blinds, con easing

`M` · ★★★

- **Hoy:** Existen `cut`, `cross_fade`, `fade_through`, `slide` y `zoom_through`, sin
  parámetro de easing.
- **Referencias:** Remotion `@remotion/transitions` (`wipe`, `clockWipe`, `iris`,
  `flip`, `springTiming`), Motion Canvas y las transiciones de After Effects (Linear,
  Radial y Iris Wipe, Venetian Blinds).
- **API:**

  ```python
  Transition.wipe(0.6, direction="left", feather=0.1)
  Transition.clock_wipe(0.8, start_angle=90)
  Transition.iris(0.7, center=(2, 1), shape="circle")  # también "star" o un Drawable
  Transition.blinds(0.6, count=8, angle=0)
  Transition.push(0.5, direction="up")
  Transition.slide(0.5, "left", easing=Easing.spring(bounce=0.2))  # easing en todas
  ```

- **Implementación:** Siempre que se pueda, la transición es un `clip` vectorial
  animado sobre el segmento entrante. Así funciona en SVG y sin texturas.
- **Hecho cuando:** Hay un baseline a mitad de cada transición.

### TS-02 · Transiciones por shader

`L` · ★★

- **API:**

  ```python
  Transition.shader(src, 0.8, uniforms={...})  # usa gaanim_from(uv), gaanim_to(uv) y progress
  Transition.preset("cross_zoom" | "directional_warp" | "ripple" | "glitch_displace" | "luma", 0.8)
  ```

- **Referencias:** gl-transitions (`transition(uv)` con `getFromColor`,
  `getToColor` y `progress`).
- **Implementación:** Se renderizan ambos segmentos a textura. La transición `luma`
  acepta una imagen en escala de grises como mapa de revelado.

### TS-03 · Magic move por claves

`M` · ★★★

- **Qué:** Entre dos estados, los objetos con la misma clave interpolan posición,
  tamaño, color y forma, y el resto aparece o desaparece. Es la transición favorita de
  presentaciones y producto.
- **Referencias:** Keynote Magic Move (por objeto, palabra o carácter), Figma Smart
  Animate (por nombre y jerarquía), GSAP Flip y Motion `layoutId`.
- **API:**

  ```python
  scene.play([magic_move(before_group, after_group, key="name", unmatched="fade").duration(0.8)])
  scene.segment("v2", transition=Transition.magic_move(0.8, key="name"))
  ```

- **Implementación:** Reutiliza `transform_matching`, las partes semánticas y
  `TransitionType::Morph` (QW-04). La clave puede ser un nombre de parte, un `id` SVG o
  un callable que se evalúa al construir la escena.

### TS-04 · Overlays sobre el corte

`S` · ★

- **API:**

  ```python
  Transition.cut(overlay=Overlay.flash(WHITE, 0.15))
  Transition.cross_fade(0.4, overlay=Overlay.light_leak(seed=2, hue=0.1))
  ```

- **Referencias:** Remotion `TransitionSeries.Overlay` y `lightLeak`.
- **Implementación:** El overlay se dibuja encima del corte sin cambiar la duración de
  los segmentos.

---

## CA · Cámara y profundidad

### CA-01 · Shake por trauma con ruido

`S` · ★★

- **Hoy:** `camera.animate.shake(amplitude, frequency)`.
- **Referencias:** Squirrel Eiserloh, "Juicing Your Cameras With Math" (GDC 2016): el
  shake crece con trauma², se mueve con ruido Perlin y el trauma decae.
- **API:**

  ```python
  scene.camera.animate.shake(trauma=0.8, decay=1.5, frequency=12, rotation=0.02, seed=0)
  ```

- **Implementación:** Se usa el ruido de PR-02 para traslación y rotación. Al ser una
  función pura del tiempo, el seek es exacto.

### CA-02 · Zoom perceptualmente uniforme

`S` · ★★

- **Qué:** Interpolar la escala de forma lineal hace que un zoom grande parezca que
  acelera. La interpolación exponencial s0·(s1/s0)^p se percibe como constante.
- **Referencias:** GSAP `ExpoScaleEase`.
- **API:**

  ```python
  scene.camera.animate.zoom_to(8.0, interpolation="exponential")  # por defecto en zoom_to/frame_to
  ```

### CA-03 · Cámara sobre trayectoria y whip pan

`M` · ★★

- **API:**

  ```python
  scene.camera.animate.follow_path(route, orient=True)
  scene.camera.animate.whip_pan(to=section_b, blur=True)
  scene.camera.animate.dolly_zoom(factor=1.5)  # vértigo, en 3D
  ```

- **Referencias:** Motion Canvas `followCurveWithRotation` y el whip pan (speed ramp
  más blur direccional).
- **Implementación:** El blur direccional del whip pan sale de FX-05 o de un pase
  dedicado.

### CA-04 · Capas de parallax 2.5D

`M` · ★★

- **API:**

  ```python
  far = scene.layer(depth=3.0)
  near = scene.layer(depth=0.6)
  far.add(mountains)
  near.add(trees)
  scene.play([scene.camera.animate.pan_to(6, 0).duration(3)])
  ```

- **Implementación:** Con cámara ortográfica, cada capa se desplaza por un factor
  1/depth del movimiento de cámara. `hud()` ya es el caso de profundidad 0. No requiere
  3D real.

### CA-05 · Profundidad de campo

`L` · ★

- **Implementación:** En 2.5D, cada capa recibe un blur proporcional a
  |depth − focus|, usando CA-04 y FX-10. La profundidad de campo 3D real exige un pase
  de profundidad.

### CA-06 · Extrusión 3D de formas y texto

`L` · ★★

- **API:**

  ```python
  logo3d = scene.geometry.extrude(logo, depth=0.3, bevel=0.03, material=Material3D.metal(GOLD))
  word = scene.geometry.extrude(scene.text("HOLA"), depth=0.2)
  ```

- **Implementación:** Triangulación del `BezPath` (tapas), paredes laterales y bevel.
  El resultado es un `Primitive3D` con los materiales y la iluminación existentes.

---

## AN · Anotación, énfasis y explicación

### AN-01 · Anotaciones a mano alzada

`M` · ★★★

- **Qué:** Subrayados, círculos, cajas, corchetes y tachados con trazo irregular que se
  dibujan solos. Dan calidez y dirigen la atención en explainers.
- **Referencias:** rough-notation (underline, box, circle, highlight, strike-through,
  crossed-off, bracket; `iterations` y `annotationGroup`) y el algoritmo de Rough.js.
- **API:**

  ```python
  ul = eq["rhs"].annotate.underline(color=RED, roughness=1.0, passes=2, seed=1)
  box = card.annotate.box(padding=0.1)
  scene.play([ul.animate.create(), box.animate.create()])
  scene.play([annotation_group(ul, box, circle).animate.show(stagger=0.3)])
  shape.sketchy(roughness=1.2, fill="hachure", seed=4)  # extensión: estilo rough para cualquier shape
  ```

- **Implementación:** El algoritmo de Rough.js sobre `BezPath`: se perturban los
  extremos, se añade curvatura con controles aleatorios cerca del 50 % y el 75 %, y se
  dibuja un doble trazo. El dibujado usa TR-01 y la semilla de PR-02.

### AN-02 · Resaltador tipo marcador

`S` · ★★

- **Hoy:** El `highlight` de las selecciones de texto es un `Circumscribe`.
- **API:**

  ```python
  quote.words[3:6].animate.marker(color=YELLOW, skew=0.05, blend="multiply")
  ```

- **Implementación:** Un rectángulo inclinado detrás del texto que crece desde la
  izquierda. Tiene más sentido con FX-07, pero sin blend puede ir detrás del texto.

### AN-03 · Puntas de flecha en cualquier trazo

`S` · ★

- **Hoy:** `grow_arrow` ya hace crecer flechas rectas y curvas con la punta sobre la
  tangente. Solo sirve para `arrow`, `curved_arrow` y `curved_arrow_arc`.
- **API:**

  ```python
  route.tip(end="arrow", start=None)
  scene.play([route.animate.grow_arrow()])  # también en connector(..., via=[...])
  ```

### AN-04 · Énfasis adicionales

`S` · ★★

- **Referencias:** Manim `Broadcast`, `AnimatedBoundary`, `FocusOn` y `Blink`, y
  ManimGL `FlashAround`/`FlashUnder`.
- **API:**

  ```python
  pin.animate.broadcast(count=4, max_scale=3.0, lag=0.2)  # ondas concéntricas
  card.animated_boundary([BLUE, PURPLE, CYAN], cycle_rate=0.5)
  scene.play([scene.fx.spotlight(target, dim=0.7).duration(0.5)])
  term.animate.flash_under()
  term.animate.flash_around()
  cursor.animate.blink(3)
  ```

### AN-05 · Carrera de barras

`M` · ★★

- **API:**

  ```python
  race = scene.viz.bar_race(frames, top=10, rank_smoothing=0.3, value_format="{:,.0f}")
  scene.play([race.animate.play().duration(20)])
  ```

- **Referencias:** D3 bar chart race (keyframes interpolados, rango suavizado para que
  las barras se adelanten con fluidez y etiquetas con ticker).
- **Implementación:** Extiende `scene.viz` y las charts declarativas, y reutiliza
  `rolling_number` para las etiquetas.

### AN-06 · Rutas y mapas

`L` · ★

- **Qué:** Trazar una ruta con un marcador móvil y la cámara siguiéndolo, incluidos
  arcos de gran círculo y GeoJSON con proyección.
- **Implementación:** Se construye con TR-01, TR-03 y CA-03.

### AN-07 · Anillos de progreso y temporizadores

`S` · ★

- **API:**

  ```python
  ring = scene.viz.progress_ring(value=0.0, width=0.12, label=True)
  scene.play([ring.animate.set(0.75)])
  timer = scene.viz.countdown(10)
  ```

- **Implementación:** `arc` más trim (TR-01) con extremos redondeados y un
  `rolling_number` en el centro.

---

## AU · Audio y sincronía

### AU-01 · Marcadores para voz en off

`M` · ★★★

- **Qué:** El código no lleva duraciones fijas. Los instantes se nombran en el código y
  sus tiempos viven en un archivo de datos que se ajusta escuchando la voz. Es la mejor
  ergonomía de sincronía que existe hoy.
- **Referencias:** Motion Canvas `waitUntil("evento")` con marcadores arrastrables
  guardados en el meta de la escena, y Theatre.js (`sequence.attachAudio`).
- **API:**

  ```python
  scene.media.audio("voz.wav")
  scene.play([title.animate.write()])
  scene.wait_until("ecuacion")  # el tiempo sale de escena.markers.json
  scene.play([eq.animate.write()])
  ```

- **Implementación:** El sidecar JSON vive junto al script, y el hot reload de assets del
  proyecto puede vigilarlo. El editor muestra los marcadores sobre la waveform, lo que encaja con el
  roadmap 0.3 de `engine_improvements.md`. Si falta un marcador, se usa por defecto un
  `wait(0)` y se emite una advertencia.
- **Hecho cuando:** Mover un marcador en el JSON retima la escena sin tocar el código.

### AU-02 · Análisis de audio como señales reactivas

`M` · ★★★

- **API:**

  ```python
  music = scene.media.audio("track.mp3")
  bass = music.band(20, 150, smoothing=0.6)  # ScalarSource en [0, 1]
  beats = music.beats()                       # tiempos de onset/beat
  scene.camera.bind_2d(zoom=computed(lambda b: 1 + 0.05 * b, inputs=[bass]))  # cualquier ScalarSource
  scene.play(parallel(*[logo.animate.indicate().delay(t) for t in beats[:8]]))
  ```

- **Referencias:** Remotion `visualizeAudio` y `useWindowedAudioData`.
- **Implementación:** Se decodifica con FFmpeg en `gaanim_media` y se calculan
  envolvente, FFT por ventana y onsets una sola vez, con caché por hash del archivo. La
  evaluación posterior es una búsqueda por tiempo, así que es determinista.

### AU-03 · Waveform y espectro como drawables

`S` · ★★

- **API:**

  ```python
  scene.viz.waveform(music, width=10, height=1.5, window=2.0)
  scene.viz.spectrum(music, bars=48, mirror=True)
  ```

### AU-04 · Efectos de sonido anclados

`S` · ★★

- **API:**

  ```python
  scene.media.sfx("whoosh.wav", at=scene.cursor)
  Transition.slide(0.5, "left", sound="whoosh.wav")
  title.animate.write().sound("typing.wav")
  ```

- **Implementación:** Son pistas cortas sobre el mezclador existente de
  `scene.media.audio`, ancladas a la animación, así que se mueven con ella.

---

## AS · Interoperabilidad de assets

### AS-01 · Lottie: segmentos, slots y reverse

`M` · ★

- **Hoy:** Hay `animation_id`, `theme_id`, `state_machine_id`, `speed`, `loop`,
  `set_theme`, `set_input` y `fire_event`.
- **API:**

  ```python
  icon.play_segment("hover")                                  # por marker de Lottie
  icon.slots(primary=BLUE, label="Nuevo")                     # overrides de color y texto
  icon.animation(reverse=True)
  ```

---

## Descartado o pospuesto

| Idea | Motivo |
|---|---|
| Runtime de Rive | Duplica la ruta Lottie/dotLottie. Revisar si hay demanda de state machines con data binding. |
| Motor de física rígida con colisiones | Es un estado acumulado difícil de seekear. PR-08 cubre los casos de motion design y lo demás se hornea con `add_updater_fn(fixed_dt=...)`. |
| Puppet pins / deformación ARAP | El coste es alto frente a la ruta glTF con skins, que ya existe. |
| Pixel sorting | Es de nicho y costoso en GPU (ordenamiento por pasadas). Puede ser un preset de FX-03 más adelante. |
| Editor visual de curvas y keyframes (Theatre.js) | Es valioso, pero depende de identidades estables e invalidación incremental (hot reload P2). Los marcadores sidecar de AU-01 son el primer paso. |
| Layout FLIP genérico | Gaanim ya anima reflows de layout (`examples/layout_reflow_demo.py`). TS-03 cubre el caso entre estados. |

## Fuentes consultadas

Varias webs oficiales (Adobe, Cavalry, GSAP, anime.js, Motion, Motion Canvas,
Remotion y Manim) no se pudieron descargar directamente desde el entorno de
investigación. Sus detalles salen de extractos de búsqueda y del código fuente público
cuando existía. Hay que confirmar las firmas exactas contra la documentación al
implementar cada ítem.

- GSAP: <https://gsap.com/docs/v3/> (Timeline, Staggers, Keyframes, SplitText, ScrambleText, DrawSVG, MorphSVG, MotionPath, Flip, Physics2D, Inertia, CustomEase/CustomWiggle/CustomBounce, EasePack)
- anime.js v4: <https://animejs.com/documentation/> (stagger, spring, createTimeline, splitText, svg, createLayout)
- Motion: <https://motion.dev/docs/spring>, <https://motion.dev/docs/stagger>, <https://motion.dev/docs/react-layout-animations>; fórmula del spring en `packages/motion-dom/src/animation/generators/spring.ts` de <https://github.com/motiondivision/motion>
- Motion Canvas: <https://motioncanvas.io/docs/flow/>, <https://motioncanvas.io/docs/time-events/>, <https://motioncanvas.io/api/2d/code/>, <https://motioncanvas.io/docs/transitions/>; Revideo: <https://github.com/midrender/revideo>
- Remotion: <https://www.remotion.dev/docs/spring>, <https://www.remotion.dev/docs/transitions/presentations>, <https://www.remotion.dev/docs/paths/>, <https://www.remotion.dev/docs/noise>, <https://www.remotion.dev/docs/motion-blur/>, <https://www.remotion.dev/docs/captions/create-tiktok-style-captions>, <https://www.remotion.dev/docs/visualize-audio>
- Manim Community: <https://docs.manim.community/en/stable/reference/manim.utils.rate_functions.html>, <https://docs.manim.community/en/stable/reference/manim.animation.indication.html>, <https://docs.manim.community/en/stable/reference/manim.animation.speedmodifier.ChangeSpeed.html>
- Lottie: <https://lottiefiles.github.io/lottie-spec/specs/shapes/>; dotLottie 2: <https://dotlottie.io/spec/2.0/>; Theatre.js: <https://www.theatrejs.com/docs/latest>; Rive: <https://rive.app/docs/>
- After Effects: <https://helpx.adobe.com/after-effects/using/shape-attributes-paint-operations-path.html>, <https://helpx.adobe.com/after-effects/using/expression-language-reference.html>, <https://helpx.adobe.com/after-effects/using/animating-text.html>, <https://helpx.adobe.com/after-effects/using/track-mattes-and-traveling-mattes.html>, <https://helpx.adobe.com/after-effects/using/transition-effects.html>
- Cavalry: <https://docs.cavalry.scenegroup.co/nodes/shapes/duplicator/>, <https://cavalry.studio/docs/nodes/utilities/falloff/>, <https://cavalry.studio/docs/nodes/behaviours/>
- Figma Smart Animate: <https://help.figma.com/hc/en-us/articles/360051748654>; Keynote Magic Move: <https://support.apple.com/guide/keynote/add-transitions-tanff5ae749e/mac>; Jitter: <https://jitter.video/product/>
- rough-notation: <https://github.com/rough-stuff/rough-notation>; algoritmos de Rough.js: <https://shihn.ca/posts/2020/roughjs-algorithms/>
- gl-transitions: <https://github.com/gl-transitions/gl-transitions>
- Bloom físico: <https://learnopengl.com/Guest-Articles/2022/Phys.-Based-Bloom>; camera shake (Eiserloh, GDC 2016): <http://www.mathforgameprogrammers.com/gdc2016/GDC2016_Eiserloh_Squirrel_JuicingYourCameras.pdf>
- Bar chart race: <https://observablehq.com/@d3/bar-chart-race-explained>
