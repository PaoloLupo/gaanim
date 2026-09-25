#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Instalación y primeros pasos",
  description: "Instala Gaanim y crea tu primera animación",
  route: "/empezar/instalacion/",
)

= Instalación — resumen

Gaanim se instala desde las
#link("https://github.com/PaoloLupo/gaanim/releases/latest")[releases de GitHub]:
cada versión publica un paquete listo para usar, sin compilar nada. La
#link("/empezar/instalacion/")[instalación detallada] explica cómo
verificar la descarga, cómo encuentra Python y cómo actualizar.

== Requisitos

- *Windows 10/11 x64* o *Ubuntu 24.04 x64*. macOS todavía no tiene paquete.
- *Python 3.14*. En Windows también sirve una versión posterior.
- *#link("https://docs.astral.sh/uv/")[uv]*, que crea el entorno de cada proyecto.
- Una GPU con controladores actualizados (Vulkan o DirectX 12).
- *FFmpeg*, opcional: solo para exportar MP4/WebM o usar video y audio.

== Descarga e instala

En la #link("https://github.com/PaoloLupo/gaanim/releases/latest")[última release]
descarga el paquete de tu sistema:

#table(
  columns: 2,
  table.header[*Sistema*][*Archivo*],
  [Windows 10/11 x64], [`gaanim-v<versión>-windows-x64.zip`],
  [Ubuntu 24.04 x64], [`gaanim-v<versión>-linux-x64.tar.gz`],
)

En Windows, extrae el zip en una carpeta y añádela al `PATH` de tu usuario:

```powershell
Expand-Archive .\gaanim-v*-windows-x64.zip -DestinationPath C:\Tools\gaanim
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
[Environment]::SetEnvironmentVariable("Path", "$userPath;C:\Tools\gaanim", "User")
```

En Ubuntu, copia los ejecutables a `~/.local/bin` y el wheel de autoría a
`~/.local/share/gaanim`:

```bash
tar -xzf gaanim-v*-linux-x64.tar.gz
install -Dm755 -t ~/.local/bin gaanim gaanim-core
install -Dm644 -t ~/.local/share/gaanim gaanim-*-py3-none-any.whl
```

== Verifica la instalación

Abre una terminal nueva para que tome el `PATH` actualizado:

```bash
gaanim --version
gaanim init video mi-video   # crea el proyecto y su entorno con uv
gaanim mi-video              # abre la vista previa con recarga al guardar
```

`gaanim` sin argumentos abre el Inicio, desde donde también puedes crear y
abrir proyectos. Si quieres contribuir al propio Gaanim y compilarlo desde el
código, sigue el #link("https://github.com/PaoloLupo/gaanim#readme")[README del
repositorio].

= Tu primera animación

Crea un archivo llamado `my_animation.py`:

```python
from gaanim import Easing, BLACK, BLUE, GOLD, Scene

scene = Scene(frame=(16, 9), background=BLACK)

circle = scene.geometry.circle(1).fill(BLUE).stroke(GOLD, 0.05)
text = scene.text("Hello World", role="title")

scene.play([
    circle.animate.grow_from_center().duration(2.0).easing(Easing.spring(stiffness=90, damping=12)),
    text.animate.write().duration(2.0).easing(Easing.SMOOTH),
])

scene.wait(1.0)
scene.play([
    circle.animate.shift_by(2.5, 0).duration(1.5).easing(Easing.SMOOTH),
    text.animate.fade_out().duration(0.5),
])
scene.render()

# Run with: gaanim my_animation.py
# output: preview.webp
```

Ejecútalo:

```bash
gaanim my_animation.py
```

Se abrirá la ventana de previsualización de Gaanim. Pulsa `Escape` para cerrarla.

= Exportación

Para exportar en lugar de abrir la previsualización:

```bash
gaanim export my_animation.py --output output.mp4    # MP4
gaanim export my_animation.py --output overlay.webm  # WebM
gaanim export my_animation.py --output preview.webp  # WebP animado
gaanim export my_animation.py --output preview.gif   # GIF
```

La extensión de `--output` elige el formato: `mp4`, `webm`, `webp`, `gif` o
`png` (secuencia de imágenes).

= Siguientes pasos

- #link("/empezar/primera-animacion/", "Guía rápida") — crea un proyecto de movimiento circular
- #link("/guias/presentaciones/", "Presentaciones") — flujo completo para presentaciones en vivo

- #link("/referencia/scene/", "API de Scene") — consulta el contrato técnico de la escena
- #link("/referencia/objetos/", "Objetos") — explora las figuras y objetos disponibles
- #link("/referencia/animations/", "Animaciones") — consulta animaciones y funciones de tiempo
- #link("/ejemplos/basicos/", "Ejemplos") — escenas completas y ejecutables
