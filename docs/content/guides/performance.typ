#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Rendimiento reproducible",
  description: "Perfiles, presupuestos y reportes p50/p95 del runtime nativo",
  route: "/guides/performance/",
  updated: datetime.today().display(),
  code-langs: (),
)

Gaanim mide el flujo completo desde el ejecutable nativo. El wheel de autoría
no participa como runtime. Los presupuestos versionados viven en
`tests/performance/budgets.json` y los resultados locales se escriben bajo
`target/performance/`.

= Ejecutar el perfil rápido

```powershell
just benchmark smoke
```

El perfil `smoke` comprueba el cableado con una muestra corta. Para obtener la
medición comparable usada por el workflow programado:

```powershell
just benchmark standard
```

Ambos usan `libx264` por defecto para mantener una referencia determinista. Para
medir un encoder hardware concreto, páselo como segundo argumento, por ejemplo
`just benchmark smoke nvenc`. La selección es estricta: si NVENC no está
disponible, el benchmark falla en vez de cambiar silenciosamente a software.

El reporte `target/performance/runtime-benchmark.json` registra muestras,
p50/p95, throughput y RSS máxima. En Linux la memoria incluye el árbol del
proceso —también FFmpeg—; otras plataformas pueden informar solo el proceso o
marcar la métrica como no disponible.

= Escenarios

- `reload` inicializa una vez Python y el mundo ECS, y mide una segunda carga del
  script más su replay sobre ese mismo proceso. El tiempo total del proceso queda
  aparte como diagnóstico.
- `seek` captura timestamps dispersos en orden determinista mediante el timeline,
  renderer GPU y readback PNG.
- `preview` captura una secuencia densa a 1920x1080 y reporta frames por segundo.
  Mide la ruta headless, no la presentación con ventana y vsync.
- `export` produce 300 frames H.264 a 1920x1080 con el preset draft. Además del
  tiempo end-to-end, separa render GPU, espera por backpressure, trabajo activo
  de encode y drenaje/finalización. Como render y encode corren en paralelo,
  estas fases sirven para diagnóstico y no deben sumarse entre sí. El reporte
  guarda tanto el encoder solicitado como el efectivo para no comparar hardware
  y software como si fueran la misma configuración.

Se puede acotar una investigación sin cambiar el archivo versionado:

```powershell
python tests/benchmark_runtime.py --profile standard --scenarios seek preview --encoder libx264
```

= Presupuestos informativos

Los límites iniciales son deliberadamente amplios y el job programado usa
`continue-on-error`: una regresión queda registrada y conserva sus logs, pero
no bloquea un pull request. Después de acumular historia en runners estables,
`--enforce` convierte cualquier exceso de p95, memoria o throughput en un exit
code distinto de cero.

```powershell
python tests/benchmark_runtime.py --profile standard --enforce
```

No se deben estrechar presupuestos a partir de una sola laptop. Compara siempre
el mismo perfil, escena, plataforma, tipo de build (`--release`) y encoder.
