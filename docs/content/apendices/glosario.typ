#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Glosario",
  description: "Los términos que usa esta documentación y el nombre que tienen en la API",
  route: "/apendices/glosario/",
)

Esta documentación explica en español y conserva en inglés los nombres de la
API. Aquí tienes cada término con el identificador que le corresponde en el
código.

/ Ancla (`Anchor`, `anchor_point`): uno de los nueve puntos de referencia de
  la caja de un objeto (`Anchor.CENTER`, `Anchor.TOP_LEFT`…). Sirve para
  colocar objetos (`move_to(x, y, anchor=...)`) y, con
  `objeto.anchor_point(...)`, como extremo que acompaña al objeto cuando se
  mueve.

/ Animación (`Anim`): la descripción de un cambio en el tiempo, como
  `circle.animate.move_to(2, 0)` con `.duration(1.0)`. No hace nada hasta que la
  pasas a `scene.play([...])`.

/ `.animate`: propiedad de solo lectura de cada handle que ofrece los mismos
  métodos que los cambios inmediatos, pero devuelve animaciones.

/ Área segura (`within="safe"`): el fotograma menos el margen de la escena
  (`Scene(margin=...)`). Layout la usa para no pegar el contenido a los bordes.

/ Binding (vínculo): relación por la que una propiedad copia la de otro
  objeto, como `bind_x_from`, `bind_rotation_from` o `follow`. Ver
  #link("/guias/reactividad/")[Reactividad].

/ Capacidad: cada grupo de fábricas de una escena: `scene.geometry`,
  `scene.text`, `scene.layout`, `scene.media`, `scene.viz`, `scene.slides`,
  `scene.mechanics`, `scene.assets`, `scene.canvas` y `scene.camera`.

/ Captura (`scene.snapshots`): imagen exacta de un instante de la escena, que
  `gaanim --diff` compara con una versión aprobada. Ver
  #link("/guias/capturas-y-comparacion/")[Capturas y comparación visual].

/ Composición (`Composition`): grupo de animaciones ordenadas con
  `parallel`, `sequence` o `stagger`, que se reproduce como una unidad.

/ Corte: cambio inmediato, sin duración, que un método aplicado fuera de
  `.animate` registra en el cursor actual (por ejemplo `circle.fill(RED)`
  después de un `play`). Es reversible: al volver atrás en la línea de tiempo,
  el objeto recupera su estado anterior.

/ Cursor (`scene.cursor`): el instante de la línea de tiempo en el que se
  añade lo siguiente que escribes. Avanza con cada `scene.play` y
  `scene.wait`.

/ Drawable: cualquier objeto visible de la escena (una figura, un texto, una
  imagen, un gráfico, un layout). En español decimos también *objeto*.

/ Easing (`Easing`): la curva que reparte el progreso de una animación en el
  tiempo: `Easing.LINEAR`, `Easing.SMOOTH`, `Easing.spring(...)`… Se aplica
  con `.easing(...)`.

/ Escena (`Scene`): el documento que describes en Python. Contiene los
  objetos, la línea de tiempo, el tema y la cámara, y termina con
  `scene.render()`.

/ Exportar (`gaanim export`): renderizar la escena a un archivo de video,
  imagen animada o secuencia PNG. Ver
  #link("/guias/proyectos/")[Proyectos y exportación].

/ Fábrica: método que crea un drawable y devuelve su handle, como
  `scene.geometry.circle(1)` o `scene.text("Hola")`.

/ Fragmento: parte de un texto seleccionada por su contenido, como
  `text["energía"]`, para colorearla o animarla por separado. Si el texto
  tiene una parte con ese nombre, se usa la parte.

/ Handle: el valor de Python que devuelve una fábrica y con el que manipulas
  el objeto: `circle = scene.geometry.circle(1)`. Sus métodos son encadenables:
  `circle.fill(BLUE).move_to(2, 0)`.

/ Layout: sistema que coloca objetos en filas, columnas, rejillas y capas sin
  coordenadas manuales (`scene.layout`). Ver #link("/guias/layout/")[Layout].

/ Línea de tiempo: la secuencia de todo lo que ocurre en la escena, con su
  instante exacto. Gaanim puede saltar a cualquier punto de ella y mostrar lo
  mismo que al reproducir.

/ Manifiesto (`gaanim.toml`): archivo que define un proyecto: nombre, tipo,
  script de entrada y carpetas de recursos y exportación.

/ Marcador (`scene.marker`): nombre de un instante de la línea de tiempo.
  El editor lo muestra en la barra de reproducción y `gaanim export --from/--to`
  lo acepta en lugar de segundos.

/ Parada (`scene.stop`): punto donde el modo presentación espera a que
  avances. La vista previa, las capturas y la exportación no se detienen en
  ella. En la vista del presentador cada parada es un *paso*.

/ Parámetro (`Parameter`, `scene.viz.parameter`): valor numérico invisible y
  animable del que pueden depender otros objetos.

/ Parte (`part`): fragmento de un texto o una ecuación con nombre propio,
  como `part("masa", "m")`, que puedes seleccionar con `text["masa"]` y que
  las transformaciones emparejan por nombre.

/ Presenter View (vista del presentador): ventana con la diapositiva actual,
  la siguiente, las notas y el tiempo, que se abre con `gaanim --present`. Ver
  #link("/guias/presentaciones/")[Presentaciones].

/ Proyecto: carpeta con un `gaanim.toml`, un script de entrada, recursos y
  exportaciones. Se crea con `gaanim init video` o `gaanim init slides`.

/ Punto de referencia (`PointRef`): punto lógico, sin dibujo, que otros
  objetos pueden seguir o usar como extremo; lo crean `polar_point`,
  `point_ref`, `offset_point` y `point_between`.

/ Reactivo: se dice del objeto o valor que se recalcula a partir de otros en
  cada fotograma, como una `tracking_line` o un `computed`.

/ Recarga en caliente: la vista previa vuelve a ejecutar el script al guardar
  y conserva lo que no cambió.

/ Rol de texto (`role=`): estilo con nombre que el tema define para un texto:
  `"title"`, `"subtitle"`, `"body"`, `"caption"`…

/ Segmento (`scene.segment`): tramo con nombre de la línea de tiempo. En una
  presentación, cada segmento es una diapositiva; entre segmentos puede haber
  una transición.

/ Stagger (`stagger`): composición que arranca varias animaciones con un
  desfase entre ellas, por orden o por distancia a un origen.

/ Tema (`theme=`, `Theme`): conjunto de colores, tipografías y estilos de
  texto que la escena aplica por defecto, como `"presentation"` o
  `"technical"`.

/ Transición (`Transition`): el paso visual de un segmento al siguiente,
  como `Transition.cross_fade(0.4)` o `Transition.wipe(...)`.

/ Unidades lógicas: la medida de toda la escena. El fotograma mide 16 × 9
  unidades con el origen en el centro; los píxeles solo se eligen al
  previsualizar o exportar.

/ Updater (`Updater`, `add_updater`): comportamiento continuo añadido a un
  objeto, como `Updater.rotate(1.5)` o `Updater.wiggle(...)`. Ver
  #link("/guias/reactividad/")[Reactividad].

/ Valor calculado (`computed`): número derivado de parámetros, variables o
  del tiempo mediante una función de Python pura con entradas explícitas.

/ Versión aprobada (*baseline*): el conjunto de capturas aceptadas contra el
  que `gaanim --diff` compara. Se actualiza con `--bless`.

/ Vista previa: la ventana que abre `gaanim archivo.py` o `gaanim proyecto`,
  con reproducción, saltos y recarga en caliente.
