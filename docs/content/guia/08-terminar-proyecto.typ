#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Terminar el proyecto",
  description: "Organizar el archivo, validar, exportar y saber qué aprender después",
  route: "/guia/terminar-proyecto/",
  updated: datetime.today().display(),
  code-langs: (),
)[

= De experimento a proyecto

Una escena está terminada cuando otra persona puede abrirla, entender su
estructura y producir el mismo resultado. El último paso no es añadir más
efectos: es hacer explícitas las decisiones.

== Organizar `main.py`

Divide el archivo en cinco zonas reconocibles:

```python
# 1. Imports y paleta
# 2. Scene y configuración
# 3. Objetos estáticos
# 4. Relaciones reactivas
# 5. Timeline y salida
```

No escondas cada línea en una función. Crea funciones cuando nombren una idea
reutilizable, como `build_axes()` o `build_reactive_projection()`, y conserva la
timeline principal visible de arriba abajo.

== Elegir una salida

Durante la edición termina con:

```python
scene.render()
```

Para un video final:

```python
# output: exports/movimiento-circular.mp4
scene.render()
```

MP4 y WebM necesitan FFmpeg. Para revisar rápidamente una animación en una web
o documento, WebP suele ser más liviano.

== Capturas reproducibles

El ejemplo final acepta el directorio que inyecta el comparador visual:

```python
import os

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.0, 3.0, 6.0, 9.0])
else:
    scene.render()
```

La misma escena puede servir al editor y a pruebas visuales sin mantener dos
archivos distintos.

== Comprobar antes de exportar

Desde la raíz del proyecto:

```powershell
gaanim check .
gaanim .
```

Revisa la escena completa y también instantes intermedios. Busca texto fuera
del área segura, objetos que aparezcan antes de su entrada, relaciones que se
rompan al hacer seek y pausas demasiado cortas para leer.

#idea[
Una buena animación no es la que contiene más métodos de la API. Es la que
mantiene una relación clara entre lo que se ve, el orden en que se revela y la
idea que debe comprender el espectador.
]

== Qué aprendiste realmente

El proyecto recorrió las capas fundamentales de Gaanim:

- `Scene` definió el espacio y la timeline.
- Los drawables expresaron geometría, texto y estilo.
- `play`, `wait`, duración y easing construyeron el ritmo.
- Layout organizó contenido que depende de su medida.
- Los updaters describieron movimiento continuo.
- Bindings y geometría reactiva conectaron objetos en el mismo frame.
- `traced_path` convirtió movimiento en información persistente.
- Render, export y snapshots produjeron salidas distintas desde la misma escena.

== Cómo continuar

No leas la referencia API de principio a fin. Úsala cuando tengas una pregunta
concreta. Para ampliar este proyecto, prueba en este orden:

1. Cambia el radio y la velocidad angular.
2. Añade una segunda curva con otra fase.
3. Sustituye colores locales por un `Theme`.
4. Adapta la composición a un viewport 9:16.
5. Convierte la explicación en segmentos de una presentación.

#checkpoint[
El archivo `examples/manual_movimiento_circular.py` debe ejecutarse con
`gaanim examples/manual_movimiento_circular.py` o mediante el launcher del
proyecto. Conserva una copia antes de experimentar con extensiones.
]

== Después de la guía

La parte siguiente abre un taller de escenas con recetas de texto estructurado,
datos, Layout, reactividad, 3D, proyectos, presentaciones y regresión visual.
La referencia técnica de la API queda reservada para el apéndice final.
]
