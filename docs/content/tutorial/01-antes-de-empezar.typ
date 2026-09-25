#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Antes de empezar",
  description: "Qué vamos a construir y cómo trabajar con la guía",
  route: "/tutorial/antes-de-empezar/",
)[

= Un proyecto que crecerá contigo

Esta guía no es un catálogo de funciones. Es el relato de un proyecto. Vamos a
construir una explicación animada de una idea matemática: cómo el movimiento
de un punto sobre un círculo produce una curva seno.

Al principio solo veremos un círculo quieto. Después aprenderemos a darle
estilo, controlar el tiempo, organizar texto, mover un punto de manera
reactiva, proyectar su altura y conservar el recorrido como una curva. Al final
tendremos un proyecto que se puede previsualizar, exportar y comprobar.

#idea[
Cada capítulo parte del archivo del capítulo anterior. No copies todos los
fragmentos a la vez. Ejecuta el proyecto después de cada cambio y observa qué
responsabilidad acaba de aparecer.
]

== Lo que necesitas saber

La guía supone Python básico: imports, variables, funciones, listas y bloques
`if`. No necesitas conocer Rust, Bevy, Vello ni programación de GPU.

Gaanim usa una API fluida. Una expresión como
`scene.geometry.circle(1.5).stroke(WHITE, 0.05).move_to(-4, 0)` se lee de izquierda a
derecha: crea un círculo, define su trazo y lo coloca en la escena. Todas las
medidas usan unidades lógicas de un fotograma de 16×9, no píxeles.

== Instalar Gaanim

Sigue #link("/empezar/instalacion/")[Instalación] para tu sistema.
Necesitas Python 3.14. FFmpeg es opcional hasta el capítulo de exportación.

Crea el proyecto que utilizaremos durante todo el libro:

```bash
gaanim init video movimiento-circular
cd movimiento-circular
gaanim .
```

El último comando abre la previsualización. Mientras editas `main.py`, el hot
reload vuelve a construir la escena al guardar.

== La carpeta del proyecto

El scaffold contiene cuatro piezas importantes:

```text
movimiento-circular/
  gaanim.toml
  main.py
  assets/
  exports/
```

`main.py` contiene la escena. `gaanim.toml` describe el proyecto. `assets/`
guarda imágenes, SVG y fuentes. `exports/` recibe los videos y previews. Durante
los primeros capítulos solo modificaremos `main.py`.

== Cómo leer el código

Una escena tiene tres tiempos distintos:

1. Construcción: creas los objetos y describes su estado inicial.
2. Timeline: `play` y `wait` ordenan lo que ocurrirá.
3. Salida: `render`, `export` o `snapshots` decide qué producir.

No confundas crear un objeto con animarlo. `scene.geometry.circle(...)` registra un
objeto. `circle.animate.create()` devuelve una animación. `scene.play([...])` coloca esa
animación en el tiempo.

#checkpoint[
Antes de seguir, `gaanim .` debe abrir el proyecto sin errores. Si no ocurre,
usa `gaanim check .` y revisa Python, el manifiesto y la ruta de `main.py`.
]

== Siguiente capítulo

En #link("/tutorial/primera-escena/")[Primera escena] reemplazaremos el scaffold por
el primer fotograma de nuestra explicación.
]
