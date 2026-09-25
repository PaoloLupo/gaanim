#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Matrices",
  description: "scene.viz.matrix: matrices seleccionables y mutables, con morph entre estados y álgebra simbólica opcional",
  route: "/referencia/matrices/",
  nav: "Matrices",
)

= Matrices

`scene.viz.matrix(...)` crea una matriz cuyas entradas son objetos
independientes. Una celda, fila, columna, diagonal o bloque puede recibir estilo
y animación sin desarmar la composición, y la matriz puede cambiar de forma o
transformarse en otra.

```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
m = scene.viz.matrix([[1, 2, 3], [4, 5, 6]], delimiters="parentheses")
m[0, :].fill(GOLD)
scene.play(m.entries.animate.write(order="spiral_in", stagger=0.05).duration(0.4))
# output: preview.webp
scene.render()
```

`Matrix` y sus tipos auxiliares (`MatrixSelection`, `MatrixEntry`…) se importan
desde `gaanim` y se definen en su módulo Python, por eso las fichas de esta
página los nombran por la variable (`matrix.row`) y no por la clase. `Matrix`
es un `Drawable`: se coloca, escala y anima como cualquier objeto, y los métodos
heredados devuelven la propia matriz.

== Crear una matriz

La fábrica y los datos que acepta.

#api-entry(
  name: "Visualization.matrix",
  kind: "factory",
  params: (
    (name: "data", type: "Sequence[Sequence[valor]] | sympy.MatrixBase", default: none, desc: [Datos rectangulares no vacíos.]),
    (name: "row_gap / column_gap", type: "float", default: "0.24 / 0.24", desc: [Separación entre filas y columnas, en unidades de escena.]),
    (name: "delimiter_gap", type: "float", default: "0.12", desc: [Espacio entre las entradas y los delimitadores.]),
    (name: "delimiters", type: "str", default: "\"brackets\"", desc: [`brackets`, `parentheses`, `braces`, `bars`, `double_bars` o `none`.]),
    (name: "delimiter_size / delimiter_weight", type: "float | None / int", default: "None / 300", desc: [Tamaño (automático) y grosor tipográfico, de 100 a 900.]),
    (name: "row_labels / column_labels", type: "Sequence | None", default: "None", desc: [Etiquetas externas alineadas con filas o columnas.]),
    (name: "cell_mode / label_mode", type: "\"math\" | \"text\"", default: "\"math\"", desc: [Entradas y etiquetas como matemática Typst o texto plano.]),
    (name: "entry_style / label_style", type: "TextStyle | None", default: "None", desc: [Estilo de entradas y etiquetas.]),
    (name: "cell_factory", type: "Callable[[valor, fila, columna], Drawable] | None", default: "None", desc: [Crea un objeto propio para cada celda.]),
    (name: "numeric_format", type: "str", default: "\"g\"", desc: [Formato de Python para los números decimales.]),
  ),
  desc: [Los escalares se convierten en ecuaciones Typst; un `Drawable` se conserva tal cual. Filas desiguales, etiquetas incompatibles, modos, grosores o delimitadores inválidos lanzan `ValueError` o `TypeError` antes del render.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
costs = scene.viz.matrix(
    [[12.5, 8], [3, 40.25]],
    row_labels=["A", "B"], column_labels=["ida", "vuelta"],
    label_mode="text", numeric_format=".1f", delimiters="brackets",
)
```
]

#api-entry(
  name: "MatrixEntry",
  kind: "class",
  signature: "MatrixEntry(value, key=None, style=None)",
  desc: [Valor de una celda con una clave estable opcional (`key`) para emparejarlo en `morph_to` y un estilo de texto local. Los predicados de `where` y `MatrixSelection.values` reciben entradas de este tipo; el número está en `.value`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
m = scene.viz.matrix([[MatrixEntry("a", key="a"), 0], [0, MatrixEntry("b", key="b")]])
```
]

