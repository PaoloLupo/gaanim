# Presentaciones de tesis con Gaanim

Gaanim incluye un flujo completo para preparar y ejecutar una sustentación:

- plantilla de tesis 16:9 con nueve slides editables;
- layouts semánticos y revelados controlados por el expositor;
- Presenter View con notas, slide actual, siguiente parada y cronómetro;
- overview navegable por nombre para responder preguntas;
- hot reload durante la edición;
- exportación de respaldo a video;
- capturas y regresión visual determinista.

## 1. Crear la presentación

Después de compilar Gaanim, prepara la DLL de Python para esta sesión de PowerShell:

```powershell
$pyBase = & .\.venv\Scripts\python.exe -c "import sys; print(sys.base_prefix)"
$env:PATH = "$pyBase;$env:PATH"
```

Ahora genera la presentación:

```powershell
target/debug/gaanim.exe init thesis mi_tesis.py
```

El comando no reemplaza archivos existentes. Para reemplazar uno de forma explícita:

```powershell
target/debug/gaanim.exe init thesis mi_tesis.py --force
```

La plantilla generada funciona inmediatamente. La portada incluida reproduce el sistema
visual institucional con fondo exacto `#1601FC`, texto blanco y Tw Cen MT. En Windows la
fuente se detecta desde `C:\Windows\Fonts\TCM_____.TTF`; no se sustituye silenciosamente
por otra tipografía.

Todo el sistema visual se configura con un único objeto:

```python
from gaanim import Scene, ThesisTemplate

scene = Scene(1920, 1080, margin=72)
design = ThesisTemplate(
    scene,
    font_path="assets/TwCenMT.ttf",  # opcional en Windows
    logo="assets/logo_ucsp_blanco.svg",
    background="#1601FC",
)
design.cover(
    "TÍTULO DE LA TESIS\nEN DOS O TRES LÍNEAS",
    "NOMBRE DEL AUTOR  •  NOMBRE DEL COAUTOR",
    "AGOSTO 2026",
)
```

También puede definirse `GAANIM_TW_CEN_FONT` para una copia licenciada de la fuente y
`GAANIM_THESIS_LOGO` para el logo blanco. Si no se proporciona un logo, aparece un
marcador vectorial neutro que hace evidente el lugar que debe reemplazarse. Busca los
textos entre corchetes (`[...]`) del resto de slides y reemplázalos con tu investigación.
Colores, títulos, texto, tablas, gráficos, captions y paneles heredan el mismo tema.

## 2. Previsualizar mientras editas

```powershell
target/debug/gaanim.exe mi_tesis.py
```

Guarda el archivo para aplicar hot reload sin reiniciar la ventana.

## 3. Modelo de navegación

```python
slide = scene.slide(
    "Resultados",
    notes="Explica primero el hallazgo y después la métrica.",
    layout="two_columns",
)

title = slide.region("title").place(
    scene.title("Resultado principal"),
    Anchor.CENTER,
)

chart = scene.bar_chart([42, 61, 86], labels=["Inicial", "Piloto", "Final"])
scene.play([chart.fade_in().duration(0.5)])
slide.step("hallazgo")
```

El contenido estático aparece al entrar al slide. La misma pulsación que avanza desde la
parada anterior inicia la primera animación del slide siguiente. Gaanim vuelve a pausar al
alcanzar `slide.step()`. Esto evita el estado intermedio de “fondo vacío” y conserva el
comportamiento esperado de una presentación en vivo.

Controles:

| Acción | Tecla |
|---|---|
| Avanzar o continuar animación | `→`, `Enter`, `Espacio` o clic izquierdo |
| Volver a la parada anterior | `←` o `Backspace` |
| Inicio / final | `Home` / `End` |
| Abrir overview en Presenter View | `O` |
| Pantalla pública negra / restaurar | `B` |
| Pantalla pública blanca / restaurar | `W` |
| Salir de pantalla completa | `Esc` |

## 4. Presentar con dos pantallas

Los índices de monitor empiezan en cero. Normalmente el proyector será `1`:

```powershell
target/debug/gaanim.exe --present --monitor 1 mi_tesis.py
```

La salida pública se abre a pantalla completa y Presenter View queda en una segunda
ventana. Presenter View contiene:

- título y paso actuales;
- siguiente parada;
- notas del expositor;
- tiempo de la presentación;
- controles anterior, pausa y siguiente;
- overview con miniaturas renderizadas de cada slide;
- búsqueda y salto directo por nombre de slide;
- acceso directo a cualquier paso nombrado desde la tarjeta del slide.

Las miniaturas se generan de forma asíncrona en un mundo de render aislado. Abrir el
overview no mueve el timeline público ni interrumpe una animación. Al guardar el script,
hot reload incrementa la revisión de la escena y las miniaturas se regeneran.

## 5. Validación visual

Antes de ensayar, ejecuta el preflight semántico:

```powershell
target/debug/gaanim.exe check mi_tesis.py
target/debug/gaanim.exe check mi_tesis.py --strict
```

Comprueba slides, duración, formato 16:9, notas, pausas nombradas y placeholders sin
reemplazar. `--strict` devuelve un error también ante advertencias, por lo que puede usarse
en CI o en el checklist final.

La plantilla incluye nueve tiempos de captura representativos. Crea la primera línea base
cuando apruebes el diseño:

```powershell
target/debug/gaanim.exe --diff --example mi_tesis.py --bless --no-gui
```

Después de cada cambio importante:

```powershell
target/debug/gaanim.exe --diff --example mi_tesis.py --no-gui
```

Un resultado sano termina con `0 changed, 0 missing`.

## 6. Exportar un respaldo

La plantilla reconoce `GAANIM_EXPORT`:

```powershell
$env:GAANIM_EXPORT = "mi_tesis_respaldo.mp4"
.\.venv\Scripts\python.exe mi_tesis.py
Remove-Item Env:GAANIM_EXPORT
```

El respaldo se exporta a 60 FPS con calidad `production`. Requiere FFmpeg disponible en
`PATH`. La presentación interactiva y el video usan el mismo timeline.

También puedes exportar un slide específico por su nombre semántico:

```python
scene.export("resultados.mp4", slide="Resultados", quality="production")
```

Para generar un archivo por cada slide sin añadir otra función al API:

```python
scene.export(
    "slides/{index}-{slide}.mp4",
    slide="*",
    quality="production",
)
```

`{index}` produce `01`, `02`, etc.; `{slide}` usa un nombre seguro para archivos. Gaanim
crea el directorio padre y usa los límites exactos registrados por `scene.slide(...)`.

## 7. Checklist para el día de la sustentación

1. Compila en release: `cargo build -p gaanim_editor --release`.
2. Ejecuta la regresión visual y confirma `0 changed, 0 missing`.
3. Exporta y reproduce el MP4 de respaldo.
4. Desactiva notificaciones y suspensión de Windows.
5. Conecta el proyector antes de iniciar Gaanim.
6. Comprueba el índice correcto con `--monitor`.
7. Recorre todos los slides una vez usando solo los controles de presentación.
8. Ejecuta `gaanim check mi_tesis.py --strict`.
9. Mantén `mi_tesis.py`, assets, ejecutable y MP4 en una misma carpeta de respaldo.
