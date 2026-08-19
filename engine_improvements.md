# Gaanim — estado actual y plan de evolución

> Corte auditado: **2026-08-19** sobre el workspace local. La configuración, el código y
> las pruebas tienen prioridad sobre este documento. Gaanim sigue en **0.1.0** y debe
> considerarse un alfa funcional, no una API estable.

## Resumen ejecutivo

Gaanim ya es un motor de autoría capaz: combina escenas vectoriales 2D, texto y
matemática Typst, visualización de datos, una ruta 3D nativa, modelos glTF, video
embebido, presentaciones, hot reload, seek y exportación desde una API Python sobre un
núcleo Rust/Bevy/Vello. La arquitectura por crates está bien separada y el workspace
compila y prueba estas capacidades en varios niveles.

La principal brecha ya no es la cantidad de objetos disponibles. Es la diferencia entre
**capacidad implementada** y **contrato de producto verificable**:

- la auditoría objetiva falla porque diez exports Python nuevos de composición y matrices
  no están declarados en el stub superior `gaanim_core.pyi`;
- hay 67 juegos de baselines visuales, pero CI compara solo cuatro ejemplos;
- existen video embebido y audio de preview para ese video, pero la ruta depende de
  `ffmpeg`/`ffprobe` y todavía carece de una matriz E2E multiplataforma;
- el wheel distribuible es deliberadamente una capa de autoría `py3-none-any`, sin
  extensión nativa ni renderer: ejecutar escenas requiere la aplicación;
- no hay presupuestos ni benchmarks reproducibles para seek, hot reload, preview o
  exportación;
- CI cubre Ubuntu y Windows, mientras el release instalable solo empaqueta Windows.

La recomendación para 0.2 es un ciclo de **convergencia y hardening**. Añadir más
primitivas antes de cerrar el contrato Python, los exports, la instalación y los E2E
aumentaría la superficie inestable.

### Semáforo de producto

| Caso de uso | Estado | Lectura actual |
|---|---:|---|
| Animación vectorial 2D | 🟢 | Núcleo, preview, seek y exportación funcionales |
| Texto, ecuaciones y contenido educativo | 🟢 | Typst, partes semánticas y transforms son una fortaleza |
| Visualización de datos | 🟡 | API declarativa, escalas, charts y estadísticas; contrato reciente |
| Matrices y álgebra animada | 🟡 | API estructurada y SymPy opcional; integración de stub aún rota |
| Presentaciones en vivo | 🟢 | Segmentos, stops, notas, overview y Presenter View disponibles |
| Escenas 3D y glTF | 🟡 | Ruta funcional con baselines; compatibilidad y E2E aún estrechos |
| Video y audio | 🟡 | Video sincronizado y audio embebido audible; faltan waveform y E2E |
| Distribución como librería Python autónoma | 🔴 | El wheel es solo autoría/tipos; el runtime vive en la aplicación |
| Pipeline audiovisual de producción | 🟡 | Muchos formatos y features, sin matriz de release suficientemente probada |

## Evidencia del corte

La fotografía del repositorio al iniciar esta actualización estaba limpia. Los datos
siguientes se obtuvieron de manifests, CI, código y fixtures, no del roadmap anterior:

| Señal | Resultado |
|---|---|
| Workspace | 20 miembros: 19 crates `gaanim_*` y `docs` |
| Versiones | Todos los crates y el paquete Python declaran `0.1.0` |
| Código Rust | 124 archivos `.rs` bajo `crates/` |
| Pruebas Rust | 469 atributos `#[test]`/`#[tokio::test]` encontrados |
| Ejemplos Python | 101 archivos bajo `examples/` |
| Documentación Typst | 35 archivos bajo `docs/content/` |
| Regresión visual | 67 manifests de baseline |
| CI visual | 4 ejemplos: transform, image, SVG y camera |
| CI general | fmt, check, workspace tests y Clippy en Ubuntu/Windows |
| Paquete Python | wheel universal de autoría, sin binario nativo |
| Auditoría del plugin | 1 error objetivo de sincronización del stub superior |

Error de contrato detectado:

```text
Top-level native imports missing from stub:
AnimationGroup, LaggedStart, Succession,
Matrix, MatrixAlgebraError, MatrixDerivation, MatrixEntry,
MatrixSelection, MatrixSelectionAnimation, MatrixStep
```

