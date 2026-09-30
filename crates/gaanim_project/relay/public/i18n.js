// Texts of the relay pages in Spanish and English, chosen by the phone's
// language. Elements with data-t="<key>" get the text of <key>, and
// elements with data-tp="<key>" get it as their placeholder.
"use strict";

const TEXTS = {
  es: {
    joinTitle: "Únete a la presentación",
    joinHint: "Escribe el código que aparece en la pantalla.",
    codeLabel: "Código",
    joinButton: "Entrar",
    scanHint: "¿Tienes el QR? Escanéalo con la cámara.",
    badCode: "Los códigos tienen 6 letras o números, sin O, I, 0 ni 1.",
    waitTitle: "Esperando la pregunta",
    waitHint: "Deja esta página abierta: la pregunta aparecerá aquí.",
    offline: "Sin conexión. Reintentando…",
    sending: "Enviando…",
    sent: "¡Voto enviado!",
    change: "Puedes cambiarlo mientras la pregunta siga abierta.",
    closed: "La pregunta se cerró.",
    failed: "No se pudo enviar. Toca de nuevo.",
    sessionTitle: "Código de la presentación",
    gameTitle: "¡Empieza el juego!",
    gameHint: "Elige un apodo para competir.",
    nameLabel: "Apodo",
    namePlaceholder: "Tu apodo",
    gameButton: "Jugar",
    nameInvalid: "Usa de 2 a 20 letras, números o espacios.",
    nameTaken: "Ese apodo ya está en uso. Prueba otro.",
    locked: "¡Respuesta enviada! Espera el resultado.",
    timeUp: "¡Se acabó el tiempo!",
    seconds: "s",
    correct: "¡Correcto!",
    wrong: "Incorrecto",
    missed: "Sin respuesta",
    answerWas: "La respuesta era",
    points: "pts",
    place: (rank, players) => `Puesto ${rank} de ${players}`,
    total: "en total",
    kickedTitle: "Saliste del juego",
    kickedHint: "El presentador te quitó de esta sesión.",
  },
  en: {
    joinTitle: "Join the presentation",
    joinHint: "Type the code shown on the screen.",
    codeLabel: "Code",
    joinButton: "Join",
    scanHint: "Got the QR code? Scan it with your camera.",
    badCode: "Codes have 6 letters or digits, without O, I, 0 or 1.",
    waitTitle: "Waiting for the question",
    waitHint: "Keep this page open: the question will appear here.",
    offline: "Offline. Retrying…",
    sending: "Sending…",
    sent: "Vote sent!",
    change: "You can change it while the question is open.",
    closed: "The question closed.",
    failed: "Could not send. Tap again.",
    sessionTitle: "Presentation code",
    gameTitle: "The game is on!",
    gameHint: "Pick a nickname to play.",
    nameLabel: "Nickname",
    namePlaceholder: "Your nickname",
    gameButton: "Play",
    nameInvalid: "Use 2 to 20 letters, digits or spaces.",
    nameTaken: "That nickname is taken. Try another.",
    locked: "Answer sent! Wait for the result.",
    timeUp: "Time's up!",
    seconds: "s",
    correct: "Correct!",
    wrong: "Wrong",
    missed: "No answer",
    answerWas: "The answer was",
    points: "pts",
    place: (rank, players) => `Place ${rank} of ${players}`,
    total: "in total",
    kickedTitle: "You left the game",
    kickedHint: "The presenter removed you from this session.",
  },
};

const LANG = (navigator.languages?.[0] ?? navigator.language ?? "es")
  .toLowerCase()
  .startsWith("es")
  ? "es"
  : "en";

function t(key, ...args) {
  const value = TEXTS[LANG][key] ?? TEXTS.es[key] ?? key;
  return typeof value === "function" ? value(...args) : value;
}

/** A score with the phone's digit grouping, e.g. 1.745 or 1,745. */
function formatScore(score) {
  return new Intl.NumberFormat(LANG === "es" ? "es" : "en").format(score);
}

document.documentElement.lang = LANG;
document.addEventListener("DOMContentLoaded", () => {
  for (const element of document.querySelectorAll("[data-t]")) {
    element.textContent = t(element.dataset.t);
  }
  for (const element of document.querySelectorAll("[data-tp]")) {
    element.placeholder = t(element.dataset.tp);
  }
});
