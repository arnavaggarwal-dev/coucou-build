// Fork: the glow round the pointer while Flow records (flow/glow.rs moves the
// window onto the pointer). It breathes on its own and swells with the voice,
// from the same nine levels the pill's dots use.

import { onEvent } from "../core/bridge";

const style = document.createElement("style");
style.textContent = `
  html, body { margin: 0; height: 100%; background: transparent; overflow: hidden; }
  #glow {
    position: absolute; left: 50%; top: 50%; width: 64px; height: 64px;
    margin: -32px 0 0 -32px; border-radius: 50%;
    background: radial-gradient(circle, rgba(250, 45, 72, 0.55) 0%, rgba(250, 45, 72, 0.22) 45%, rgba(250, 45, 72, 0) 70%);
    opacity: 0; transform: scale(0.6);
    transition: opacity 0.25s ease, transform 0.12s ease-out;
    animation: breathe 1.6s ease-in-out infinite;
    animation-play-state: paused;
  }
  #glow.on { opacity: 1; animation-play-state: running; }
  @keyframes breathe { 50% { filter: brightness(1.35); } }
`;
document.head.append(style);

const glow = document.getElementById("glow")!;
let on = false;

void onEvent<boolean>("glow-on", (value) => {
  on = value;
  glow.classList.toggle("on", on);
  glow.style.transform = on ? "scale(1)" : "scale(0.6)";
});

// Louder speech, a bigger glow.
void onEvent<number[]>("pill-bands", (levels) => {
  if (!on || !levels.length) return;
  const level = Math.min(1, levels.reduce((a, b) => a + b, 0) / levels.length);
  glow.style.transform = `scale(${(1 + level * 0.6).toFixed(3)})`;
});
