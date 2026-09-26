# Rediseño de la documentación de Gaanim

Auditoría crítica de `docs/` (versión 0.4.1, 2026-09-25) y propuesta de rediseño por
fases. Los hallazgos citan archivos bajo `docs/content/` salvo que se indique otra
ruta; se contrastaron con `crates/gaanim_python/gaanim/gaanim_core.pyi`, el launcher
y un build local del sitio.

## Veredicto

La infraestructura es buena: Typst genera web y PDF desde una sola fuente, los
ejemplos pueden ejecutarse con el runtime real, hay búsqueda con sinónimos y el
hero de la portada tiene identidad propia. El problema es el contenido. **La mayor
parte del código de la documentación nunca se ejecuta**, y por eso enseña métodos
que no existen, versiones de Python que el launcher rechaza y un tutorial que no
termina en un archivo funcional. Encima de eso, la arquitectura repite los mismos
temas en cuatro sitios y mezcla documentación de usuario con la de contribuidor.

Esto importa más de lo habitual: el wheel empaqueta `docs/content` como
`gaanim/_docs` y la skill `gaanim-docs` lo usa como fuente para los asistentes de
código en los proyectos de usuario. **Cada error de la documentación termina
convertido en código generado que falla.**

## Diagnóstico

### 1. El código no se valida (crítico)

- 33 de las 37 páginas pasan `code-langs: ()` a `docs-chapter`: manual, las 8
  lecciones del proyecto, guías, `examples/advanced` e instalación. Ningún bloque
  Python de esas páginas se ejecuta.
- En la referencia solo se ejecutan los `api-entry` con `# output:`: unos 106 de los
  292 bloques. Nunca se ejecutan `layout.typ`, `assets.typ`, 42 de los 43 bloques de
  `scene.typ` ni los 23 de `themes.typ`. Las firmas y las tablas de parámetros
  están escritas a mano y nada las compara con el stub.

Consecuencias verificadas:

| Error | Dónde | Realidad |
|---|---|---|
| `.about(0, 0)`, `move`, `rotate` como animaciones | manual/animaciones | `about_point`, `shift_by`, `rotate_by` |
| `lag` como parámetro de grupo | manual/animaciones, guia/04:46 | eliminado en 0.2; se usa `stagger(each=)` |
| "Python 3.12 o posterior" | guia/01:41, guides/projects:118, guides/slides:311 | el launcher exige 3.14 (`gaanim_project/src/lib.rs:22`) |
| `circle(100)…move_to(-350, 0)` | guia/01 | unidades lógicas 16 × 9 |
| `# output: …mp4` como forma de exportar | guia/08:44 | es una directiva del build de docs; el comando real es `gaanim export` |
| `Camera.perspective(…, duration=1.0) -> Anim` | api/scene:746 | devuelve `Camera` y no admite `duration` |
| `scene.canvas.safe_area()` | api/scene:68 | no existe (`set_safe_area`, `safe_width`, `safe_height`) |
| `from=` en `tracking_line`, `bar_between`… | api/mobjects:90, 1772-1869 | `from_` |
| Defaults en píxeles (`margin=32`, `width=520`) | api/mobjects:1300, 1313; api/matrices:28 | 0.32, 5.2, 0.24 |
| `fade_to`, `fade_transform`, `color_by`, `at_3d`, `material_to` | api/animations, text, mobjects | no existen |
| `Easing`, `content` y `circle_center` sin definir | guia/04:58, guia/05:58, guia/07:61 | `NameError` |
| Recetas sin `scene.render()` que dicen "Ejecuta este archivo" | examples/advanced:19-66 | no producen nada |

### 2. Una documentación de animación casi sin animaciones

El proyecto práctico (8 capítulos), el manual y las guías no muestran ni una
vista previa. Los "puntos de control" piden comprobar cosas que el lector no
puede ver (por ejemplo, "inspecciona θ = π/2" sin forma de hacer seek), y los
tiempos de snapshot de guia/08 (0–9 s) no coinciden con una línea de tiempo de
más de 15 s.

### 3. El proyecto práctico no termina en un archivo que funcione

- No hay listado final completo. Desde el capítulo 3 solo aparecen fragmentos,
  sin indicar dónde se insertan.
- Los capítulos 6 y 7 construyen dos diseños incompatibles. El 6 mueve `point`
  con `Updater.orbit` y una línea llamada `radius`. El 7 reasigna `radius = 1.5`
  y crea otro punto guiado por `theta`, sin pedir que se borre lo anterior.
  Resultado: dos puntos a velocidades distintas.
