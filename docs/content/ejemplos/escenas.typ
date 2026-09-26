// Catálogo de las escenas de ejemplo. El código vive en basicos.py y
// avanzados.py (una celda `# %% nombre` por escena, cada una un script
// completo); las páginas de ejemplos y la galería lo leen de aquí, así el
// código mostrado, la vista previa y la tarjeta de la galería no divergen.
// Este archivo no es una página: no se incluye en index.typ.

#let _cells(path) = {
  let out = (:)
  for chunk in read(path).split(regex("(?m)^# %% ")).slice(1) {
    let lines = chunk.split("\n")
    out.insert(lines.first().trim(), lines.slice(1).join("\n").trim())
  }
  out
}

#let _sources = (
  basicos: _cells("basicos.py"),
  avanzados: _cells("avanzados.py"),
)

#let catalogo = (
  (
    page: "basicos", cell: "basic_circle", label: "ej-circulo-rectangulo",
    title: "Círculo y rectángulo",
    summary: [Dos figuras entran a la vez con easings distintos, uno suave y otro de resorte, y salen en paralelo.],
  ),
  (
    page: "basicos", cell: "text_and_math", label: "ej-texto-matematicas",
    title: "Texto y matemáticas",
    summary: [Un título y una ecuación que se escriben trazo a trazo, seguidos de un subtítulo.],
  ),
  (
    page: "basicos", cell: "shapes_gallery", label: "ej-figuras",
    title: "Galería de figuras",
    summary: [Cinco primitivas de `scene.geometry` que entran escalonadas con `stagger`.],
  ),
  (
    page: "basicos", cell: "groups", label: "ej-grupos",
    title: "Grupos",
    summary: [Tres círculos agrupados que crecen, se desplazan y giran como un solo objeto.],
  ),
  (
    page: "avanzados", cell: "transforms", label: "ej-transformaciones",
    title: "Transformaciones entre segmentos",
    summary: [Un círculo se convierte en un título al cambiar de segmento con un fundido, y el título en una ecuación.],
  ),
  (
    page: "avanzados", cell: "structured_text", label: "ej-texto-estructurado",
    title: "Ecuaciones con partes con nombre",
    summary: [`part()` marca qué fragmento es la masa, así la transformación relaciona conceptos y no solo glifos parecidos.],
  ),
  (
    page: "avanzados", cell: "chart_story", label: "ej-grafico",
    title: "Un gráfico en una explicación",
    summary: [`ChartSpec` separa los datos de su representación; el gráfico resultante es un objeto más dentro del layout.],
  ),
  (
    page: "avanzados", cell: "vertical_layout", label: "ej-vertical",
    title: "Composición vertical",
    summary: [Encabezado, viñetas y pie en formato 9:16, colocados por Layout sin coordenadas manuales. Para exportarla, pide una resolución vertical con `--width 1080` y `--height 1920`.],
    preview: false,
  ),
  (
    page: "avanzados", cell: "title_card", label: "ej-portada",
    title: "Una apertura reutilizable",
    summary: [Un componente editorial que agrupa tipografía, alineación y tema en una portada.],
  ),
  (
    page: "avanzados", cell: "reactive_path", label: "ej-rosa",
    title: "Una curva trazada por un parámetro",
    summary: [Un único `Parameter` mueve el punto y `traced_path` deja su rastro: una rosa de tres pétalos.],
  ),
  (
    page: "avanzados", cell: "curve_parameter", label: "ej-curva",
    title: "Punto y tangente sobre una curva",
    summary: [`point_on_curve` y `tangent_on_curve` recorren una curva con un parámetro que va de 0 a 1.],
  ),
  (
    page: "avanzados", cell: "staggered_grid", label: "ej-cuadricula",
    title: "Cuadrícula escalonada",
    summary: [`stagger` con `origin=` propaga la entrada desde el centro y el cambio de color desde los bordes; `distribute` reparte los tamaños.],
  ),
  (
    page: "avanzados", cell: "scene_3d", label: "ej-3d",
    title: "Una escena 3D con material y cámara",
    summary: [Luz de estudio, dos primitivas con materiales distintos y una órbita de cámara.],
    preview: false, experimental: true,
  ),
)

// Source of an entry, with the preview directive when it has one. The
// directive line is not displayed; the gallery compiles the same text, so it
// reuses the page's cached preview.
#let scene-code(entry) = {
  let code = _sources.at(entry.page).at(entry.cell)
  if entry.at("preview", default: true) { code + "\n# output: preview.webp" } else { code }
}

#let scene-block(entry) = raw(scene-code(entry), lang: "python", block: true)
