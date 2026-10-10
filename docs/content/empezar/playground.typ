#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "El playground",
  description: "Escribe y ejecuta escenas de Gaanim en el navegador, sin instalar nada: proyectos, recursos, paquetes, vídeo y publicación",
  route: "/empezar/playground/",
  nav: "Playground",
)

El #link("https://paololupo.github.io/gaanim/playground/")[playground] es
Gaanim en el navegador. Escribes Python en un editor con autocompletado, lo
ejecutas y ves la escena al lado, sin instalar Python ni Gaanim. Es la forma
más rápida de probar una idea, de seguir el tutorial o de enseñar una
animación a alguien que no tiene la aplicación.

Todo ocurre en tu navegador: Python corre en
#link("https://pyodide.org")[Pyodide] (Python 3.14 compilado a WebAssembly) y
la escena se dibuja con WebGPU. Tus proyectos se guardan en el navegador y no
se envían a ningún servidor.

#figure(
  image("../../assets/playground/interfaz.webp", alt: "El playground: el explorador del proyecto a la izquierda, el editor en el centro, la escena y la consola a la derecha"),
  caption: [El explorador del proyecto, el editor, la escena grabada y la consola.],
)

= La interfaz

La barra de arriba tiene, de izquierda a derecha:

#table(
  columns: 2,
  [*Control*], [*Qué hace*],
  [*Ejecutar*], [Ejecuta `main.py` y graba la escena. `Ctrl+S` guarda y ejecuta; `Ctrl+Enter` ejecuta.],
  [Rayo (*En vivo*)], [Ejecuta la escena cada vez que dejas de escribir, si el código no tiene errores de sintaxis.],
  [*fps*], [Fotogramas por segundo de la grabación: 24, 30 o 60.],
  [Enlace (*Compartir*)], [Copia un enlace con los archivos de texto del proyecto.],
  [Flecha (*Descargar*)], [Descarga un vídeo o GIF, la grabación `.gaanim` o el proyecto en un `.zip`.],
  [Sol o luna], [Cambia entre el tema claro y el oscuro.],
  [`</>` (*Insertar*)], [Prepara la escena para publicarla en un blog o un sitio web.],
)

Debajo están el *explorador* del proyecto, el *editor* con una pestaña por
archivo abierto, el *reproductor*, que muestra la última grabación, y la
*consola*, con lo que imprime `print` y los errores. Los separadores entre
paneles se arrastran, y el botón de la esquina del editor oculta el
explorador. En una pantalla estrecha, el explorador se abre por encima del
editor.

= Proyectos

En el playground cada trabajo es un _proyecto_: una carpeta con `main.py`, que
es el archivo que se ejecuta, y los módulos, imágenes, audio, fuentes o datos
que quieras. El explorador funciona como el de un editor de código:

- Los botones de su cabecera crean un archivo o una carpeta y suben archivos
  o carpetas enteras. También puedes arrastrarlos desde tu escritorio al
  explorador, o soltarlos en cualquier parte de la página.
- Arrastra un archivo o una carpeta sobre otra carpeta para moverlo.
- El menú contextual (clic derecho) abre, renombra (`F2`), descarga o elimina
  (`Supr`). Eliminar se puede deshacer durante unos segundos.

El nombre del proyecto, arriba del explorador, abre el menú de proyectos:

#figure(
  image("../../assets/playground/proyectos.webp", alt: "El menú de proyectos, abierto desde el nombre del proyecto"),
  caption: [Cambiar de proyecto, crear uno, abrir un `.zip`, duplicar, renombrar, descargar o eliminar.],
)

- *Nuevo proyecto* empieza con un `main.py` de ejemplo.
- *Nuevo desde un ejemplo…* abre una escena de la galería como proyecto
  propio, sin tocar los que ya tienes.
- *Abrir un .zip…* convierte un archivo ZIP en un proyecto: uno descargado del
  playground o la carpeta comprimida de un proyecto de la aplicación. Si todo
  está dentro de una carpeta, su contenido es el proyecto.
- *Duplicar*, *Renombrar…* y *Descargar .zip* hacen lo que dicen. *Eliminar
  este proyecto* también se puede deshacer.

Los proyectos se guardan en el almacenamiento de tu navegador: siguen ahí
cuando vuelves, pero solo en ese navegador y ese equipo. Si borras los datos
del sitio, se pierden; descarga el `.zip` de los que quieras conservar.

= Módulos, imágenes, audio y fuentes <recursos>

`main.py` importa los módulos del proyecto como en tu equipo: con
`lib/formas.py`, escribe `from lib.formas import fila_de_cuadros`. Cada
ejecución los vuelve a leer, así que un cambio en un módulo se ve en la
siguiente.

