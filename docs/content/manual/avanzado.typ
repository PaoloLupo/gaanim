#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Temas avanzados",
  description: "Reactividad, visualización, 3D, presentaciones y pruebas visuales",
  route: "/manual/avanzado/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Reactividad

Los `Parameter`, `ValueTracker`, bindings y `Updater` permiten que una parte de
la escena dependa de otra. Para completar el proyecto de movimiento circular,
un punto puede orbitar y una proyección puede dibujar una curva sin reconstruir
la escena desde Python en cada frame.

El ejemplo canónico es `examples/sine_curve_unit_circle.py`. Combina
`Updater.orbit`, `tracking_line`, `bind_y_from` y `traced_path`, e incluye
capturas deterministas para regresión visual.

== Visualización de datos

Las escalas, ejes, espacios tipados y gráficos convierten datos en drawables
normales. Esto permite aplicarles Layout, temas y animaciones con claves
estables. Consulta #link("/api/visualization/")[Visualización] para elegir entre
una composición científica y un `ChartSpec` declarativo.

== Escenas 3D

Gaanim dispone de geometría, cámara, iluminación y modelos glTF 3D. Mantén las
unidades y la orientación coherentes; valida primero una escena estática y
añade movimientos de cámara después.

== Producción

- #link("/guides/projects/")[Proyectos] explica manifiesto, recursos y salidas.
- #link("/guides/slides/")[Presentaciones] cubre segmentos, notas y paradas.
- #link("/guides/visual-regression/")[Regresión visual] enseña capturas,
  comparación y aprobación de cambios.
- #link("/api/")[Referencia de la API] contiene firmas y contratos técnicos.

== Criterio de dominio

Una escena avanzada sigue siendo mantenible cuando separa contenido, estilo,
composición y tiempo; usa claves estables para transformaciones; evita
coordenadas manuales en estructuras editoriales; y tiene al menos una forma
repetible de validar su salida.
