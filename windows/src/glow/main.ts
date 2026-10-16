// Fork: a glow traced round the pointer's own outline while Flow records
// (flow/glow.rs keeps the window centred on the pointer's tip and sends the
// pointer's image whenever its shape changes). The copy sits exactly under the
// real pointer, so only its glowing outline shows. It breathes on its own and
// grows with the voice, from the same nine levels the pill's dots use.

import { onEvent } from "../core/bridge";

interface CursorImage {
  src: string;
  hotX: number;
  hotY: number;
  width: number;
  height: number;
}

const style = document.createElement("style");
style.textContent = `
  html, body { margin: 0; height: 100%; background: transparent; overflow: hidden; }
  #glow {
    position: absolute; left: 50%; top: 50%;
    opacity: 0; transition: opacity 0.25s ease;
    --spread: 2px;
    filter:
      drop-shadow(0 0 var(--spread) rgba(250, 45, 72, 1))
      drop-shadow(0 0 calc(var(--spread) * 2) rgba(250, 45, 72, 0.75));
    animation: breathe 1.6s ease-in-out infinite;
    animation-play-state: paused;
  }
  #glow.on { opacity: 1; animation-play-state: running; }
  @keyframes breathe { 50% { opacity: 0.7; } }
`;
document.head.append(style);

const glow = document.createElement("img");
glow.id = "glow";
glow.alt = "";
document.body.append(glow);
let on = false;

void onEvent<CursorImage>("glow-cursor", (c) => {
  // The image is in physical pixels; the page lays out in CSS pixels.
  const s = window.devicePixelRatio || 1;
  glow.src = c.src;
  glow.style.width = `${c.width / s}px`;
  glow.style.height = `${c.height / s}px`;
  glow.style.marginLeft = `${-c.hotX / s}px`;
  glow.style.marginTop = `${-c.hotY / s}px`;
});

void onEvent<boolean>("glow-on", (value) => {
  on = value;
  glow.classList.toggle("on", on);
});

// Louder speech, a wider glow.
void onEvent<number[]>("pill-bands", (levels) => {
  if (!on || !levels.length) return;
  const level = Math.min(1, levels.reduce((a, b) => a + b, 0) / levels.length);
  glow.style.setProperty("--spread", `${(2 + level * 4).toFixed(1)}px`);
});
