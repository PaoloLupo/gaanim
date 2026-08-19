#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Matrices",
  description: "Matrices seleccionables, mutables y conectadas con álgebra simbólica",
  route: "/api/matrices/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Matrices

Una matriz de Gaanim conserva cada entrada como un `Drawable`. Por eso una
celda, fila, columna, diagonal o bloque puede recibir estilo y animación sin
desarmar el layout.

#api-entry(
  name: "Scene.matrix",
  kind: "factory",
  signature: "matrix(data, *, row_gap=24, column_gap=24, delimiter_gap=12, delimiters=\"brackets\", delimiter_size=None, delimiter_weight=300, row_labels=None, column_labels=None, label_mode=\"math\", cell_mode=\"math\", entry_style=None, label_style=None, cell_factory=None, numeric_format=\"g\") -> Matrix",
  params: (
    (name: "data", type: "Sequence[Sequence[value]] | sympy.MatrixBase", default: none, desc: [Datos rectangulares no vacíos.]),
    (name: "delimiters", type: "str", default: "\"brackets\"", desc: [brackets, parentheses, braces, bars, double_bars o none.]),
    (name: "delimiter_size / delimiter_weight", type: "float / int", default: "auto / 300", desc: [Tamaño y grosor tipográfico (100–900) de los delimitadores.]),
    (name: "row_gap / column_gap", type: "float", default: "24 / 24", desc: [Separación entre tracks automáticos en unidades de escena.]),
    (name: "row_labels / column_labels", type: "Sequence | None", default: "None", desc: [Etiquetas externas alineadas con las filas o columnas.]),
    (name: "cell_mode / label_mode", type: "\"math\" | \"text\"", default: "\"math\"", desc: [Render Typst matemático o texto plano.]),
    (name: "entry_style / label_style", type: "TextStyle | None", default: "None", desc: [Estilo de entradas y etiquetas.]),
    (name: "cell_factory", type: "Callable[[value, row, column], Drawable] | None", default: "None", desc: [Constructor personalizado por celda.]),
    (name: "numeric_format", type: "str", default: "\"g\"", desc: [Formato Python para valores flotantes.]),
  ),
  returns: (type: "Matrix", desc: [Matriz respaldada por Layout v2.]),
  desc: [Los valores escalares se convierten en ecuaciones Typst; un Drawable se conserva y `cell_factory` permite crear entradas arbitrarias. Filas desiguales, labels incompatibles y factories inválidas producen errores antes del render.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(640, 360)
m = scene.matrix([[1, 2, 3], [4, 5, 6]], delimiters="parentheses")
m[0, :].fill(GOLD)
scene.play(m.entries.write(0.4, order="spiral_in", stagger=0.05))
```
]

== Selección y cambio estructural

`m[row, column]` devuelve una celda. Un slice devuelve `MatrixSelection`.
También están disponibles `row`, `column`, `block`, `diagonal`,
`anti_diagonal` y `where`. Los órdenes incorporados son `row_major`,
`column_major`, `main_diagonal`, `anti_diagonal`, `spiral_in`, `spiral_out` y
`random`; este último usa `seed` y es reproducible.

`set`, `insert_row`, `remove_row`, `insert_column`, `remove_column`,
`swap_rows`, `swap_columns`, `reorder_rows`, `reorder_columns` y `become`
actualizan la composición. `morph_to(match="auto")` empareja claves de
`MatrixEntry`, luego valores y finalmente posiciones.

== Álgebra opcional

Instala `gaanim[algebra]` para habilitar SymPy. `add`, `subtract`, `matmul`,
`hadamard`, `scale_by`, `transpose`, `determinant`, `inverse`, `rank`, `trace`,
`rref`, `lu`, `qr` y `eigen` devuelven `MatrixDerivation[T]`: `T` es `Matrix`
para resultados matriciales, `Drawable` para escalares y `tuple[Matrix, Matrix]`
para descomposiciones. `Matrix` expone en el LSP los métodos fluidos de
`Drawable`, incluido `result.at(...)`. Los fallos exactos no se
aproximan silenciosamente; usa `exact=False, precision=N` de forma explícita.

```python
# show-code: true
from gaanim import Scene
scene = Scene(640, 360)
a = scene.matrix([[1, 2], [3, 4]])
b = scene.matrix([[2, 0], [1, 2]])
derivation = a.matmul(b)
derivation.result.at(180, 0)
scene.play(derivation.animate())
```
