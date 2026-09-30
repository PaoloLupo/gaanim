// Texts of the relay pages in Spanish and English, chosen by the phone's
// language. Elements with data-t="<key>" get the text of <key>.
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
  },
};

const LANG = (navigator.languages?.[0] ?? navigator.language ?? "es")
  .toLowerCase()
  .startsWith("es")
  ? "es"
  : "en";

function t(key) {
  return TEXTS[LANG][key] ?? TEXTS.es[key] ?? key;
}

document.documentElement.lang = LANG;
document.addEventListener("DOMContentLoaded", () => {
  for (const element of document.querySelectorAll("[data-t]")) {
    element.textContent = t(element.dataset.t);
  }
});
