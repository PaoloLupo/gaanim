"""Questions written outside Python: a Markdown file or a spreadsheet (CSV).

``load_questions(path)`` reads either and returns :class:`Question` objects;
``scene.question(q)`` opens each as a quiz (it has right answers) or a poll.

Markdown, one question per heading::

    # ¿Cuál es la capital de Australia?
    ![](imagenes/australia.png)
    tiempo: 20

    - Sídney
    - [x] Canberra
    - Melbourne
    - Perth

Answers are list items; ``[x]`` marks the right ones (several make it
multiple choice) and ``[ ]`` or nothing the others; without any ``[x]`` it is a
poll. ``key: value`` lines set ``tiempo``/``time``, ``puntos``/``points``,
``modo``/``mode`` (``una``/``single`` or ``varias``/``multiple``),
``imagen``/``image``, ``acierta``/``right`` (the share the rehearsal answers
right), ``votos``/``votes`` (rehearsal weights, one per answer) and
``notas``/``notes``. Other text is ignored.

CSV, one question per row, with a header naming the columns (Spanish or
English; Kahoot's spreadsheet template works): ``pregunta``/``question``,
``respuesta 1``…``respuesta 6``/``answer 1``…, ``correcta``/``correct``
(the right answers' numbers from 1, like ``2`` or ``1, 3``; empty for a poll),
and optionally ``tiempo``, ``puntos``, ``imagen``, ``modo``, ``acierta``,
``votos`` and ``notas``.

A relative ``path`` is relative to the script that loads it; image paths
are relative to the questions file.
"""

from __future__ import annotations

import csv
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional, Union

__all__ = ["Question", "QuestionError", "load_questions"]

MAX_ANSWERS = 6


class QuestionError(ValueError):
    """A questions file that cannot be read, with the file and line."""


@dataclass
class Question:
    """One question, as ``scene.question`` opens it."""

    text: str
    options: list = field(default_factory=list)
    #: The right answers' indexes, from 0; empty for a poll.
    correct: list = field(default_factory=list)
    #: Players may choose several answers.
    multiple: bool = False
    time: int = 20
    points: int = 1000
    #: An absolute path to the picture phones show, or None.
    image: Optional[str] = None
    #: How the rehearsal answers: a share that answers right, or weights.
    rehearse: Union[float, list, None] = None
    notes: str = ""

    @property
    def is_quiz(self) -> bool:
        return bool(self.correct)


_KEYS = {
    "tiempo": "time", "time": "time", "segundos": "time", "time limit": "time",
    "puntos": "points", "points": "points",
    "modo": "mode", "mode": "mode", "tipo": "mode", "type": "mode",
    "imagen": "image", "image": "image", "foto": "image",
    "acierta": "right", "aciertan": "right", "right": "right",
    "votos": "votes", "votes": "votes", "pesos": "votes", "weights": "votes",
    "notas": "notes", "nota": "notes", "notes": "notes",
}
_MULTIPLE = {"varias": True, "multiple": True, "múltiple": True, "multiple choice": True,
             "una": False, "single": False, "única": False, "unica": False}


def load_questions(path: Union[str, Path]) -> list:
    """The questions of a ``.md`` (or ``.txt``) or ``.csv`` file.

    Raises :class:`QuestionError`, naming the file and line, for a question
    without answers, more than 6 answers, an unknown setting or a value
    that is not a number.
    """
    path = Path(path)
    if not path.is_absolute():
        # Relative to the script that asks, as scene pictures are.
        caller = Path(sys._getframe(1).f_code.co_filename).parent
        path = caller / path if (caller / path).exists() else path
    path = path.resolve()
    if path.suffix.lower() == ".csv":
        return _load_csv(path)
    return _load_markdown(path)


def _fail(path: Path, line: int, message: str) -> QuestionError:
    return QuestionError(f"{path.name}, line {line}: {message}")


def _number(text: str, path: Path, line: int, what: str, kind=float):
    try:
        return kind(text.strip().replace(",", ".") if kind is float else text.strip())
    except ValueError:
        raise _fail(path, line, f"{what} must be a number, got {text!r}") from None


def _setting(question: Question, key: str, value: str, path: Path, line: int) -> None:
    value = value.strip()
    if key == "time":
        question.time = _number(value, path, line, "tiempo", int)
    elif key == "points":
        question.points = _number(value, path, line, "puntos", int)
    elif key == "mode":
        mode = value.lower()
        if mode not in _MULTIPLE:
            raise _fail(path, line, f"modo is 'una' or 'varias', got {value!r}")
        question.multiple = _MULTIPLE[mode]
    elif key == "image":
        question.image = str((path.parent / value).resolve()) if value else None
    elif key == "right":
        share = _number(value.rstrip("%"), path, line, "acierta")
        question.rehearse = share / 100 if value.endswith("%") or share > 1 else share
    elif key == "votes":
        question.rehearse = [
            _number(part, path, line, "votos") for part in re.split(r"[,;\s]+", value) if part
        ]
    elif key == "notes":
        question.notes = value


