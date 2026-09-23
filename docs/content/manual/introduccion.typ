#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Introducción",
  description: "Qué es Gaanim, cómo pensar una escena y cómo recorrer este manual",
  route: "/manual/introduccion/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Bienvenido a Gaanim

Gaanim es un motor de animación vectorial 2D controlado desde Python. Escribes
una escena con objetos, estilos y animaciones; Gaanim construye una línea de
tiempo y la muestra en una ventana o la exporta a un archivo.

Este manual supone que ya conoces lo básico de Python: variables, llamadas a
funciones, listas e importaciones. No necesitas conocer Rust, Bevy ni Vello.

== El modelo mental

Una animación se construye con cuatro piezas:

1. `Scene` define el lienzo, el fondo y la línea de tiempo.
2. Los métodos de la escena crean objetos como círculos, textos y flechas.
3. Los métodos fluidos configuran posición y apariencia.
4. `scene.play(...)` coloca animaciones en el tiempo; `scene.render()` o
   `gaanim export ...` produce el resultado desde el ejecutable.

```python
from gaanim import BLUE, Scene

scene = Scene(frame=(16, 9))
circle = scene.geometry.circle(1).fill(BLUE)
scene.play([circle.animate.create().duration(1.0)])
scene.render()
```

El objeto se crea primero y su aparición se programa después. La llamada
`circle.animate.create()` no dibuja inmediatamente: devuelve una descripción de
animación que `scene.play` añade a la línea de tiempo.

== Unidades y coordenadas

Gaanim usa unidades lógicas independientes de la resolución. El frame
predeterminado mide 16 × 9 unidades: el origen `(0, 0)` está en el centro, X
crece hacia la derecha (de `-8` a `8`) e Y hacia arriba (de `-4.5` a `4.5`).
Posiciones, tamaños, grosores de trazo y tamaños de texto usan la misma unidad;
los píxeles solo se eligen al exportar. Las
duraciones se expresan en segundos y los ángulos 3D en radianes, salvo que una
firma indique otra unidad.

== Cómo estudiar el manual

La guía rápida comienza un proyecto sobre movimiento circular. Los capítulos
siguientes amplían ese mismo proyecto con texto, composición, tiempo y
reactividad. Cada concepto enlaza después con su ficha técnica en la
#link("/api/")[referencia de la API].

Si prefieres aprender por piezas pequeñas, consulta los
#link("/examples/basic/")[ejemplos básicos]. Si ya conoces el flujo principal,
salta a #link("/manual/avanzado/")[temas avanzados].

== Siguiente paso

Continúa con #link("/getting-started/")[Instalación y primeros pasos].
