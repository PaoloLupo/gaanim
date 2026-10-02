#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Preguntas",
  description: "Imágenes, selección múltiple y preguntas escritas en Markdown o en una hoja de cálculo",
  route: "/publico/preguntas/",
)

En esta guía verás qué clases de pregunta puedes hacer y cómo escribirlas
fuera de Python, en un archivo que cualquiera del equipo puede editar.

= Tipos de pregunta <tipos-de-pregunta>

Además de una sola respuesta, una pregunta puede llevar una imagen y aceptar
varias respuestas:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿De qué país es esta bandera?", ["Japón", "Bangladés", "Palaos"],
                  correct=1, image="assets/bandera.png")
paises = scene.quiz("¿Qué países están en Sudamérica?", ["Perú", "Chile", "México", "Brasil"],
                    correct=[0, 1, 3])          # selección múltiple
gustos = scene.poll("¿Qué te gustó?", ["Mapas", "Banderas", "El juego"], multiple=True)
```

- `image=` muestra la imagen sobre la pregunta en los teléfonos, reducida a
  1024 píxeles. En la escena la pones tú, con `scene.media.image`.
- Con `correct=[...]` el cuestionario es de selección múltiple: el teléfono
  deja marcar varias respuestas y enviarlas, y solo suma puntos quien marcó
  todas las correctas y ninguna otra.
- `multiple=True` en una encuesta deja votar por varias. `votes(i)` cuenta
  cada respuesta elegida, y `total()` y `share(i)` cuentan a los teléfonos que
  respondieron.
- `poll.icon(i, tamaño)` es la forma que tiene la respuesta `i` en el teléfono
  (triángulo, rombo, círculo, cuadrado, estrella, hexágono), con su color, y
  `poll.color(i)` el color. En un cuestionario los teléfonos ordenan y colorean
  las respuestas a su manera para que no se copien, así que ahí la forma de la
  pantalla no coincide con la del teléfono.

Una pregunta tiene de 2 a 6 respuestas de hasta 120 caracteres, y la
pregunta, hasta 300.

= Preguntas en un archivo <preguntas-en-un-archivo>

Las preguntas se pueden escribir fuera de Python, en un Markdown o en una hoja
de cálculo, y abrirlas con `scene.question`:

```markdown
# ¿De qué país es esta bandera?
![](assets/bandera.png)
tiempo: 15
acierta: 45%

- Japón
- [x] Bangladés
- Palaos

# ¿Qué países están en Sudamérica?

- [x] Perú
- [x] Chile
- México
- [x] Brasil

# ¿Qué te gustó más?
modo: varias
votos: 3, 2, 4

- Los mapas
- Las banderas
- El juego
```

Cada `#` empieza una pregunta y cada elemento de la lista es una respuesta;
`[x]` marca las correctas (varias la hacen de selección múltiple) y sin
ninguna es una encuesta. El texto antes de la primera pregunta se ignora, así
que puedes dejar ahí instrucciones para quien edite el archivo. Las líneas
`clave: valor` ajustan:

#table(
  columns: (auto, 1fr),
  [*Clave*], [*Qué ajusta*],
  [`tiempo`], [Segundos para responder un cuestionario, de 5 a 300.],
  [`puntos`], [Puntos máximos de una respuesta correcta, de 100 a 10000.],
  [`modo`], [`una` o `varias`: si una encuesta acepta varias respuestas.],
  [`imagen`], [Una imagen, como también la acepta `![](ruta)`.],
  [`acierta`], [La parte del ensayo que acierta, como `70%` o `0.7`.],
  [`votos`], [Un peso por respuesta para el ensayo, como `3, 1, 2`.],
  [`notas`], [Notas del orador para la diapositiva.],
)

En una hoja de cálculo (`.csv`) va una pregunta por fila con las columnas
`pregunta`, `respuesta 1` a `respuesta 6` y `correcta` (los números de las
correctas desde 1, como `2` o `1, 3`), y opcionalmente las mismas claves; la
plantilla de Kahoot sirve tal cual.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
from gaanim import load_questions

for q in load_questions("preguntas.md"):
    scene.segment(q.text, notes=q.notes)
    poll = scene.question(q)                  # cuestionario o encuesta
    scene.text(q.text, size=0.6).move_to(0, 3.5)
    if q.image:
        scene.media.image(q.image, width=5, height=3).move_to(4, 0)
    for i, respuesta in enumerate(q.options):
        poll.icon(i, 0.5).move_to(-6, 2 - i * 1.2)
        scene.text(respuesta, size=0.45).move_to(-3.5, 2 - i * 1.2)
    scene.stop()
    if q.is_quiz:
        poll.reveal()
        scene.wait(1)
        scene.stop()
```

`load_questions` lee la ruta relativa al script y devuelve objetos `Question`
con `text`, `options`, `correct` (los índices de las correctas, vacío en una
encuesta), `multiple`, `time`, `points`, `image`, `rehearse`, `notes` e
`is_quiz`. El diseño de cada diapositiva sigue siendo tuyo: puedes elegir un
diseño para las preguntas con imagen y otro para las demás, o intercalar una
diapositiva propia después de una pregunta concreta. Un archivo con un error
lanza `QuestionError` con el nombre del archivo y la línea.