Esto no significa que esas clases falten: existen en `composition.py`, `matrix.py` y sus
stubs específicos, y se reexportan desde `gaanim.__init__`. Significa que el validador y
el stub canónico aún no modelan correctamente una API pública compuesta por módulos
puros y símbolos PyO3. Es deuda P0 porque hoy la auditoría de repositorio no pasa.

## Arquitectura actual

| Capa | Crates | Estado |
|---|---|---:|
| Tipos y matemáticas | `gaanim_core`, `gaanim_expr`, `gaanim_math` | Sólida |
| ECS y jerarquía | `gaanim_scene` | Sólida; orden centralizado en `SceneSet` |
| Animación y timeline | `gaanim_animation`, `gaanim_timeline` | Funcional; falta perf cuantificada |
| Media | `gaanim_media` | Video + audio embebido sincronizados; depende de FFmpeg |
| Render y objetos | `gaanim_renderer`, `gaanim_objects`, `gaanim_text` | 2D maduro, 3D reciente |
| Layout y visualización | `gaanim_layout`, `gaanim_visualization` | Amplios; APIs en consolidación |
| Fachada Rust | `gaanim_api` | Canónica, con alta dependencia transversal |
| Fachada Python | `gaanim_python` | Amplia; contrato modular todavía desalineado |
| Aplicación | `gaanim_editor`, `gaanim_launcher`, `gaanim_project` | Flujo principal de ejecución |
| Salida y comparación | `gaanim_export`, `gaanim_diff` | Funcionales; matriz E2E incompleta |

La dirección arquitectónica se debe preservar: Python construye specs diferidos, la
aplicación los materializa en ECS y timeline, y preview/export comparten el modelo
temporal. No se deben introducir wrappers de `peniko`/`kurbo`/`glam`, imports ECS directos
fuera de las capas permitidas ni orden de sistemas al margen de `SceneSet`.

### Decisión de producto vigente

`Scene` es la fachada pública. `scene.canvas` contiene viewport, fondo, dimensiones y
safe areas. `Canvas(...)` es solo compatibilidad deprecada y debe desaparecer antes de
1.0.

La distribución actual tiene dos piezas distintas:

1. **Aplicación `gaanim`:** contiene PyO3, renderer, preview, hot reload y exportación.
2. **Wheel de autoría:** instala helpers, stubs y `py.typed`; excluye `.so`, `.pyd` y
   `.dylib` y no puede ejecutar una escena por sí solo.

El proyecto debe documentar esta frontera siempre con los mismos términos. Hablar de una
“wheel headless” o de importación autónoma contradice el empaquetado actual.

## Capacidades confirmadas

### Autoría y animación

- formas, paths, SVG, raster, grupos, estilos, gradientes, clipping y efectos;
- transforms, matching de formas/texto/TeX, escritura, creación y animaciones de cámara;
- composición con `AnimationGroup`, `LaggedStart` y `Succession` desde helpers Python;
- layouts, anchors, regiones, reflow y componentes editoriales;
- sistema reactivo con expresiones, parámetros, geometría derivada y callbacks;
- segmentos, transiciones, stops, notas y navegación de presentación.

### Texto, datos y matemática

- shaping vectorial, Typst, partes semánticas y selección de texto;
- ejes 2D/3D, escalas lineales/no lineales, funciones, campos y superficies;
- charts declarativos, series, estadísticas y ejemplos visuales;
- matrices con selección por filas/columnas/bloques/diagonales, órdenes animados,
  morphing y álgebra opcional mediante SymPy.

La API de matrices es más avanzada que lo descrito en el corte anterior, pero sigue en
estado beta hasta que auditoría, validación del wheel, ejemplos y docs se ejecuten juntos.

### 3D y assets

- primitivas PBR, cámara perspectiva, HUD y billboards;
- `axes_3d`, superficies y polilíneas con reveal;
- glTF/GLB con jerarquía, materiales, skins, morph targets y acciones;
- baselines para ejes 3D, primitivas, glTF y escenas de visualización.

La existencia de baselines locales no equivale a cobertura continua: la mayoría no se
ejecuta en CI.

### Multimedia y exportación

- MP4/H.264, WebM/VP9, WebP animado, GIF y secuencia PNG;
- mezcla de pistas declaradas con `scene.audio(...)` para MP4/WebM;
- `scene.video(...)` como drawable sincronizado, con trim, loop, velocidad, volumen,
  seek visual y audio embebido audible en preview;
