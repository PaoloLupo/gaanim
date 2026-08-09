# Plan de visualización: coordenadas, funciones, datos y estadística

## Resumen

Este documento define la futura evolución de Gaanim para ofrecer un sistema
unificado de coordenadas, representación de funciones y visualización de datos.
La implementación deberá priorizar una API clara para videos educativos, una
composición más flexible que Manim y regeneración nativa de los elementos
visuales cuando cambien parámetros, dominios o datos.

El alcance actual termina en representación matemática y estadística. No
incluye motores de sistemas dinámicos, integración de trayectorias ni PDE.

## Estado de implementación

La primera versión funcional de este plan ya está integrada en el workspace:

- `gaanim_expr` implementa expresiones escalares y vectoriales, parámetros,
  evaluación CPU y diferenciación automática.
- `gaanim_visualization` implementa escalas, ticks, espacios, muestreo,
  campos estáticos, tablas, fuentes de datos y marcas estadísticas.
- La API Rust y Python expone handles tipados para líneas, planos cartesianos,
  polares, complejos y 3D, además de plots, superficies y herramientas de
  cálculo.
- Las capas 2D son drawables independientes, `CoordinateRef` sigue la
  jerarquía del espacio y `animate_view` anima una vista afín sin destruir el
  transform de layout del espacio.
- Los plots basados en `Expr`/`Parameter` y las marcas basadas en `DataSource`
  se regeneran de forma nativa; las lambdas Python se muestrean una sola vez.
- Los ejemplos y la documentación Typst usan la nueva API pública. Los antiguos
  entrypoints Python de plots y ejes ya no forman parte de la superficie
  pública.

Quedan como mejoras posteriores el remuestreo y relabeling durante
`animate_view`, transiciones de filas emparejadas por clave, una implementación
completa del protocolo `__dataframe__`, leyendas/colorbars públicas, clipping
geométrico estricto y la retirada del código Rust interno que todavía sostiene
los ejes 3D heredados. Estas limitaciones no reintroducen ODE, sistemas
dinámicos ni PDE en el alcance actual.

## Alcance

### Incluido

- `NumberLine`, ejes cartesianos, `NumberPlane`, plano polar, plano complejo y
  ejes 3D.
- Curvas escalares, paramétricas, polares e implícitas.
- Curvas y superficies 3D.
- Campos vectoriales estáticos 2D y 3D, sin integración de trayectorias.
- Herramientas educativas: coordenadas, proyecciones, tangentes, normales,
  secantes, derivadas, áreas y sumas de Riemann.
- Representaciones de datos mediante line, step, area, scatter, bars,
  histogram, box, violin, error bars, heatmap y contour.
- Reactividad nativa ante parámetros animados, cambios de dominio y
  actualizaciones de datos.
- Escalas lineales, logarítmicas, symlog, de potencia, temporales y
  categóricas.

### Fuera de alcance por ahora

- ODE, mapas discretos y modelos de simulación.
- Streamlines, trayectorias integradas, retratos de fase, nullclines y análisis
  de estabilidad.
- PDE de cualquier dimensión.
- Compute shaders, volúmenes, isosuperficies y raymarch.
- CFD, bifurcaciones, exponentes de Lyapunov y álgebra simbólica general.
- Una gramática estadística completa, faceting y dashboards interactivos.
- Compatibilidad con la API antigua o con Manim.

## API futura

### Ejes y espacios de coordenadas

- Introducir builders fluidos e inmutables:
  - `Axis.linear`, `Axis.log`, `Axis.symlog`, `Axis.power`, `Axis.time` y
    `Axis.category`.
  - Configuración reutilizable de dominios, cruces, ticks, formatos, etiquetas
    y estilos.
- `Scene` ofrecerá `number_line`, `axes`, `number_plane`, `polar_plane`,
  `complex_plane` y `axes_3d`.
- Los espacios retornarán handles tipados derivados de `CoordinateSpace`, en
  lugar de un `Drawable` genérico.
- `space.coord(...)` producirá un `CoordinateRef` simbólico que seguirá el
  layout, las transformaciones y los cambios de dominio del espacio.
- `data_to_local` y `local_to_data` serán las conversiones inmediatas. No
  dependerán de dimensiones de canvas hardcodeadas.
- Las capas `axis`, `grid`, `ticks`, `numbers` y `labels` serán handles reales e
  individualmente animables.
- `animate_view(...)` animará pan, zoom y dominio, remuestreando las
  visualizaciones asociadas.

### Funciones y herramientas educativas

- Los métodos de representación vivirán en el espacio correspondiente:
  `plot`, `parametric`, `polar_plot`, `implicit`, `surface`, `contour` y
  `vector_field`.
