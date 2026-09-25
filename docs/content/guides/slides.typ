#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Presentaciones",
  description: "Crea, previsualiza y presenta diapositivas semánticas reutilizables",
  route: "/guides/slides/",
  updated: datetime.today().display(),
  code-langs: (),
)

Los proyectos `slides` son el punto de partida general para clases, charlas,
demos y presentaciones técnicas. La identidad visual pertenece al proyecto: el
starter no incluye colores, fuentes ni contenido de una institución concreta.

= Crear el proyecto

```powershell
gaanim init slides mi-charla
gaanim mi-charla
```

También puedes ejecutar `gaanim` sin argumentos, elegir *Nuevo proyecto* y
seleccionar *Slides*. El Inicio detecta Python, `.venv` y uv antes de abrir.

= Estructura semántica

```python
from gaanim import Scene, lecture, title_slide

scene = Scene(frame=(16, 9), margin=0.9)
scene.canvas.set_theme("presentation")
scene.slides.brand(footer="MI CHARLA", slide_numbers=True, rule=True)

cover = scene.segment("Portada", template=title_slide, notes="Presenta el tema.")
cover.bind(
    title=scene.text("Una idea clara", role="title"),
    subtitle=scene.text("Slides reutilizables", role="subtitle"),
)
scene.wait(0.5)
scene.stop("portada")

content = scene.segment("Contenido", template=lecture, notes="Desarrolla la idea.")
content.bind(
    title=scene.text("Contenido", role="title"),
    body=scene.text("Una sola idea por slide.", size=0.42),
)
scene.wait(0.5)
scene.stop("mensaje")
scene.render()
```

`scene.slides.brand(...)` configura logo, footer, numeración y regla para todo el deck.
Las plantillas `title_slide`, `lecture`, `comparison` y `credits` devuelven un
`Layout` raíz responsive y pueden reemplazarse por funciones Python propias.

= Presentar y validar

```powershell
gaanim check mi-charla --strict
gaanim --present --monitor 1 mi-charla
```

== Ensayar una parte

```powershell
gaanim mi-charla --sections resultados,conclusiones
gaanim --present mi-charla --from resultados
```

`--sections` reproduce solo los segmentos indicados y `--from` empieza en uno y
sigue hasta el final; combinados, `--from` recorta la lista. Cada nombre
selecciona el segmento con ese nombre exacto, todos los segmentos de una
`Section` con esa clave o el paso de una `Section` con ese nombre de
`SectionStep` (por ejemplo `--sections "Problemática · país sísmico"`), sin
distinguir mayúsculas. Un nombre desconocido se
muestra como error con las opciones disponibles, y entonces se reproduce todo.

El script se ejecuta completo: objetos persistentes, cámara y tema llegan al
primer segmento elegido exactamente como en la ejecución entera, porque la
selección *salta* a ese segmento en lugar de omitir código. La reproducción,
`Home`/`End`, `Advance`/`Previous` y los steps solo visitan lo elegido; al
terminar un tramo se salta al siguiente. Con `gaanim --diff ... --capture-stops`
las mismas opciones limitan las pausas capturadas, que conservan su numeración
global; sin `--capture-stops` no aplican, porque `scene.snapshots` elige sus
tiempos desde el script.

Los índices de monitor empiezan en cero. Presenter View utiliza los nombres y
notas de `scene.segment(...)`, y solo espera input en los puntos marcados con
`scene.stop(...)`. La exportación ignora esas paradas y genera un video continuo.

Presenter View habla en términos de diapositivas: cada `scene.segment(...)` es
una *slide* y cada `scene.stop(...)` dentro de ella un *step*, es decir, un punto
donde la reproducción espera al orador. El cockpit se lee de arriba abajo:

- *Encabezado:* estado (`PLAYING`, `PAUSED` o `END`), `Slide n of N`, la hora,
  el tiempo transcurrido con un botón `↺` discreto para reiniciarlo y una barra
  con un bloque por diapositiva, marcas de steps y clic para saltar.
- *Now on screen:* nombre de la diapositiva, `Step k of K · nombre` y una
  preview grande de lo que ve la audiencia. Si la audiencia está en negro o
  blanco, la preview lo indica; mientras se reproduce muestra un aviso.
  Debajo, los chips de steps permiten saltar a `Slide start` o a cualquier step.
- *Up Next:* el siguiente punto de pausa, indicando si es otro step de la misma
  diapositiva o la siguiente diapositiva.
- *Speaker notes:* texto claro con tamaño ajustable mediante `A−`/`A+`.
- *Dock:* primer step, step anterior, reproducir/pausa, step siguiente, último
  step, `Overview`, `Black` y `White`, el progreso de las previews y la lista de
  atajos (icono de teclado).

Usa `Right`, `Enter` o clic en la pantalla de audiencia para avanzar; `Space`
solo pausa o reanuda la animación en curso y nunca salta al siguiente step;
`Left`/`Backspace` para volver; `Home`/`End` para ir al primer o último step;
`O` para el overview, que busca por nombre de diapositiva, step o notas y salta
a la primera coincidencia con `Enter`; y `B`/`W` para apagar la audiencia en
negro o blanco. Saltar a una diapositiva muestra su estado inicial aunque la
anterior termine con un stop en el mismo instante. Si cierras Presenter View, la
presentación sigue en fullscreen y `P` vuelve a abrir el cockpit. `Esc` cierra
primero el overview o blanking activo y después sale del modo presentación.

La pantalla fullscreen muestra un dock compacto con primer step, step anterior,
reproducir/pausa, step siguiente, último step, el nombre y número de la
diapositiva y la barra de progreso al llevar el cursor a su zona inferior. El dock se oculta al retirar el cursor o cambiar el
foco al cockpit. Sus botones y los atajos pasan por las mismas acciones, por lo
que un clic en el dock no avanza dos veces.

Las previews se generan en segundo plano sin tocar la presentación en vivo: la
diapositiva actual y la siguiente se renderizan primero y el resto aparece
progresivamente. Tras un hot reload las previews anteriores siguen visibles,
marcadas como `Updating preview…`, hasta que llega su reemplazo. Se conservan
al reabrir Presenter View, se adaptan al tamaño y DPI de la ventana y solo se
regeneran al agrandarla. Si el render falla, el dock ofrece `Retry` y conserva
las previews ya generadas. Las previews dibujan las capas 2D; en escenas con 3D
nativo el dock lo advierte, y los objetos 3D siguen visibles en la audiencia.

La salida fullscreen, tanto en el editor como en Presenter Mode, ajusta el lienzo
completo al monitor sin deformarlo y rellena en negro cualquier franja exterior,
independientemente del fondo de la escena. En el fullscreen normal del editor,
`Esc` también restaura la ventana, igual que `F11`.

Para revisar una animación sin pausas en el editor, activa *Continuous* junto a
los controles de transporte. El toggle dura la sesión y sobrevive al hot reload,
pero Presenter Mode sigue respetando `scene.stop(...)`. Los seeks, snapshots y
la exportación también continúan ignorando stops como antes.

= Entorno opcional con uv

El visor embebe Gaanim y puede usar un Python compatible del sistema. Si el
proyecto no contiene `.venv`, el Inicio muestra el runtime detectado y permite
abrir igualmente. Para aislar dependencias, copia y ejecuta fuera de Gaanim:

```powershell
cd mi-charla
uv venv --python 3.12
```

Gaanim nunca ejecuta esas instrucciones ni modifica el entorno por su cuenta.
