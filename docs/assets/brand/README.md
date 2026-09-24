# Identidad de marca de Gaanim

Todos los archivos de esta carpeta salen de `tools/generate_brand.py`. Para
cambiar la geometría o los colores, edita el script y ejecútalo de nuevo con
`python tools/generate_brand.py`. No edites los SVG ni los PNG a mano.

## Concepto

El símbolo muestra **tres fotogramas de una misma interpolación**: un cuadrado
que se convierte en círculo mientras avanza, dibujado como en papel cebolla (los
fotogramas anteriores se desvanecen hacia el fondo). Es exactamente lo que hace
el motor: toma un estado, lo interpola en el tiempo y lo presenta fotograma a
fotograma.

El símbolo está en **píxel**, que es lo que rasteriza la GPU. El logotipo está en
**curvas vectoriales**, que es lo que describe la escena. El contraste entre los
dos es intencionado.

## Construcción

Todo se dibuja sobre una sola rejilla de unidades.

| Elemento | Medida |
| --- | --- |
| Lienzo del icono | 16 × 16 unidades, esquinas escalonadas de 2 pasos |
| Fotograma | 8 × 8 unidades (cuadrado, intermedio, círculo de píxel) |
| Paso entre fotogramas | 2 unidades; símbolo completo de 12 × 8 |
| Altura x del logotipo | 8 unidades, igual al diámetro del círculo |
| Trazo | 2 unidades, igual al paso entre fotogramas |
| Ascendente (punto de la i) y descendente (cola de la g) | 4 unidades cada uno, así el logotipo mide 16, como el icono |
| Punto de la i | cuadrado de 2 × 2, alineado a la rejilla de píxel |
| Espaciado | 2 entre astas, 1,5 entre asta y curva |
| Arcos de la m | 7 de ancho (contraformas de 3, frente a 4 en la n) |
| Overshoot óptico | 0,12 en vertical para las formas redondas |
| Lockup horizontal | 71 × 16 unidades, separación de 4 entre símbolo y texto |

## Archivos

| Archivo | Uso |
| --- | --- |
| `gaanim-logo.svg` | Logotipo horizontal sobre fondos claros |
| `gaanim-logo-dark.svg` | Logotipo horizontal sobre fondos oscuros |
| `gaanim-logo-mono.svg` | Una tinta (`currentColor`); fotogramas al 30 % y 60 % |
| `gaanim-symbol.svg`, `gaanim-symbol-dark.svg` | Símbolo sin texto |
| `gaanim-icon.svg` | Icono con fondo; favicon SVG |
| `favicon.ico` | 16, 32 y 48 px, escalados sin interpolar |
| `apple-touch-icon.png` | 180 px, opaco (el sistema redondea las esquinas) |
| `gaanim-icon-512.png` | Avatar para GitHub y redes |
| `gaanim-social.png` | Vista previa social del repositorio (1280 × 640) |

## Color

| Nombre | Hex | Uso | Contraste |
| --- | --- | --- | --- |
| Tinta | `#14112E` | Texto sobre claro, fondo del icono | 18,3:1 sobre blanco |
| Papel | `#F7F6FF` | Texto sobre oscuro | 17,5:1 sobre `#121212` |
| Violeta | `#7C6CFF` | Fotograma intermedio, gráficos | 3,9:1 sobre blanco (solo gráficos) |
| Violeta profundo | `#5B4AF0` | Enlaces y texto de marca sobre claro | 5,7:1 sobre blanco |
| Violeta claro | `#A99FFF` | Enlaces sobre oscuro | 8,1:1 sobre `#121212` |
| Bruma | `#CBC6FF` / `#3F37A8` | Primer fotograma sobre claro / oscuro | decorativo |
| Oro | `#FFC933` | Fotograma actual y punto de la i sobre oscuro | 12,2:1 sobre `#121212` |
| Oro profundo | `#F0A800` | Punto de la i sobre claro | decorativo |

Superficies de la web: **Papel cálido** `#FAF9F6` en el tema claro y
**Grafito** `#1A1A1A`/`#121212` en el oscuro, sin tinte azul.

El fotograma actual siempre usa el color de mayor contraste con el fondo (tinta
sobre claro, oro sobre oscuro) y los anteriores se acercan al fondo.

## Lenguaje de interfaz

La web aplica el símbolo a sus propios controles (`docs/assets/base.css`):

- **Esquinas rectas.** Ningún control ni contenedor lleva radio: todo es un
  fotograma.
- **Hover por fotogramas.** Botones, tarjetas y vistas previas avanzan 3 px en
  tres pasos (`steps(3)`) y dejan dos fotogramas fantasma en violeta (tokens
  `--frames` y `--frame-motion`).
- **Marcador de sección.** Tres cuadrados, del más tenue al más intenso, antes
  de cada sección principal.
- **Fotogramas clave.** Los grupos del menú lateral son rombos, huecos al
  cerrarse y rellenos al abrirse, como en la línea de tiempo del hero.
- **Encuadre.** El hero y las vistas previas llevan marcas de esquina de cámara
  y una rejilla de unidades.
- **Cabezal.** Bajo la cabecera, una línea con un cabezal cuadrado muestra el
  progreso de lectura.
- **Etiquetas.** Botones, insignias, cabeceras de tabla y rótulos usan la fuente
  de código (VictorMono).
- **Movimiento reducido.** Con `prefers-reduced-motion` los cambios son
  instantáneos y la animación del hero queda en un fotograma fijo.

## Uso

- **Escala entera.** Muestra el logotipo a un número entero de píxeles por
  unidad (71 × 16, 142 × 32, 213 × 48…) para que los píxeles del símbolo no se
  emborronen. La cabecera de la web usa 2 px por unidad.
- **Tamaño mínimo.** El icono funciona desde 16 px. El lockup horizontal se usa
  desde 142 px de ancho; por debajo, usa el icono.
- **Área de respeto.** Deja al menos 4 unidades libres alrededor (la mitad de la
  altura x).
- **Versión según el fondo.** `gaanim-logo.svg` sobre fondos claros y
  `gaanim-logo-dark.svg` sobre oscuros; sobre fotos o color, la versión mono.
- **No** deformes, rotes ni recolorees los fotogramas por separado, **no** añadas
  sombras ni degradados y **no** recompongas el logotipo con una fuente.
