#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Partes de una escena",
  description: "Viewport, objetos, línea de tiempo, cámara y salida",
  route: "/manual/escena/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Anatomía de `Scene`

`Scene` es el punto de entrada de la API. Conserva la configuración visual,
las especificaciones de los objetos y el cursor de la línea de tiempo. Una
escena habitual se lee de arriba abajo en cinco zonas.

== Configuración

```python
scene = Scene(1920, 1080, background="#0f172a", margin=64)
```

El viewport establece el sistema de coordenadas. `margin` define un área
segura que puede usar Layout; no desplaza manualmente cada objeto.

== Construcción

Las fábricas `scene.circle`, `scene.text`, `scene.arrow` y similares registran
objetos en la escena. Guarda sus handles con nombres que expresen su función:

```python
orbit = scene.circle(140)
moving_point = scene.dot(12)
explanation = scene.text("Radio constante", role="body")
```

== Apariencia y composición

El estilo describe cómo se dibuja un objeto. La composición decide dónde va.
Usa coordenadas para movimientos geométricos deliberados y Layout para
estructuras editoriales como títulos, columnas y tarjetas.

== Línea de tiempo

`play` avanza el cursor según la animación más larga del grupo; `wait` añade
una pausa. Varias llamadas consecutivas se reproducen en secuencia.

```python
scene.play([orbit.create().duration(1.0)])
scene.play([moving_point.fade_in().duration(0.3)])
scene.wait(1.0)
```

Los segmentos permiten nombrar partes de una presentación y reutilizarlas en
exportaciones o navegación en vivo.

== Salida

Una escena debe terminar con una intención clara: `render()` para el editor,
`export(...)` para un archivo o `snapshots(...)` para regresión visual. Evita
combinar salidas incompatibles sin una condición explícita.

== Cámara y clipping

La cámara cambia la vista, no las coordenadas locales de los objetos. Clipping
y máscaras limitan qué fragmentos son visibles. Son herramientas de
composición avanzada; consulta #link("/api/scene/")[la referencia de Scene]
para sus firmas exactas.

== Siguiente paso

Continúa con #link("/manual/objetos/")[Objetos, texto y estilos].
