#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Animaciones y tiempo",
  description: "Entrada, transformación, easing, paralelismo y secuencia",
  route: "/manual/animaciones/",
)

= Descriptores de animación

Los métodos de `.animate` como `create`, `shift_by`, `rotate_by` y `fade_out`
devuelven un `Anim`.
Puedes configurar ese descriptor antes de entregarlo a la escena.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>orbit = scene.geometry.circle(1.75).stroke(BLUE, 0.05).no_fill()
>>>point = scene.geometry.dot(0.15).fill(YELLOW).move_to(1.75, 0)
entrance = orbit.animate.create().duration(1.2).easing(Easing.SMOOTH)
movement = point.animate.rotate_by(6.28318).about_point(0, 0).duration(4.0).easing(Easing.LINEAR)

scene.play([entrance])
scene.play([movement])
```

== Paralelo y secuencia

Las animaciones dentro de una misma lista comparten una llamada a `play`. Las
llamadas distintas son secuenciales. `stagger(a, b, ..., each=0.1)` introduce un
retraso progresivo entre los miembros del grupo.

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
`wait`; después las duraciones y el `each` de `stagger`. Las capturas por seek de regresión
visual permiten inspeccionar instantes exactos sin reproducir todo el video.

== Siguiente paso

Continúa con #link("/guides/layout/")[Layout] para sustituir coordenadas
manuales por una composición adaptable.
