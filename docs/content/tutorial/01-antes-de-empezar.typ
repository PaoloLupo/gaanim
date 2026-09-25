#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Antes de empezar",
  description: "Qué vamos a construir, cómo está organizado el tutorial y cómo preparar el proyecto",
  route: "/tutorial/antes-de-empezar/",
)[

= Qué vamos a construir

En este tutorial construirás, paso a paso, una animación de diez segundos que
explica una idea matemática: cómo un punto que gira sobre un círculo dibuja una
onda seno.

Al terminar, la escena mostrará un título, un círculo con su radio y un punto
que da una vuelta completa. A la derecha, una recta numerada de $0$ a $2 pi$ y
una onda que se dibuja al mismo ritmo, unida al punto por una línea de
proyección. Todo el movimiento depende de un único valor: el ángulo `theta`.

Cada capítulo añade una idea de Gaanim a la misma escena:

+ *Primera escena*: el lienzo, las coordenadas y los primeros objetos.
+ *Objetos y estilo*: una paleta con propósito, trazos, texto y orden de dibujo.
+ *Animar el tiempo*: `scene.play`, duraciones, `stagger` y easing.
+ *Componer y explicar*: una fórmula y un panel organizado con Layout.
+ *Dar vida a la escena*: un parámetro animable que mueve el punto y el radio.
+ *Del círculo a la onda*: una recta numerada, la curva seno y la proyección.
+ *Terminar el proyecto*: un cierre, la revisión y la exportación.

= Cómo leer cada capítulo

Todos los capítulos siguen el mismo orden:

+ *Objetivo*: qué verás al terminar el capítulo.
+ *Cambios*: fragmentos de código con la explicación de cada uno. Un comentario
  como `# Círculo` indica en qué zona de `main.py` va cada fragmento.
+ *Archivo completo*: el `main.py` entero en ese punto, con una vista previa de
  lo que debe mostrar. Si algo no coincide, compara tu archivo con este.

Cada archivo completo se ejecuta al construir esta documentación, así que
siempre funciona tal como aparece.

#idea[
No copies todos los fragmentos de golpe. Aplica un cambio, guarda y mira el
resultado en el editor. Ver cómo reacciona la escena a cada línea enseña más que
leer el código terminado.
]

= Lo que necesitas saber

El tutorial supone Python básico: imports, variables, tuplas, funciones y
listas. No necesitas saber Rust, Bevy ni programación de GPU.

Gaanim usa una API fluida: cada llamada devuelve el mismo objeto, así que
puedes encadenarlas. Esta expresión crea un círculo, le da un trazo y lo coloca
en la escena, de izquierda a derecha:

```python
>>>from gaanim import WHITE, Scene
>>>scene = Scene(frame=(16, 9))
circle = scene.geometry.circle(1.5).stroke(WHITE, 0.05).move_to(-4, 0)
```

Todas las medidas usan unidades lógicas de un fotograma de 16 × 9, nunca
píxeles.

= Preparar el proyecto

Si todavía no tienes Gaanim, sigue primero
#link("/empezar/instalacion/")[Instalación]. Después crea el proyecto que
usaremos durante todo el tutorial y ábrelo:

```bash
gaanim init video del-circulo-al-seno
cd del-circulo-al-seno
gaanim .
```

`gaanim .` abre el editor con la escena de ejemplo del proyecto.
Mientras editas `main.py`, el editor vuelve a ejecutar la escena cada vez que
guardas.

Estas son las piezas del proyecto que importan en el tutorial:

```text
del-circulo-al-seno/
  gaanim.toml   # nombre del proyecto y archivo de entrada
  main.py       # la escena: el único archivo que editaremos
  assets/       # imágenes, SVG y fuentes
  exports/      # videos exportados
```

El scaffold crea además un `README.md`, un `pyproject.toml` para el entorno de
Python y un `AGENTS.md` para asistentes de código.

#checkpoint[
El editor muestra la escena de ejemplo del proyecto sin errores. Si no se abre,
ejecuta `gaanim check .` desde la carpeta del proyecto para ver qué falla y revisa
#link("/empezar/instalacion/")[Instalación].
]

= El archivo final

El resultado de este tutorial también se instala con Gaanim: está en la carpeta
del paquete `gaanim`, en `_docs/tutorial/circulo_al_seno.py`. Úsalo para
comparar si te pierdes, pero intenta llegar a él por tu cuenta.

En el siguiente capítulo, #link("/tutorial/primera-escena/")[Primera escena],
sustituirás el ejemplo por el primer fotograma de la explicación.
]