Las rutas de los recursos son relativas a `main.py`, igual que en la
aplicación: `scene.media.image("assets/imagenes/logo.png")` o
`scene.media.audio("assets/audio/tema.wav")`. Al abrir una imagen, un audio o
una fuente en el explorador, el editor la muestra y propone el código que la
usa.

El navegador solo trae tres fuentes: New Computer Modern, Libertinus Serif y
DejaVu Sans Mono. Para usar otra, sube su archivo `.ttf` u `.otf` a una
carpeta del proyecto y nómbrala en el tema. La vista previa de la fuente dice
el nombre de familia que hay que escribir:

#figure(
  image("../../assets/playground/fuente.webp", alt: "Una fuente subida a assets/fonts, con su muestra, su familia y el código del tema"),
  caption: [Una fuente en `assets/fonts`, con su familia y el código que la usa.],
)

```python
# no-run: usa una fuente subida al proyecto del playground
from gaanim import Scene, Theme

scene = Scene(theme=Theme("technical", font_dir="assets/fonts", fonts={"text": "Victor Mono"}))
scene.text("Con mi fuente")
scene.render()
```

Si una escena nombra una fuente que el navegador no tiene, la consola lo
advierte y explica cómo añadirla. Los vídeos como recurso
(`scene.media.video`) necesitan FFmpeg y solo funcionan en la aplicación.

= Paquetes de Python <paquetes>

La pestaña *Paquetes*, junto a la consola, añade paquetes al proyecto. Escribe
un nombre y elige entre las sugerencias: los paquetes que
#link("https://pyodide.org/en/stable/usage/packages-in-pyodide.html")[Pyodide
compila para el navegador] (NumPy, SymPy, SciPy, pandas, NetworkX…) y el
paquete con ese nombre en PyPI. *Añadir* (o `Enter`) lo escribe en el proyecto
y lo instala en el momento, sin ejecutar la escena; la lista muestra la versión
instalada de cada uno, y la × lo quita del proyecto. Una versión escrita con el
nombre, como `rich>=13`, se conserva.

#figure(
  image("../../assets/playground/paquetes.webp", alt: "La pestaña Paquetes: el buscador y la lista con numpy y webcolors instalados en el pyproject.toml del proyecto"),
  caption: [`numpy`, de Pyodide, y `webcolors`, de PyPI, en el `pyproject.toml` del proyecto.],
)

Los paquetes se guardan como en cualquier proyecto de Python. Si el proyecto
tiene un `pyproject.toml` con la lista `dependencies` de la tabla
`[project]`, como los que crea `gaanim init`, van allí: es la lista donde
escribe `uv add`. Si no, van a `requirements.txt`, un paquete por línea. Puedes
editar esos archivos a mano; el panel los sigue. `gaanim` no hace falta
declararlo, porque el playground ya lo tiene.

Al ejecutar, el playground instala con
#link("https://pyodide.org/en/stable/usage/loading-packages.html")[micropip] lo
que falte. Los paquetes de Pyodide se cargan también solos cuando el código
los importa, aunque no estén en la lista. El editor completa los paquetes
instalados igual que la API de Gaanim. Si un `import` falla con
`ModuleNotFoundError`, la consola ofrece un enlace que añade el paquete. A
veces se llama distinto de su `import`: `import yaml` viene de `pyyaml`.

En el navegador solo se instalan los paquetes de Python puro (con una rueda
`py3-none-any`) y los que compila Pyodide, y lo mismo vale para sus
dependencias. El panel avisa de los que no se pueden instalar y dice por qué:
un paquete que no está en PyPI, o que necesita código nativo, falla sin
detener la escena, y *Reintentar* lo intenta otra vez. Esos paquetes funcionan
en la aplicación. Instalar necesita conexión; lo instalado sigue disponible
mientras no detengas Python.

= Del playground a la aplicación, y de vuelta <aplicacion>

Un proyecto del playground y uno de la aplicación tienen la misma forma:
`main.py` con rutas relativas a su carpeta. Para pasar de uno a otro:

- *Del playground a tu equipo.* Descarga el `.zip` del proyecto y
  descomprímelo. Si el proyecto tiene `pyproject.toml`, `uv sync` instala sus
  dependencias. Si usa `requirements.txt`, crea el proyecto con
  `gaanim init video mi-video`, copia los archivos y añade los paquetes con
  `uv add -r requirements.txt`. Luego `gaanim mi-video` abre la vista previa.
- *De tu equipo al playground.* Comprime la carpeta del proyecto y ábrela con
  *Abrir un .zip…*. Las dependencias de su `pyproject.toml` se instalan al
  ejecutar; `.venv`, `uv.lock` y los archivos ocultos no hacen falta.

```bash
uv add -r requirements.txt   # pasa los paquetes del playground al pyproject.toml
uv sync                      # instala las dependencias de un proyecto descargado
```

= Ejecutar, detener y corregir