#api-entry(
  name: "matrix.shape / nrows / ncols",
  kind: "property",
  signature: "shape: tuple[int, int] · nrows: int · ncols: int · entries: MatrixSelection · row_labels · column_labels · drawable",
  desc: [Dimensiones, todas las entradas como selección, las etiquetas como tuplas de `Drawable` y el objeto raíz de la composición.],
  none,
)

== Seleccionar entradas

Grupos estables de celdas que se estilizan y animan juntos.

#api-entry(
  name: "matrix[fila, columna]",
  kind: "method",
  signature: "matrix[r, c] -> Drawable · matrix[filas, columnas] -> MatrixSelection",
  desc: [Con dos enteros devuelve la celda; con _slices_ o listas de índices, una `MatrixSelection`. Admite índices negativos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
m = scene.viz.matrix([[1, 2, 3], [4, 5, 6], [7, 8, 9]])
m[1, 1].fill(RED)
m[:, 0].fill(GOLD)
```
]

#api-entry(
  name: "matrix.row / column / block",
  kind: "method",
  signature: "row(index) · column(index) · block(rows, columns) -> MatrixSelection",
  desc: [Una fila, una columna o un bloque; `rows` y `columns` aceptan un índice, un _slice_ o una lista.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
m = scene.viz.matrix([[1, 2, 3], [4, 5, 6], [7, 8, 9]])
m.block(slice(0, 2), [1, 2]).fill(BLUE)
scene.play(m.row(2).animate.indicate(order="row_major", stagger=0.1))
```
]

#api-entry(
  name: "matrix.diagonal / anti_diagonal",
  kind: "method",
  signature: "diagonal(offset=0) · anti_diagonal(offset=0) -> MatrixSelection",
  desc: [La diagonal principal o la secundaria, desplazada `offset` columnas.],
  none,
)

#api-entry(
  name: "matrix.where",
  kind: "method",
  signature: "where(mask_or_predicate) -> MatrixSelection",
  desc: [Selecciona con una máscara rectangular de booleanos o con un predicado `f(entrada, fila, columna)`, donde `entrada` es un `MatrixEntry`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
m = scene.viz.matrix([[1, 2, 3], [4, 5, 6], [7, 8, 9]])
m.where(lambda entry, row, column: entry.value % 2 == 0).fill(GOLD)
```
]

#api-entry(
  name: "MatrixSelection",
  kind: "class",
  signature: "fill(color) · opacity(value) · offset(dx, dy) · coordinates · values · animate",
  desc: [Secuencia estable de celdas. `fill` y `opacity` cambian su estilo al instante, `offset` desplaza las celdas sin salir del layout, `coordinates` y `values` devuelven sus posiciones y entradas, y `animate` da un `MatrixSelectionAnimation`.],
  none,
)

#api-entry(
  name: "MatrixSelectionAnimation",
  kind: "class",
  signature: "write · create · fade_in · fade_out · indicate · wiggle · fill · opacity · scale_by · rotate_by (order=…, stagger=…, seed=0) · duration · easing",
  desc: [Animación de todas las celdas de una selección. `order` recorre las celdas en `simultaneous`, `row_major`, `column_major`, `main_diagonal`, `anti_diagonal`, `spiral_in`, `spiral_out` o `random` (reproducible con `seed`), o en el orden de una lista de coordenadas o de una función `(fila, columna) -> clave`. `stagger` es el retraso entre celdas. Se pasa directamente a `scene.play`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
m = scene.viz.matrix([[1, 2, 3], [4, 5, 6], [7, 8, 9]])
scene.play(m.entries.animate.fade_in(order="main_diagonal", stagger=0.08).duration(0.8))
scene.play(m.diagonal().animate.fill(GOLD).duration(0.4))
```
]

== Cambiar la estructura

Cambios de valores, filas y columnas que recomponen la matriz en el cursor.