- decodificación secuencial durante playback y solicitudes coalescidas durante scrub;
- proceso aislado para exportaciones 3D iniciadas desde el editor.

La distinción actual es importante: el audio de un video embebido tiene preview; las
pistas independientes añadidas con `scene.audio(...)` siguen orientadas a exportación y
no tienen waveform ni scrubbing audible de primera clase.

## Riesgos técnicos y mejoras propuestas

### 1. Contrato Python fragmentado — P0

El binding PyO3, `gaanim_core.pyi`, módulos Python, `__init__.py`, ejemplos y docs forman
una sola API, pero la validación actual mezcla “nativo” con “público”. El error de
auditoría demuestra la deriva.

**Mejora:** definir un manifiesto de API pública generado o declarativo que enumere
propietario, reexport y origen (`native` o `python`) de cada símbolo. Usarlo para validar
`__all__`, stubs, runtime embebido y documentación.

**Criterio de salida:** auditoría limpia; imports y tipos coinciden; cada símbolo público
tiene prueba de importación y referencia documental.

### 2. Cobertura visual amplia pero poco continua — P0

67 baselines frente a cuatro comparaciones de CI dejan sin gate continuo matrices,
charts, layouts, presentación, glTF y 3D.

**Mejora:** clasificar baselines por subsistema y crear una suite smoke de 10–15 escenas
en cada PR, con shards o rotación determinista. Ejecutar la matriz completa de forma
programada y antes de release.

**Criterio de salida:** cada subsistema de render tiene al menos una escena gated; la
suite completa tiene dueño, frecuencia y tiempo máximo documentados.

### 3. Exportación y media dependen del entorno — P0

FFmpeg, ffprobe, codecs, GPU y drivers varían por plataforma. Hay unit tests y un fixture
de media, pero no una matriz CI que produzca y sondee cada formato, audio incluido.

**Mejora:** fixtures mínimos reproducibles y smoke tests que exporten MP4, WebM, WebP,
GIF y PNG; validar dimensiones, duración, frames, alpha y streams con ffprobe. Separar
explícitamente rutas GPU, CPU y export 3D aislado.

**Criterio de salida:** todos los formatos se verifican en Ubuntu y Windows; los formatos
audiovisuales confirman codec, duración y audio, no solo exit code.

### 4. Rendimiento sin presupuesto — P1

El timeline usa índices y snapshots, el video tiene decodificador secuencial y el renderer
retiene fragmentos, pero no existen números reproducibles que detecten regresión.

**Mejora:** benchmarks de seek aleatorio, hot reload de una escena mediana, preview 1080p
y export de 300 frames. Registrar p50/p95, memoria máxima y frames por segundo.

**Criterio de salida:** presupuestos versionados y comparables; las regresiones se
reportan automáticamente, aunque inicialmente no bloqueen PRs.

### 5. Historia de instalación incompleta — P1

El release crea un zip Windows, CI prueba Ubuntu y el README menciona Linux/macOS. No hay
una matriz que distinga “compila”, “CI”, “paquete instalable” y “soportado”.

**Mejora:** publicar esa tabla y escoger una estrategia: aplicación nativa por plataforma,
archivo portable, o runtime separado del wheel de autoría. Mantener el wheel puro mientras
esa decisión siga vigente.

**Criterio de salida:** un usuario nuevo instala, crea proyecto, previsualiza y exporta
siguiendo un quickstart probado por release para cada plataforma declarada.

### 6. Dos modelos de audio en preview — P1

El video embebido reproduce audio; `scene.audio(...)` no. Esta asimetría será confusa al
crear contenido sincronizado con voz o música.

**Mejora:** unificar ambos como fuentes de una mezcla de preview, con reloj, seek, pausa,
velocidad y latencia compartidos. Añadir waveform cacheada y marcadores sin reconstruir
prematuramente un editor multipista.

**Criterio de salida:** una pista independiente y el audio de un video conservan sync al
reproducir, pausar, cambiar velocidad y hacer seek repetido.

### 7. Complejidad de API en expansión — P1

101 ejemplos y una superficie extensa facilitan demos, pero aumentan duplicación,
compatibilidad y coste documental.