El reproductor muestra la última grabación. Se detiene en cada `scene.stop()`
hasta un clic o `Espacio`, y si la pausa tiene un bucle (`loop=`), lo repite
mientras espera, como al presentar; *Continuar* sigue desde el final del
bucle. El vídeo exportado no se detiene y reproduce cada bucle una vez.

Mientras la escena corre, el reproductor dice si se está ejecutando el script
o grabando la escena, cuánto lleva grabado y cuánto tiempo ha pasado:

#figure(
  image("../../assets/playground/grabando.webp", alt: "El reproductor durante una grabación, con el porcentaje grabado y los segundos"),
  caption: [Una grabación en curso. El reproductor sigue mostrando la anterior.],
)

*Detener* (o `Esc`) para la ejecución. Como Python no se puede interrumpir a
mitad de un cálculo, el playground lo reinicia. Si la ejecución duraba un
rato, ya tenía otro Python cargándose en segundo plano, y vuelves a tenerlo
listo casi de inmediato.

Un error escribe su traceback en la consola, abre el archivo donde ocurrió
(también dentro de un módulo) y marca la línea. Cada `File "…", line N` de la
consola es un enlace a ese punto:

#figure(
  image("../../assets/playground/error.webp", alt: "Un AttributeError dentro de lib/formas.py: el archivo abierto con la línea marcada y el traceback en la consola"),
  caption: [Un error en `lib/formas.py`: el archivo se abre en la línea que falló.],
)

El editor también marca los errores de sintaxis mientras escribes. Con *En
vivo* activado, la escena se ejecuta sola cuando dejas de escribir y el código
no tiene errores de sintaxis; si una ejecución ya está en marcha, la
siguiente espera a que termine.

= Compartir y descargar

*Compartir* copia un enlace que lleva los archivos de texto del proyecto
comprimidos en la propia dirección: quien lo abre recibe una copia como
proyecto nuevo. Las imágenes, el audio y las fuentes no caben en un enlace;
para compartirlos, descarga el `.zip`.

*Descargar* ofrece tres cosas:

- *Vídeo o GIF…* convierte la última grabación en MP4 (H.264), WebM (VP9) o
  GIF, a 480p, 720p, 1080p o 4K. La conversión usa los codificadores del
  navegador y el archivo no sale de tu equipo. El MP4 y el WebM llevan el
  audio de la escena si el navegador sabe codificarlo; si no, el diálogo
  avisa de que va sin sonido. El GIF va sin sonido, con hasta 25 fotogramas
  por segundo y 256 colores.
- El `.gaanim` de la grabación, que se reproduce, presenta y exporta con la
  aplicación sin Python (ver #link("/guias/compartir/")[Compartir sin Python]).
- El proyecto en un `.zip`.

#figure(
  image("../../assets/playground/video.webp", alt: "El diálogo Exportar vídeo, con el formato, el tamaño y la duración"),
  caption: [Exportar vídeo, con el tamaño y los fotogramas del resultado.],
)

El vídeo del navegador no dibuja el posprocesado ni los fondos, transiciones y
efectos de shader; el diálogo avisa cuando la escena los usa. Para un vídeo
con todo, o con la calidad de producción, expórtalo con la aplicación.

= Insertar en un blog o sitio web

*Insertar* prepara la escena para publicarla: descarga el `.gaanim` con el
nombre que elijas y compone el `<iframe>` del reproductor ligero, con la
dirección donde lo publicarás y la proporción de la escena. Al lado muestra
cómo se verá. Los detalles y las opciones están en
#link("/guias/compartir/#insertar")[Insertar en un blog o sitio web].

#figure(
  image("../../assets/playground/insertar.webp", alt: "El diálogo Insertar: la vista previa a la izquierda y los pasos con el código del iframe a la derecha"),
  caption: [Insertar: el nombre del archivo, su dirección, las opciones y el código.],
)

= Tema claro y oscuro

El playground sigue el tema de tu sistema. El botón del sol o la luna lo
cambia y la elección se recuerda; es la misma que la del botón de esta
documentación, así que cambiar uno cambia el otro.

#figure(
  image("../../assets/playground/claro.webp", alt: "El playground con el tema claro"),
  caption: [El tema claro. La escena conserva sus propios colores.],
)

= Límites

- Hace falta un navegador con WebGPU: Chrome o Edge recientes en el
  escritorio, y Safari o Firefox en las versiones que ya lo traen activado.
  Sin WebGPU, el reproductor no puede dibujar la escena.
- Python corre en un solo hilo del navegador: una escena larga o con muchos
  objetos tarda más en grabarse que en la aplicación.
- Lo que necesita FFmpeg (vídeos como recurso) y los paquetes con código
  nativo que no están en Pyodide solo funcionan en la aplicación, igual que
  `subprocess`, los hilos y las conexiones de red desde el script.
- Los proyectos viven en el navegador. Descarga el `.zip` de los que
  importen: es también la forma de llevarlos a otro equipo.