#api-entry(
  name: "matrix.set",
  kind: "method",
  signature: "set(row, column, value) -> Drawable",
  desc: [Sustituye el valor de una celda y devuelve su nuevo objeto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
m = scene.viz.matrix([[1, 2], [3, 4]])
scene.wait(0.5)
m.set(0, 1, 7).fill(GOLD)
```
]

#api-entry(
  name: "matrix.insert_row / remove_row / insert_column / remove_column",
  kind: "method",
  signature: "insert_row(index, values) · remove_row(index) · insert_column(index, values) · remove_column(index) -> Matrix",
  desc: [Añade o quita filas y columnas; los valores nuevos deben tener la longitud correcta.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
m = scene.viz.matrix([[1, 2], [3, 4]])
m.insert_row(1, [0, 0]).remove_column(0)
```
]

#api-entry(
  name: "matrix.swap_rows / swap_columns / reorder_rows / reorder_columns",
  kind: "method",
  signature: "swap_rows(first, second) · swap_columns(first, second) · reorder_rows(order) · reorder_columns(order) -> Matrix",
  desc: [Intercambia o permuta filas y columnas; `order` es una permutación completa de índices.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
m = scene.viz.matrix([[1, 2, 3], [4, 5, 6], [7, 8, 9]])
m.swap_rows(0, 2)
m.reorder_columns([2, 0, 1])
```
]

#api-entry(
  name: "matrix.become",
  kind: "method",
  signature: "become(data) -> Matrix",
  desc: [Sustituye todos los datos, incluso con otra forma.],
  none,
)

#api-entry(
  name: "matrix.morph_to",
  kind: "method",
  signature: "morph_to(target: Matrix, *, match=\"auto\", stagger=0.0) -> MatrixSelectionAnimation",
  desc: [Transforma la matriz en otra. Con `match="auto"` empareja primero por las claves de `MatrixEntry`, después por valores y por último por posiciones. Configura el tiempo con `.duration(...)` y `.easing(...)`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
a = scene.viz.matrix([[1, 2], [3, 4]]).move_to(-2.5, 0)
b = scene.viz.matrix([[1, 3], [2, 4]]).move_to(2.5, 0)
scene.play(a.entries.animate.write(stagger=0.05).duration(0.5))
scene.play(a.morph_to(b).duration(1.0))
# output: preview.webp
scene.render()
```
]

== Álgebra simbólica

Operaciones exactas con SymPy que devuelven el resultado y los pasos animables.

Instala el extra `gaanim[algebra]` para usarlas. `add`, `subtract`, `matmul`,
`hadamard`, `scalar_multiply`, `transpose`, `determinant`, `inverse`, `rank`,
`trace`, `rref`, `lu`, `qr` y `eigen` aceptan `exact=True, precision=None` y
devuelven una `MatrixDerivation[T]`: `T` es `Matrix` para resultados
matriciales, `Drawable` para escalares y `tuple[Matrix, Matrix]` para
descomposiciones. Los fallos exactos no se aproximan en silencio: pasa
`exact=False, precision=N` de forma explícita. `to_sympy()` devuelve la matriz
de SymPy.

#api-entry(
  name: "MatrixDerivation",
  kind: "class",
  signature: "value · result · steps · animations(*, order=\"row_major\", stagger=0.04)",
  desc: [Resultado de una operación: `value` es el valor exacto de SymPy, `result` el objeto visual y `steps` los pasos (`MatrixStep`, con `kind`, `sources`, `target` y `expression`). `animations()` devuelve una `MatrixSelectionAnimation` que recorre los pasos. Un error de álgebra lanza `MatrixAlgebraError`, subclase de `ValueError`.],
)[
```python
# no-run: requiere el extra opcional gaanim[algebra] (SymPy), que el build de la documentación no instala
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
a = scene.viz.matrix([[1, 2], [3, 4]]).move_to(-3, 0)
b = scene.viz.matrix([[2, 0], [1, 2]])
product = a.matmul(b)
product.result.move_to(3, 0)
scene.play(product.animations().duration(0.7))
scene.render()
```
]