- El muestreo adaptativo usará error visual, clipping y separación automática
  ante `NaN`, infinitos, discontinuidades y asíntotas.
- También habrá un modo de muestreo fijo para resultados controlados y pruebas
  reproducibles.
- Las ayudas educativas se derivarán del plot o expresión original para evitar
  duplicar dominios y conversiones: puntos, etiquetas de coordenadas,
  proyecciones, tangentes, normales, secantes, derivadas, áreas y sumas de
  Riemann.
- Los campos vectoriales únicamente evaluarán y dibujarán una función vectorial
  sobre una grilla. No calcularán streamlines ni moverán objetos.

### Expresiones y reactividad

- Añadir `Expr`, variables y `Parameter` para evaluar expresiones reactivas en
  Rust.
- La expresión soportará operaciones escalares y vectoriales, funciones
  trascendentes, condicionales y diferenciación automática.
- Los plots dependientes de `Parameter` se regenerarán de forma nativa cuando
  cambie la señal, sin ejecutar Python por frame.
- Las lambdas Python seguirán disponibles para plots estáticos y se evaluarán
  durante la construcción de la escena.
- No se incluirá resolución simbólica general ni compilación WGSL en esta
  etapa.

### Datos y estadística

- `DataTable` aceptará columnas Python, objetos con buffer como NumPy y el
  protocolo `__dataframe__`, sin introducir dependencias obligatorias de NumPy
  o pandas.
- `DataSource` permitirá reemplazar y anexar datos compartidos por varias
  visualizaciones.
- Las transiciones entre datasets emparejarán filas mediante una clave estable
  opcional; cuando no exista, usarán el índice.
- Las marcas iniciales serán line, step, area, scatter, bars, histogram, box,
  violin, error bars, heatmap y contour.
- Las visualizaciones compartirán escalas, leyendas, colorbars, colormaps y
  paletas accesibles.
- Los valores no finitos usarán `gap` por defecto en líneas y `drop` en
  agregados. También estarán disponibles las políticas explícitas `gap`,
  `drop` y `error`.
- La implementación priorizará geometría agrupada e instancias, evitando una
  entidad ECS por punto, barra o tick.

## Arquitectura futura

- Añadir `gaanim_expr` para el AST de expresiones, evaluación CPU y
  diferenciación automática.
- Añadir `gaanim_visualization` para escalas, ticks, transformaciones,
  muestreo, espacios y marcas.
- Incorporar `SceneSet::Visualization` después de `SceneSet::Updaters` y antes
  de `SceneSet::Layout`.
- Mover la generación de ejes fuera del compilador monolítico de `gaanim_api`.
- Mantener transformaciones y bounds preparados para 3D mediante los tipos
  nativos `glam`, `kurbo` y `peniko`, sin introducir wrappers gráficos
  equivalentes.
- Añadir paths e instancias 2D agrupadas, además de meshes 3D con color por
  vértice, para datasets grandes y superficies.
- Mantener la API Python fluida y diferida mediante `gaanim_api`, PyO3, stubs y
  exports dedicados.
- Renombrar el actual `CoordinateSystem`, que representa unidades globales, a
  `CanvasUnits`; `CoordinateSpace` quedará reservado para espacios matemáticos.
- Eliminar durante la implementación `AxesConfig`, `Axes3DConfig`,
  `scene.plot`, `get_graph`, `function_graph`, los monkey patches y las firmas
  de ejes con decenas de keywords.
- Migrar conjuntamente los ejemplos, la documentación Typst y las llamadas
  internas afectadas. No se mantendrán shims de compatibilidad.

## Verificación futura

- Probar transformaciones data↔local, escalas, ticks, formatos y dominios
  inválidos.
- Verificar que `CoordinateRef` permanece alineado después de mover, escalar,
  rotar, relayout y animar el espacio.
- Cubrir muestreo adaptativo, asíntotas, discontinuidades y clipping.
- Validar diferenciación automática y regeneración por `Parameter` sin
  callbacks Python por frame.
- Probar buffers, datos categóricos y temporales, valores faltantes,
  histogramas y estadísticas de box/violin.
- Añadir ejemplos y visual diffs para planos, cálculo, superficies 3D, campos
  vectoriales estáticos y una galería estadística.
- Ejecutar las pruebas focalizadas de los crates nuevos antes de `just check`,
  `just clippy`, `just python-develop`, `just validate-python-api` y
  `just docs`.
- Comparar las nuevas fixtures visuales sin aprobar baselines automáticamente;
  cualquier `--bless` requerirá aprobación explícita.
