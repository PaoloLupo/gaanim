// Join page: type the six-character code shown by the presentation.
"use strict";

const INVALID = /[^A-HJ-NP-Z2-9]/;
const ALLOWED = new RegExp(INVALID.source, "g");
const CODE = /^[A-HJ-NP-Z2-9]{6}$/;

document.addEventListener("DOMContentLoaded", () => {
  const form = document.getElementById("join");
  const input = document.getElementById("code");
  const go = document.getElementById("go");
  const error = document.getElementById("join-error");

  const clean = (value) => value.toUpperCase().replace(/\s+/g, "").replace(ALLOWED, "");

  input.addEventListener("input", () => {
    const raw = input.value.toUpperCase().replace(/\s+/g, "");
    const value = clean(raw).slice(0, 6);
    // Characters that are never in a code (O, I, 0, 1, symbols) are dropped;
    // the hint stays until the code is complete.
    if (INVALID.test(raw)) error.textContent = t("badCode");
    input.value = value;
    go.disabled = !CODE.test(value);
    if (!go.disabled) error.textContent = "";
  });

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const value = clean(input.value);
    if (!CODE.test(value)) {
      error.textContent = t("badCode");
      input.focus();
      return;
    }
    location.assign(`/s/${value}`);
  });

  // /?code=ABC234 opens the session directly.
  const preset = clean(new URLSearchParams(location.search).get("code") ?? "");
  if (CODE.test(preset)) {
    location.replace(`/s/${preset}`);
    return;
  }
  input.focus();
});
