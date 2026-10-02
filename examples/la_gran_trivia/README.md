# La Gran Trivia

Una trivia de cultura general en vivo, por equipos (Búhos contra Alondras),
que junta todo lo que Gaanim ofrece para jugar con el público: sala de
espera con personajes, preguntas con imágenes y selección múltiple escritas
en Markdown, revelaciones con `magic_move` y confeti, una batalla de equipos,
una carrera de puntajes y un podio.

- Las preguntas están en `preguntas.md`: cada `#` es una pregunta, `[x]` marca
  las correctas (varias = selección múltiple, ninguna = encuesta) y las
  líneas `tiempo:`, `acierta:`, `votos:`, `notas:` y `![](imagen)` ajustan
  cada una. `main.py` arma las diapositivas según el tipo de pregunta.
- Las imágenes las dibuja `python herramientas/generar_imagenes.py`.
- Los sonidos de `assets/sonidos` son de [Kenney](https://www.kenney.nl)
  (paquetes Interface Sounds y Music Jingles, licencia CC0) y la fuente es
  Fredoka (SIL Open Font License, en `assets/fonts/OFL.txt`).
- En la vista previa juega un curso inventado de 18 estudiantes
  (`scene.rehearsal`), así que se ven la sala, las respuestas y el podio sin
  teléfonos.

## Previsualizar y presentar

Con un relay configurado (`gaanim relay`, ver la guía *Público en vivo*):

```sh
gaanim examples/la_gran_trivia
gaanim --present examples/la_gran_trivia
```

Para presentarla sin Python, grábala en un paquete:

```sh
gaanim export examples/la_gran_trivia --output trivia.gaanim
gaanim --present trivia.gaanim
```
