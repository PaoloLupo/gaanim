# gaanim

Autocompletado, tipos y documentación para escribir escenas de
[Gaanim](https://paololupo.github.io/gaanim/), un motor de animación vectorial
acelerado por GPU que se programa en Python.

**Este paquete no ejecuta escenas.** Gaanim corre dentro de la aplicación
`gaanim`, que trae el motor, la vista previa y la exportación. El paquete solo
instala, para el entorno de tu proyecto:

- los stubs (`.pyi`) y `py.typed`, para que tu editor y tu verificador de
  tipos conozcan la API;
- helpers en Python puro;
- la documentación de la misma versión como fuentes Typst (`gaanim/_docs`),
  para asistentes de código.

Fuera de la aplicación, `import gaanim` lanza `RuntimeError` a propósito.

## Empezar

1. Descarga la aplicación desde las
   [versiones publicadas](https://github.com/PaoloLupo/gaanim/releases/latest).
2. Crea un proyecto y ábrelo:

   ```bash
   gaanim init video intro
   gaanim intro
   ```

`gaanim init` crea el `pyproject.toml` con este paquete fijado a la versión de
la aplicación, y la aplicación lo instala en `.venv` al abrir el proyecto.
`uv sync` instala la misma versión desde PyPI.

Para probar Gaanim sin instalar nada, usa el
[playground](https://paololupo.github.io/gaanim/playground/).

## Licencia

MIT o Apache-2.0, a tu elección.
