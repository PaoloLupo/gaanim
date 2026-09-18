# Lottie Compressor

Herramienta independiente para convertir Lottie JSON a `.lottie`, minificar JSON y
recomprimir `.lottie` existentes. Usa Python 3.10 o posterior y su biblioteca
estándar. No necesita Gaanim, Rust, servicios externos ni instalación de paquetes.
Puedes copiar esta carpeta a otro proyecto y ejecutarla allí.

## Uso

Desde esta carpeta:

```powershell
# JSON -> dotLottie v2 (genera animacion.lottie)
python lottie_compressor.py pack animacion.json

# Minificar un JSON (genera animacion.min.json)
python lottie_compressor.py minify animacion.json

# Minificar los JSON internos y recomprimir (genera animacion.min.lottie)
python lottie_compressor.py minify animacion.lottie

# Elegir salida e ID interno
python lottie_compressor.py pack animacion.json -o salida.lottie --id loading

# Reemplazar una salida existente; nunca permite reemplazar la entrada
python lottie_compressor.py minify animacion.lottie -o salida.lottie --force
```

La salida muestra bytes de entrada, bytes de salida y porcentaje de reducción.
`--level 0` a `--level 9` controla Deflate; el valor predeterminado es 9. La carpeta
de salida debe existir. Las salidas se publican desde un temporal mediante enlace
duro (sin reemplazo) o renombrado (`--force`); el sistema de archivos necesita
soportar estas operaciones, como NTFS en Windows.

## Qué conserva y qué reduce

- Elimina espacios y saltos de línea innecesarios del JSON. Conserva claves,
  nombres de capas, expresiones, marcadores y los números con su precisión original.
- Genera ZIP Deflate con `manifest.json` y `a/animation.json`, según
  [dotLottie v2](https://www.dotlottie.io/spec/2.0/).
- Al convertir, incluye las imágenes locales referenciadas por `assets[].u/p`
  dentro de `i/`, ajusta sus rutas y deduplica archivos idénticos. Busca las imágenes
  respecto a la carpeta del JSON; deben estar dentro de ella. Los bytes de las
  imágenes no se alteran. Las imágenes base64 permanecen incrustadas.
- Al recomprimir, conserva la versión original del manifiesto
  ([v1](https://www.dotlottie.io/spec/1.0/) o v2), todas las animaciones, imágenes,
  fuentes, temas, máquinas de estados y otros archivos. Solo minifica archivos
  `.json` y sustituye metadatos del ZIP por valores deterministas.
- Si recomprimir un paquete Deflate no reduce su tamaño, copia el original.
  Un JSON muy pequeño puede crecer al empaquetarlo por el coste del contenedor.
  Cuando incluye imágenes locales, el porcentaje compara el contenedor completo
  con el JSON original, sin sumar los archivos de imagen externos a la entrada.

No redondea coordenadas, simplifica curvas, elimina fotogramas ni convierte
imágenes a formatos con pérdida. No garantiza un porcentaje fijo de ahorro.

Las URL externas permanecen intactas y requieren acceso desde el reproductor;
la herramienta no descarga recursos. Las fuentes locales mediante `fPath` se
rechazan al convertir: exporta glifos o utiliza una URL de fuente. Las fuentes ya
contenidas en un `.lottie` se conservan. La compatibilidad de reproducción depende
de las funciones de animación que admita cada reproductor.

## Validación

```powershell
python -m unittest discover -s . -p "test_*.py" -v
```

Las pruebas comprueban precisión numérica, conservación de recursos v1/v2,
empaquetado de imágenes, idempotencia, entradas inválidas y protección contra
sobrescrituras. La validación de entrada comprueba estructura básica, JSON y
referencias de animaciones del manifiesto; no es un validador completo del esquema
Lottie ni una prueba de reproducción visual.

Por operación se admiten hasta 128 MiB por entrada, 128 MiB descomprimidos en total
y 4096 entradas ZIP. Se rechazan claves JSON duplicadas, NaN/Infinity, ZIP cifrados,
rutas inseguras y compresión ZIP distinta de Store/Deflate. El procesamiento es
en memoria y no ejecuta expresiones ni extrae archivos ZIP al disco.
