#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Terminar el proyecto",
  description: "Un cierre para la animación, el archivo final, la revisión y la exportación",
  route: "/tutorial/terminar-proyecto/",
)[

= Objetivo

La explicación ya funciona. En este capítulo le damos un final: cuando el
punto completa la vuelta, la proyección se retira y la escena se queda quieta
un momento con el círculo y la onda. Después comprobaremos el proyecto y lo
exportaremos a video.

= Cambios

== Describir el archivo

Añade un docstring al principio de `main.py`. Es lo primero que leerá quien
abra el archivo, incluido tú dentro de unos meses:

```python
"""Del círculo al seno: el proyecto final del tutorial de Gaanim.

Un punto gira sobre un círculo y su altura dibuja una onda seno. Un único
parámetro, ``theta``, gobierna el punto, el radio, la onda y su proyección.
"""
>>># Contexto mínimo para validar los fragmentos de esta página.
>>>from gaanim import Scene
>>>scene = Scene(frame=(16, 9))
>>>projection = scene.geometry.line((0, 0), (1, 0))
```

== Un cierre que respira

Al final de la zona `# Línea de tiempo`, sustituye `scene.wait(1)` por:

```python
# continue
scene.play([projection.animate.fade_out().duration(0.5)])
scene.wait(1.5)
```

La línea de proyección era un andamio para explicar la relación entre los dos
puntos. Al terminar la vuelta queda superpuesta a la recta, así que la
retiramos con `fade_out`. La pausa final de 1.5 segundos da tiempo a mirar el
resultado antes de que el video termine: una exportación acaba exactamente en
el último instante de la línea de tiempo.

= Archivo completo

Este es el programa terminado. Es el mismo archivo que se instala con Gaanim en
`_docs/tutorial/circulo_al_seno.py`, dentro de la carpeta del paquete `gaanim`:

```python
# cell: circulo_al_seno
# output: preview.webp
# timeout: 300
```

#checkpoint[
La vista previa dura 10 segundos. Hasta el segundo 8 es igual que la del
capítulo anterior; entre el 8 y el 8.5 la línea de proyección se desvanece y
en los últimos 1.5 segundos quedan el círculo con su radio, la recta y la onda
completa, con los dos puntos amarillos a la altura de la recta.
]

= Comprobar el proyecto

Desde la carpeta del proyecto:

```bash
gaanim check .
```

`gaanim check` ejecuta la escena sin abrir ventana e informa de su duración, en
este caso 10 segundos. Si algo falla, muestra el error de Python con su línea.
Con `--strict` también falla ante las advertencias.

Después abre la escena con `gaanim .` y revísala entera, también a saltos con
la barra de reproducción:

- Todo el texto queda dentro del margen de seguridad.
- Ningún objeto aparece antes de su animación de entrada.
- El radio, la proyección y la onda siguen al punto en cualquier instante.
- Las pausas son lo bastante largas para leer la fórmula.

= Exportar

Cuando la escena esté lista, expórtala sin tocar el script:

```bash
gaanim export . --output exports/del-circulo-al-seno.mp4
```

La extensión elige el formato: `.mp4` y `.webm` para video, `.webp` y `.gif`
para animaciones ligeras en una web o un documento, y `.png` para una
secuencia de imágenes. Todos salvo PNG necesitan FFmpeg. `--quality` elige
entre `draft` (rápido), `standard` (el valor por defecto) y `production`, y
`--from` y `--to` exportan solo un tramo:

```bash
gaanim export . --output exports/vuelta.webp --from 4 --to 8
```

La referencia completa de estas opciones está en
#link("/referencia/cli/")[Línea de comandos].

#idea[
Una buena animación no es la que usa más funciones de la API. Es la que
mantiene una relación clara entre lo que se ve, el orden en que se revela y la
idea que debe entender el espectador.
]

= Lo que has aprendido

- `Scene` define un lienzo de unidades lógicas, con el origen en el centro y un
  margen de seguridad.
- `scene.text` y `scene.geometry` crean objetos y devuelven handles; `fill`,
  `stroke`, `no_fill` y `z_index` les dan estilo y orden.
- `.animate`, `scene.play`, `stagger`, `duration`, `Easing` y `scene.wait`
  construyen la línea de tiempo.
- `scene.layout.column` organiza contenido que depende de su medida y pasa a
  ser el dueño de la posición de sus hijos.
- Un `Parameter` animado con `theta.animate.set(...)` gobierna todo lo que
  depende de él: `polar_point`, `follow`, líneas con objetos como extremos y
  valores `computed`.
- `Axis`, `scene.viz.number_line`, `number_line.function(..., reveal=theta)` y
  `number_line.point_ref` convierten el ángulo en una curva sin deriva.
- `gaanim check` y `gaanim export` validan y producen la salida sin cambiar el
  script.

= Cómo continuar

Modifica el proyecto antes de empezar otro. Guarda una copia y prueba, en este
orden:

+ Cambia `R` a `1.2`: el círculo, el punto, la onda y la proyección se adaptan.
+ Da dos vueltas: lleva el eje y la animación de `theta` hasta `4 * math.pi`.
+ Añade una segunda curva con `number_line.function(math.cos, ...)` y otro color.
+ Anima el panel con otras entradas y easings de
  #link("/guias/movimiento/")[Movimiento].

Para ir más lejos, las guías resuelven tareas concretas:
#link("/guias/layout/")[Layout] para composiciones más ricas,
#link("/guias/reactividad/")[Reactividad] para rastros, valores calculados y
simulaciones, #link("/guias/presentaciones/")[Presentaciones] para convertir la
escena en diapositivas y #link("/guias/capturas-y-comparacion/")[Capturas y
comparación] para comprobar que un cambio no altera lo que se ve. Consulta la
#link("/referencia/")[Referencia] cuando tengas una pregunta concreta sobre una
función.
]