- Los capítulos 7 y 8 remiten a `examples/manual_movimiento_circular.py`, que no
  está en el wheel y además usa otros valores (radio 1.0 frente a 1.5, margen 0.4
  frente a 0.6).
- El capítulo 8 dice que ya se aprendieron `traced_path` y los *bindings*, pero
  ninguno aparece en la Parte II.
- El capítulo 5 escribe `x(t) = r cos(ωt)` para describir la altura (un seno).

### 4. Arquitectura de información duplicada y a la deriva

- **La escena del círculo aparece cinco veces**, con radios distintos: home,
  guía rápida, manual/escena y objetos (1.75), guia/02-08 (1.5) y el ejemplo
  canónico (1.0). La guía rápida promete "movimiento circular" y el punto nunca
  se mueve.
- **La instalación está en cinco páginas**: getting-started/index,
  getting-started/installation, guia/01, guides/projects y guides/slides.
  `init`/`check`/`export` aparecen en nueve.
- La reactividad se enseña de tres formas distintas en guia/06-07,
  manual/avanzado y examples/advanced.
- El menú lateral (`site-map` en `components/section.typ`) se mantiene a mano,
  aparte del orden del libro (`content/index.typ`), y ya divergen: `performance`,
  `migration-0-2` y `api/matrices` están en el libro pero no en el menú, y el orden
  de la API es distinto.
- Las carpetas mezclan idiomas y se confunden entre sí: `guia/` (proyecto) frente a
  `guides/` (producción), `manual/guia-rapida`, y además `examples/`,
  `getting-started/` y `api/` en inglés.
- No hay navegación anterior/siguiente: el tutorial depende de enlaces escritos a
  mano. El manual termina enviando a `guides/layout` y se salta la Parte II.
- `migration-0-2` (0.1 → 0.2) está dentro de "Taller de escenas" con la versión
  0.4.1 publicada, y no existen notas para 0.3 ni 0.4.

### 5. Material de contribuidor en la documentación de usuario

`guides/performance` está dirigida por completo a contribuidores
(`just benchmark`, `tests/performance/budgets.json`, jobs de CI). En
`guides/visual-regression` solo `--capture-stops` sirve en proyectos de usuario;
el resto habla de `tests/visual/`, `target\debug\gaanim.exe` y del CI. El tutorial
para principiantes enseña `GAANIM_SNAPSHOTS` (guia/08:52). Todo esto se empaqueta
en el wheel y llega a los asistentes de los usuarios.

### 6. La referencia: escrita a mano, desordenada y mitad en inglés

- **Cobertura:** tiene ficha de referencia real para unos 404 de 769 métodos.
  `Easing` (16 presets), `Color` y `Lottie` no tienen ninguna entrada. De
  `Drawable` se documentan 31 de 58 métodos y de `Scene`, 14 de 32. Nunca se
  nombran `CameraAnimation`, `LayoutItem`, `ConstraintSet` ni `SceneSections`.
- **Dos estilos de página:** fichas `api-entry` (mobjects, animations, text) y
  prosa con firmas en bloques de código (layout, assets, audio, themes).