def _finish(question: Question, path: Path, line: int) -> Question:
    if len(question.options) < 2:
        raise _fail(path, line, f"{question.text!r} needs at least 2 answers")
    if len(question.options) > MAX_ANSWERS:
        raise _fail(path, line, f"{question.text!r} has more than {MAX_ANSWERS} answers")
    if len(question.correct) > 1:
        question.multiple = True
    return question


_HEADING = re.compile(r"^#{1,6}\s+(.*\S)\s*$")
_ANSWER = re.compile(r"^\s*(?:[-*+]|\d+[.)])\s+(?:\[([ xX✓✔])\]\s*)?(.*\S)\s*$")
_IMAGE = re.compile(r"^\s*!\[[^\]]*\]\(([^)]+)\)\s*$")
_KEY = re.compile(r"^\s*([A-Za-zÁÉÍÓÚáéíóúñÑ ]+?)\s*:\s*(.*)$")


def _load_markdown(path: Path) -> list:
    try:
        lines = path.read_text(encoding="utf-8-sig").splitlines()
    except OSError as error:
        raise QuestionError(f"could not read {path}: {error}") from None
    questions = []
    current: Optional[Question] = None
    start = 0
    for number, line in enumerate(lines, start=1):
        heading = _HEADING.match(line)
        if heading:
            if current is not None:
                questions.append(_finish(current, path, start))
            current, start = Question(heading.group(1)), number
            continue
        if current is None:
            continue
        answer = _ANSWER.match(line)
        if answer:
            mark, text = answer.groups()
            if mark and mark.strip():
                current.correct.append(len(current.options))
            current.options.append(text)
            continue
        image = _IMAGE.match(line)
        if image:
            _setting(current, "image", image.group(1), path, number)
            continue
        key = _KEY.match(line)
        if key and key.group(1).strip().lower() in _KEYS:
            _setting(current, _KEYS[key.group(1).strip().lower()], key.group(2), path, number)
    if current is not None:
        questions.append(_finish(current, path, start))
    if not questions:
        raise QuestionError(f"{path.name} has no questions: start each with a # heading")
    return questions


def _column(header: str) -> Optional[str]:
    """What a CSV column holds: ``text``, ``answer N``, ``correct`` or a setting."""
    name = header.strip().lower()
    name = re.split(r"\s+-\s+|\(", name)[0].strip()  # "Answer 1 - max 75 characters"
    if name.startswith(("pregunta", "question")):
        return "text"
    answer = re.match(r"(?:respuesta|answer|opci[oó]n|option|alternativa)\s*(\d)", name)
    if answer:
        return f"answer {answer.group(1)}"
    if name.startswith(("correct", "respuesta correcta")):
        return "correct"
    return _KEYS.get(name)


def _load_csv(path: Path) -> list:
    try:
        text = path.read_text(encoding="utf-8-sig")
    except OSError as error:
        raise QuestionError(f"could not read {path}: {error}") from None
    dialect = csv.Sniffer().sniff(text.splitlines()[0] if text else ",", delimiters=",;\t")
    rows = list(csv.reader(text.splitlines(), dialect))
    if not rows:
        raise QuestionError(f"{path.name} is empty")
    columns = [_column(header) for header in rows[0]]
    if "text" not in columns:
        raise QuestionError(f"{path.name}: the first row must name a 'pregunta' column")
    questions = []
    for number, row in enumerate(rows[1:], start=2):
        cells = {column: cell.strip() for column, cell in zip(columns, row) if column}
        if not cells.get("text"):
            continue
        question = Question(cells["text"])
        answers = sorted(
            (int(column.split()[1]), cell)
            for column, cell in cells.items()
            if column.startswith("answer ") and cell
        )
        question.options = [cell for _, cell in answers]
        for part in re.split(r"[,;\s]+", cells.get("correct", "")):
            if part:
                index = _number(part, path, number, "correcta", int) - 1
                if not 0 <= index < len(question.options):
                    raise _fail(path, number, f"correcta {part} is not one of the answers")
                question.correct.append(index)
        for key in ("time", "points", "mode", "image", "right", "votes", "notes"):
            if cells.get(key):
                _setting(question, key, cells[key], path, number)
        questions.append(_finish(question, path, number))
    return questions
