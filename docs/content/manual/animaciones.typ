#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Animaciones y tiempo",
  description: "Entrada, transformación, easing, paralelismo y secuencia",
  route: "/manual/animaciones/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Descriptores de animación

Los métodos como `create`, `move`, `rotate` y `fade_out` devuelven un `Anim`.
Puedes configurar ese descriptor antes de entregarlo a la escena.

```python
entrance = orbit.animate.create().duration(1.2).smooth()
movement = point.animate.rotate_by(6.28318).about(0, 0).duration(4.0).linear()

scene.play([entrance])
scene.play([movement])
```

== Paralelo y secuencia

Las animaciones dentro de una misma lista comparten una llamada a `play`. Las
llamadas distintas son secuenciales. `lag` introduce un retraso progresivo
entre miembros del grupo.

== Duración y easing

`duration` usa segundos. El easing describe cómo progresa el valor: `linear`
mantiene velocidad constante, `smooth` suaviza entrada y salida y `spring`
produce una respuesta elástica. Elige el easing por significado, no solo por
ornamento.

== Transformaciones

Una transformación puede alterar una propiedad, convertir un objeto en otro o
reemplazarlo. Conserva handles distintos cuando la identidad posterior de cada
objeto importe. Para texto estructurado usa las transiciones específicas de
`Text`, que también notifican a Layout cuando cambia la medida.

== Depuración del tiempo

Cuando algo aparece demasiado pronto, revisa primero el orden de `play` y
`wait`; después las duraciones y `lag`. Las capturas por seek de regresión
visual permiten inspeccionar instantes exactos sin reproducir todo el video.

== Siguiente paso

Continúa con #link("/guides/layout/")[Layout] para sustituir coordenadas
manuales por una composición adaptable.