- **Fichas inconsistentes:** 29 agrupan varios métodos ("fill / stroke / opacity /
  effects") y no se pueden enlazar por método. `kind` admite 14 valores libres;
  los desconocidos salen en mayúsculas inglesas y generan clases CSS con espacios.
- **Orden:** `api/mobjects` (2143 líneas) empieza por "Booleanas vectoriales",
  sigue con glTF y PBR, y las primitivas no llegan hasta la sección 6.
- **Duplicados:** "Geometría reactiva" está en scene y en mobjects;
  `Visualization.parameter` aparece dos veces en mobjects, con anclas duplicadas.
- **Idioma:** `api/index` promete explicaciones en español, pero scene, text y
  mobjects tienen más inglés que español. Hay frases que cambian de idioma a mitad
  ("Usa `always_redraw_arc` to regenerate…").
- **Nombres:** la página se titula "Objetos", vive en `/api/mobjects/` y enlaza
  "Ver Mobjects", aunque la API nunca usa la palabra *mobject*
  (`scene.geometry`, `media`, `viz`…).
- Los docstrings del stub tampoco sirven como fuente: unos 50 usan una API plana
  (`scene.circle(...)`) que ya no existe.

### 7. Lo aprendido de las olas 0–2 no llega a las guías

Las 26 funciones de motion design ya integradas (springs, easings expresivos,
`repeat`/`yoyo`, stagger por rejilla, ruido con semilla, `wiggle`, glow/blur,
trim paths, animador de texto, máscaras de revelado, scramble, transiciones,
shake, marcador…) solo aparecen en la referencia. Ninguna guía ni ejemplo las
enseña, y la Ola 3 (post-proceso) añadirá más.

### 8. Diseño visual e interfaz

Revisado en un build local, a 1440 px y a 390 px:

- **Recuadros sin estilo en la web:** "Idea clave" y "Punto de control" solo
  tienen estilo en el PDF (`components/tutorial.typ` usa `block`). En la web se
  ven como texto suelto en mayúsculas.
- **"Última actualización"** es la fecha del build (`datetime.today()`), no la del
  contenido, y aparece debajo del primer subtítulo de cada página.
- **`/api/` roto visualmente:** tarjetas en damero con huecos; "Matrices" no
  tiene enlace; la numeración salta ("01 — NÚCLEO", "TEXT — UNIFICADO",
  "MATH — MATRIX"); los contadores se contradicen ("58 fábricas" frente a
  "más de 40", "22 animaciones" frente a 39 fichas); dice "Layout v2" y la ruta
  de navegación repite "Referencia de la API / Referencia de la API".
- **Tipografía:** la CSS declara `Inter` pero no la incluye (se usa la del
  sistema). Se incluye `Aleo`, que la web no usa, y el PDF va en Aleo, así que web
  y PDF no se parecen.
- **Home saturada:** ocho bloques seguidos (features, tres pasos, primera escena,
  nueve tarjetas, tabla, tres tarjetas, recursos) que repiten los mismos enlaces.
- Quedan etiquetas en inglés en la interfaz (`Code:` en las celdas).
- No se indica a qué versión corresponde la documentación.
- **Móvil:** no hay desbordamiento horizontal de la página. Los bloques de código
  largos se desplazan dentro de su caja, que es aceptable.

## Principios del rediseño

- **La API 3D se presenta como experimental.** Tiene errores conocidos y
  mucho trabajo pendiente: cada sección que la documenta abre con el aviso
  `#experimental()` (`docs/components/tutorial.typ`).

1. **El código de la documentación se ejecuta, siempre.** No ejecutarlo es la
   excepción: se declara con `# no-run: <motivo>`. CI falla con cualquier error.
2. **Una fuente para cada hecho.** Firmas y valores por defecto salen del stub.
   El orden del menú sale de `index.typ`. La instalación vive en una sola página.
   La escena del círculo vive en un solo archivo, que se incluye en el wheel.
3. **Cada tipo de página, para una necesidad.** Aprender (tutorial), resolver
   una tarea (guías), consultar (referencia) y entender (conceptos), al estilo de
   Diátaxis. Ninguna página mezcla los cuatro.
4. **Ver antes de leer.** Toda página de aprendizaje o guía muestra la animación
   resultante.
5. **Español para las explicaciones; inglés para los identificadores.** Esto
   incluye rutas, menú e interfaz. Un glosario fija los términos.
6. **La documentación de usuario no habla del repositorio.** Lo que requiere un
   checkout de Gaanim va a la documentación de contribución, que no se empaqueta.

## Nueva arquitectura

```text
/                       Inicio: hero, 3 pasos, galería breve, entrada a cada sección
/empezar/
  instalacion/          Única página de instalación (Windows, Ubuntu, verificación, problemas)
  primera-animacion/    La única "primera escena": crear, previsualizar, exportar
  como-piensa-gaanim/   Modelo mental: escena, handles, .animate, línea de tiempo, unidades
/tutorial/              "Del círculo al seno": 8 lecciones que siempre compilan
  01-…/ … 08-…/         Cada lección: objetivo → cambios → archivo completo → vista previa
/guias/
  texto-y-ecuaciones/   roles, Typst, selecciones, write/reveal
  movimiento/           easings, springs, stagger, repeat, trayectorias (olas 1–2)
  tipografia-cinetica/  animador de rango, máscaras, typewriter, scramble
  transiciones/         wipes, iris, overlays, morph
  efectos/              glow/blur/shadow y, desde la Ola 3, post-proceso
  graficas-y-datos/     ejes, funciones, ChartSpec, estadística
  layout/               anclas, filas, grids, regiones
  reactividad/          Updater, geometría reactiva, trazos (una sola forma de enseñarla)
  camara-y-3d/
  audio-y-video/
  presentaciones/
  proyectos-y-exportacion/   gaanim.toml, estructura, export, check
  capturas-y-comparacion/    --capture-stops y snapshots en proyectos de usuario
/ejemplos/              Galería en cuadrícula de vistas previas; cada ejemplo, su página
/referencia/
  scene/ geometry/ text/ viz/ layout/ media/ camera/ animate/ easing/
  color-y-temas/ updater/ assets/ audio/
  cli/                  gaanim, init, check, export, --present, --diff
  gaanim-toml/          esquema del manifiesto
/apendices/
  novedades/            changelog por versión con migraciones (0.2, 0.3, 0.4…)
  glosario/  solucion-de-problemas/
```

Qué pasa con las páginas actuales:

| Actual | Destino |
|---|---|
| getting-started/index + installation + secciones de instalación de guia/01, projects y slides | `empezar/instalacion` |
| home "Tu primera escena", manual/introduccion, getting-started "Tu primera animación", manual/guia-rapida | `empezar/primera-animacion` + `empezar/como-piensa-gaanim` |
| manual/escena, objetos, animaciones | se reparten entre `como-piensa-gaanim` y las guías; el manual desaparece como sección |
| guia/01-08 | `tutorial/`, reescrito para que compile |
| manual/avanzado, examples/advanced | `guias/reactividad`, `guias/camara-y-3d`, `ejemplos/` |
| examples/basic | `ejemplos/` |
| guides/layout, slides, projects | `guias/` |
| guides/visual-regression | parte de usuario → `guias/capturas-y-comparacion`; resto → docs de contribución |
| guides/performance | docs de contribución |
| guides/migration-0-2 | `apendices/novedades` |
| api/matrices | `referencia/viz` |
| api/mobjects | `referencia/geometry`, `media` y `viz`, según el namespace real |

## Referencia generada desde el stub

`docs/src/pyapi.rs` ya analiza el stub para el buscador. La propuesta es
ampliarlo para que las fichas tomen del stub todo lo que sea mecánico:

```typst
#api-entry("Geometry.tracking_line")[
  Línea cuyos extremos siguen a dos drawables en cada frame.
  // Firma, parámetros, tipos, valores por defecto y retorno: del stub.
  ```python
  # output: preview.webp
  …
  ```
]
```

- Una ficha por método. Nada de "fill / stroke / opacity" en una sola ficha: se
  agrupan visualmente por sección, no en la misma entrada.
- El build falla si una ficha nombra un símbolo inexistente.
- Un informe de cobertura lista los símbolos públicos sin ficha; empieza como
  aviso y se endurece por clase.
- `kind` se deriva del stub (clase, método, función, propiedad, constante).
- Las descripciones en español se escriben en la docs. Los docstrings del stub
  siguen en inglés para el IDE, pero se corrigen los ~50 que usan la API plana.

## Plan por fases

Cada fase es un PR que se puede revisar por separado.

### Fase 0: corregir lo que hoy es falso (S) · hecha

Pendiente para fases posteriores: la parte de contribuidor de
`guides/visual-regression` (Fase 2) y las descripciones de fichas que siguen en
inglés (Fase 4).

- Corregir la tabla de errores del §1, los nombres sin definir del tutorial y la
  versión de Python (3.14 en todas partes).
- En guia/08, sustituir `# output:` por `gaanim export`.
- Dar estilo web a `lesson-box` (callout con borde y fondo en ambos temas).
- Quitar `updated: datetime.today()`: o se usa la fecha del último commit de la
  página, o no se muestra.
- Arreglar la ruta de navegación duplicada, la tarjeta "Matrices", los contadores
  y el damero de `/api/`.
- Traducir las etiquetas de la interfaz (`Code:`).

### Fase 1: ejecución obligatoria (M) · hecha

- Todo bloque `python` se ejecuta con el runtime real; `# no-run: <motivo>` es la
  excepción explícita. Los fragmentos sin `render()` se validan contra el módulo
  embebido, así que cada nombre que usan tiene que existir.
- `# continue` repite, oculta, la celda anterior de la misma página: un tutorial
  en fragmentos se valida entero.
- `docs/fixtures/` es un proyecto de muestra enlazado en el directorio de cada
  celda, para que los ejemplos de recursos se ejecuten de verdad.
- El build falla ante cualquier ejemplo con error (`--allow-example-errors` para
  borradores) e imprime un resumen. El workflow `Docs` corre también en PRs que
  tocan `crates/**`.
- Las celdas pendientes se ejecutan en paralelo (`--jobs`) y se recompila una vez.
- La caché registra la versión del stub y del runtime: al cambiar, una vista
  previa se revalida con `check` en vez de renderizarse, y los fallos se
  reintentan siempre.
- Las fichas `api-entry` fallan si nombran símbolos que el stub no expone y, sin
  firma escrita, muestran la del stub.

### Fase 2: arquitectura (M) · hecha

- Mover las páginas según la tabla anterior, con rutas en español y páginas de
  redirección para las rutas antiguas enlazadas desde README y plugins.
- Generar el menú lateral a partir de `index.typ`, no desde `site-map`.
- Añadir navegación anterior/siguiente.
- Separar la documentación de contribución: excluirla del wheel en
  `crates/gaanim_python/hatch_build.py` o moverla fuera de `docs/content`.

### Fase 3: tutorial reescrito (M) · hecha

- Un único archivo canónico (`docs/content/tutorial/circulo_al_seno.py`), que el
  wheel incluye. Cada lección muestra el diff y el archivo completo con su vista
  previa.
- Una sola forma de reactividad, coherente con `guias/reactividad`.
- Puntos de control con capturas en el instante que piden revisar.

### Fase 4: referencia completa (M-L) · hecha

- Una página por namespace.
- Cobertura de `Easing`, `Color`, `Lottie`, `Camera`/`CameraAnimation`, `Canvas`,
  `Drawable` y `Scene`.
- Unificar layout, assets, audio y themes al formato de fichas.
- Referencia de la CLI y de `gaanim.toml`.

### Fase 5: contenido nuevo (M) · hecha

- Guías de movimiento, tipografía cinética, transiciones y efectos con lo
  entregado en las olas 0–2.
- Añadir a la definición de "hecho" de cada ítem del backlog de motion design
  (`motion_design_backlog.md`, paso 2) una sección en su guía, no solo la ficha.
- Galería de ejemplos y novedades por versión.

### Fase 6: pulido visual (S-M) · hecha

- Home en cuatro bloques: hero, tres pasos, galería y cuatro puertas (Empezar,
  Tutorial, Guías, Referencia).
- Una sola familia tipográfica compartida entre web y PDF.
- Indicador de versión y enlace "Editar esta página".

## Validación continua

- CI de docs: build completo, ninguna celda con error, ningún símbolo de API
  desconocido y cobertura que no disminuya.
- Una comprobación ligera, sin runtime, con `audit.py` del plugin `gaanim-dev`:
  bloques `python` sin `# no-run` que no se ejecutan, rutas del menú que no
  existen y páginas del libro ausentes del menú.
- El glosario como fuente de términos: *drawable* (no *mobject*), *handle*,
  *fábrica*, *línea de tiempo*, *escena*.

## Hallazgos del runtime (fuera del alcance de la documentación)

Ejecutar todos los ejemplos destapó comportamientos del runtime que la
documentación ahora describe tal como son, pero que conviene corregir en código:

- `Text.next_to`, `to_edge` y `to_corner` usan un espaciado por defecto de
  `24.0` (`crates/gaanim_python/src/pytext.rs`) frente al `0.24` del stub.
- `animate.rotate_by(...).about_point(...)` ignora el punto
  (`crates/gaanim_api/src/anim.rs:652`); `with_pivot` sí funciona.
- `Geometry.transform_matching*` no avanza `scene.cursor` ni la línea de tiempo.
- `Scene()` sin fondo ni tema dibuja blanco sobre blanco; no hay tema por
  defecto aunque la documentación antigua decía `technical`.
- Los formatos de eje `"pi"` y `"fraction"` no simplifican (`2π/2`).
- `bind_y_from` ignora una fuente movida con `follow(PointRef)`.
- `text.animate.transform_to` pierde los colores de `part()`.
- Un `Text` con `wrap="auto"` en contenedores `hug` envuelve distinto de lo que
  el layout midió; un `fade_in` dentro de `layout.column` se ve desde t=0.
- `chart.animate.to(...)` deja un gráfico 2D invisible en la exportación.
- `scene.media.svg` importa a una unidad por píxel SVG.
- `marker(blend="multiply")` se dibuja igual que `"normal"`.
- El stub declara `Canvas.theme` como escribible (es de solo lectura) y omite
  `Color.r/g/b/a`; `load_project()` exige `assets_dir` aunque la CLI lo supone.
- 3D (experimental): la exportación nativa necesita una ventana (Xvfb en CI),
  registra errores "use-after-free" de Bevy y un modelo `robot.glb` no apareció en la exportación.