**Mejora:** establecer niveles `stable`, `experimental` e `internal`; introducir pruebas
de compatibilidad por nivel y una política de deprecación. Consolidar un catálogo pequeño
de recorridos canónicos y etiquetar el resto como fixtures.

**Criterio de salida:** cada API pública tiene nivel y propietario; cambios en `stable`
requieren deprecación y nota de migración.

### 8. Hot reload completo — P2

El proceso gráfico sobrevive, pero guardar vuelve a ejecutar el script y recompila specs.

**Mejora:** primero instrumentar tiempos y asignar IDs estables; luego aplicar diff
incremental solo a recursos y subárboles seguros. Posponer edición bidireccional hasta
tener identidad y semántica de invalidación claras.

**Criterio de salida:** cambios de estilo o posición preservan recursos no afectados y
reducen el p95 de reload sin alterar el resultado de un rebuild completo.

## Roadmap recomendado

### 0.1.x — Reparar el baseline

1. Corregir el contrato de stubs para composición y matrices.
2. Hacer que `audit.py` y `validate-python-api` entiendan símbolos nativos y helpers
   Python sin falsos positivos ni huecos.
3. Actualizar docs de matrices, composición, video y distribución del wheel.
4. Añadir un smoke CI de matrices y otro de video con audio.
5. Declarar la matriz real de plataformas y artefactos disponibles.

### 0.2 — Hardening de creación y publicación

1. Suite visual representativa por subsistema y ejecución completa programada.
2. Matriz E2E de los cinco formatos de exportación en Ubuntu/Windows.
3. Preview unificado para pistas de audio y video, con sync bajo seek.
4. Benchmarks iniciales de seek, preview, reload y export.
5. Clasificación de estabilidad de API y política de deprecación.
6. Matriz glTF con assets de distintos exporters y límites documentados.

### 0.3 — Rendimiento y flujo creativo

1. Snapshots jerárquicos/adaptativos para reducir el coste de seeks largos.
2. Hot reload incremental guiado por perfiles.
3. Waveform, marcadores y snapping audiovisual.
4. Encuadre y picking coherentes para contenido mixto 2D/3D.
5. Export paralelo solo después de demostrar determinismo entre workers.

### 1.0 — Contrato estable

- API Rust/Python versionada, documentada y con migraciones;
- instalación reproducible en todas las plataformas declaradas;
- suite API, visual y E2E obligatoria para release;
- compatibilidad de proyectos entre versiones menores;
- presupuestos públicos de rendimiento y memoria;
- troubleshooting para GPU, Python, FFmpeg, fuentes y Typst.

## Indicadores de madurez

| Indicador | Baseline actual | Objetivo 1.0 |
|---|---:|---:|
| Auditoría objetiva | 1 error | 0 errores |
| Miembros workspace | 20 | Informativo; no maximizar |
| Ejemplos Python | 101 | 100% clasificados y ejecutables |
| Baselines visuales | 67 | 100% con frecuencia definida |
| Baselines ejecutados en CI por PR | 4 | Al menos uno por subsistema crítico |
| Formatos con smoke E2E multiplataforma | Incompleto | 5/5 |
| Plataformas de CI | Ubuntu, Windows | Igual a plataformas soportadas |
| Benchmarks con presupuesto | 0 | Seek, reload, preview y export |
| API pública cubierta por stubs/docs | <100% | 100% |

## Diferenciadores a preservar

1. Render vectorial GPU con Vello/wgpu y ruta 3D integrada.
2. ECS con orden de sistemas centralizado mediante `SceneSet`.
3. Timeline seekable compartido por preview, snapshots y exportación.
4. Texto y matemática Typst convertidos en geometría animable.
5. Modelo diferido apto para hot reload y determinismo.
6. Partes semánticas aplicables a texto, matrices, SVG, charts y glTF.
7. Separación entre fachada, runtime, editor, media y exportación.
8. Tipos 3D-ready y uso directo de `peniko`, `kurbo` y `glam`.

La oportunidad diferencial de Gaanim es ofrecer **autoría Python expresiva con un motor
Rust/GPU determinista y seekable**. Para convertirla en ventaja de producto, el siguiente
paso debe demostrar que una escena puede instalarse, reproducirse, inspeccionarse y
exportarse de forma repetible. La profundidad ya existe; ahora debe volverse contrato.
