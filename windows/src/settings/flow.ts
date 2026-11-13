// Fork: Flow dictation's settings and « OK Coucou », drawn as cards of this
// window's layout. Same controls and wiring as the Coucou window's own pages
// (../hub/pages/flowsettings.ts and voice.ts): every change goes through the
// window's HubContext, which saves it with FlowBridge.

import { h, clear } from "../views/dom";
import { t } from "../i18n/i18n";
import { Bridge } from "../core/bridge";
import { FlowBridge, flowKeyId, type Edge, type FlowKey, type VoiceOutput } from "../core/flow";
import type { HubContext } from "../hub/shell";
import { keyKeycap, keycap } from "../hub/ui";
import { modelsControl, shortcutRecorder } from "../hub/pages/flowsettings";
import { pageSave } from "../hub/save";
import { card, item, segmented, toggle } from "./parts";

const ELEVEN = "elevenlabs-api-key";
const KEY_STORE = navigator.userAgent.includes("Windows") ? "Windows Credential Manager" : "Secret Service (GNOME Keyring, KWallet)";

function select<T extends string>(options: [T, string][], value: T, onChange: (v: T) => void): HTMLSelectElement {
  const el = h("select", {}) as HTMLSelectElement;
  for (const [id, text] of options) el.append(h("option", { value: id, text }));
  el.value = value;
  el.addEventListener("change", () => onChange(el.value as T));
  return el;
}

function seg<T extends string>(label: string, options: [T, string][], value: T, onChange: (v: T) => void): HTMLElement {
  const s = segmented(label, options, (v) => onChange(v as T));
  s.set(value);
  return s.el;
}

function button(text: string, onclick: () => void, cls = ""): HTMLButtonElement {
  return h("button", { class: cls, text, onclick }) as HTMLButtonElement;
}

const note = (text: string) => h("div", { class: "hint", text });

/** Settings → Dictation: the key, the pill, the microphone, Parakeet, the Notetaker and the history. */
export function flowCards(ctx: HubContext): HTMLElement[] {
  const { status } = ctx;
  const prefs = status.prefs;
  const keys: [FlowKey, string][] = [
    ["right_ctrl", t("Right {0}", { 0: "Ctrl" })],
    ["right_alt", t("Right {0}", { 0: "Alt" })],
    ["right_shift", t("Right {0}", { 0: "Shift" })],
    ["left_ctrl", t("Left {0}", { 0: "Ctrl" })],
    ["left_alt", t("Left {0}", { 0: "Alt" })],
    ["left_shift", t("Left {0}", { 0: "Shift" })],
    ["left_ctrl_super", status.platform === "windows" ? "Ctrl + Win" : "Ctrl + Super"],
    ["custom", t("A shortcut of your own…")],
  ];
  const edges: [Edge, string][] = [["bottom", t("Bottom")], ["bottom-left", t("Bottom-left")], ["bottom-right", t("Bottom-right")], ["left", t("Left")], ["right", t("Right")]];
  const wayland = status.keys === "wayland";
  // Fork: turning dictation on and the Parakeet download live on Flow's
  // Dictation tab only, so they aren't here twice.
  const keysCard = card(t("Keys"), null,
    wayland
      ? item(t("Hold to dictate"), null, keyKeycap("wayland", status.platform))
      : item(t("Hold to dictate"), prefs.key === "right_alt" ? t("On a keyboard with AltGr, the right Alt types characters: pick another key.") : null,
        select(keys, prefs.key, (key) => void ctx.setPrefs((p) => (p.key = key)).then(() => ctx.navigate("flowsettings")))),
    // A shortcut of one's own: its keys, and the button that records new ones.
    prefs.key === "custom" && !wayland
      ? item(t("Your shortcut"), t("Hold the keys together to dictate. Its letter no longer reaches the app in front while you hold them."),
        shortcutRecorder(prefs.keyCombo, status.platform, (combo) => void ctx.setPrefs((p) => {
          p.key = "custom";
          p.keyCombo = combo;
        }).then(() => ctx.navigate("flowsettings"))))
      : null,
    item(t("Where the pill is"), null, select(edges, prefs.edge, (edge) => void ctx.setPrefs((p) => (p.edge = edge)))),
    item(t("Hide the pill until I dictate"), t("It shows only while you dictate, record a call or get a message. The dictation key still works."),
      toggle(prefs.hidePill, (on) => void ctx.setPrefs((p) => (p.hidePill = on)))),
    item(t("Hands-free"),
      prefs.oneTapHandsFree ? t("Tap once, speak as long as you like, tap again to paste.") : t("Tap twice, speak as long as you like, tap once to paste."),
      keyKeycap(wayland ? "wayland" : flowKeyId(prefs), status.platform, !prefs.oneTapHandsFree)),
    item(t("Hands-free with one press"), null,
      toggle(prefs.oneTapHandsFree, (on) => void ctx.setPrefs((p) => (p.oneTapHandsFree = on)).then(() => ctx.navigate("flowsettings")))),
    item(t("Live transcription"), t("Words appear in the pill as you speak, and the paste comes faster when you let go. Uses more of the processor while you talk."),
      toggle(prefs.stream, (on) => void ctx.setPrefs((p) => (p.stream = on)))),
    item(t("Paste the last dictation again"), t("When the words went to the wrong place."), button(t("Paste again"), () => void FlowBridge.pasteAgain())),
    item(t("Cancel a dictation"), null, keycap(t("Esc"))),
    wayland
      ? h("div", { class: "notice warn", text: t("On Wayland, the desktop holds the dictation key for Coucou: Ctrl+Alt+V, or the keys you picked when it asked. The first paste asks once to let Coucou type for you.") })
      : status.keys !== "ok"
        ? h("div", { class: "notice warn", text: t("Coucou can't watch the keyboard here: use the pill's microphone to dictate.") })
        : null,
  );

  const mics: [string, string][] = [["", t("System default")], ...status.mics.map((m): [string, string] => [m.name, m.name])];
  if (prefs.mic && !mics.some(([id]) => id === prefs.mic)) mics.push([prefs.mic, prefs.mic]);

  const confirmRow = h("div", { class: "row" });
  const keep: [string, string][] = [["0", t("Don't keep")], ["7", t("7 days")], ["30", t("30 days")], ["365", t("1 year")], ["-1", t("Forever")]];
  const privacy = card(t("Privacy"), null,
    item(t("Keep the history"), null, select(keep, String(prefs.keepDays), (v) => void ctx.setPrefs((p) => (p.keepDays = Number(v))))),
    confirmRow,
    note(t("The history, the statistics and your notes stay on this computer, in Coucou's folder. Dictations are never saved as audio; a call's audio is deleted as soon as its transcript is written.")),
  );
  const saveErase = pageSave(privacy);
  const erase = () => {
    clear(confirmRow);
    confirmRow.append(
      button(t("Erase the history"), () => void saveErase(() => FlowBridge.clear(false))),
      button(t("Erase everything"), () => {
        clear(confirmRow);
        confirmRow.append(
          h("span", { class: "hint", style: "flex:1", text: t("Erase the history and the statistics?") }),
          button(t("Erase everything"), () => {
            void saveErase(async () => {
              await FlowBridge.clear(true);
              erase();
            });
          }, "danger"),
          button(t("Cancel"), erase),
        );
      }),
    );
  };
  erase();

  return [
    keysCard,
    card(t("Microphone"), null,
      item(t("Microphone"), t("The one Coucou listens to for dictation and calls."), select(mics, prefs.mic, (mic) => void ctx.setPrefs((p) => (p.mic = mic)))),
      // Silence while you speak (flow/hush.rs): Windows mutes each app on its own.
      status.platform === "windows"
        ? item(t("Mute the computer while you speak"),
          t("Videos, music and calls go quiet while Coucou listens to you, then come back. Coucou's own sounds keep playing."),
          toggle(prefs.muteOthers, (on) => void ctx.setPrefs((p) => (p.muteOthers = on))))
        : null),
    card(t("Notetaker"), null,
      item(t("Summaries"), t("With Claude, the transcript (never the audio) goes to Anthropic with your key from Notch → Chat."),
        select([["claude", t("Claude (your key)")], ["off", t("No summary")]], prefs.summaryEngine, (engine) => void ctx.setPrefs((p) => (p.summaryEngine = engine)))),
      note(t("Everything stays on this computer. Tell the people in the call that you are recording.")),
    ),
    privacy,
  ];
}

/** Settings → General: whether this window opens with Coucou. */
export function openAtLaunchItem(ctx: HubContext): HTMLElement {
  return item(t("Open this window when you open Coucou"), t("Not when Coucou starts with the computer."),
    toggle(ctx.status.prefs.openAtLaunch, (on) => void ctx.setPrefs((p) => (p.openAtLaunch = on))));
}

function elevenKey(): HTMLElement {
  const box = h("div", { class: "row" });
  const draw = async () => {
    const present = (await Bridge.secretPresent(ELEVEN)) ?? false;
    clear(box);
    if (present) {
      box.append(
        h("span", { class: "hint", text: `✓ ${t("Key saved in the {store}.", { store: KEY_STORE })}` }),
        button(t("Remove"), () => void Bridge.secretClear(ELEVEN).then(draw), "danger"),
      );
      return;
    }
    const input = h("input", { type: "password", placeholder: t("ElevenLabs key"), autocomplete: "off", spellcheck: "false", style: "width:220px" }) as HTMLInputElement;
    const save = button(t("Save key"), () => {
      const value = input.value.trim();
      if (!value) return;
      void Bridge.secretSet(ELEVEN, value).then(draw, () => input.classList.add("err"));
    }, "primary");
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") save.click();
    });
    box.append(input, save);
  };
  void draw();
  return box;
}

/** Settings → OK Coucou: listening, the answers out loud, what leaves the computer. */
export function voiceCards(ctx: HubContext): HTMLElement[] {
  const { status } = ctx;
  const prefs = status.prefs;
  const ready = status.models.parakeet.state === "ready";
  const outputs: [VoiceOutput, string][] = [["system", t("System voice")], ["elevenlabs", "ElevenLabs"], ["off", t("Text only")]];
  return [
    card(t("Listening"), t("Say « OK Coucou », then ask anything: add a pill, play music, the weather, an email to prepare."),
      item(t("Listen for « OK Coucou »"), t("The microphone stays open; only what follows « OK Coucou » goes further."),
        toggle(prefs.voice, (on) => void ctx.setPrefs((p) => (p.voice = on)).then(() => ctx.navigate("voice")))),
      item(t("Parakeet, on your computer"), t("Hears « OK Coucou » here, without the internet."), modelsControl(ctx, () => ctx.navigate("voice"))),
      prefs.voice && !status.hasClaudeKey
        ? h("div", { class: "notice warn", text: t("Coucou answers with Claude: add your Claude key in Notch → Chat.") })
        : prefs.voice && !ready
          ? h("div", { class: "notice warn", text: t("Download the voice model so Coucou can hear you.") })
          : null,
    ),
    card(t("Answers"), null,
      item(t("Coucou answers out loud"), null,
        seg(t("Coucou answers out loud"), outputs, prefs.voiceOutput, (v) => void ctx.setPrefs((p) => (p.voiceOutput = v)).then(() => ctx.navigate("voice")))),
      prefs.voiceOutput === "elevenlabs"
        ? item(t("Voice"), null, seg(t("Voice"), [["female", t("Female")], ["male", t("Male")]], prefs.voiceGender, (g) => void ctx.setPrefs((p) => (p.voiceGender = g))))
        : null,
      prefs.voiceOutput === "elevenlabs" ? item(t("ElevenLabs key"), null, elevenKey()) : null,
    ),
    card(t("Privacy"), null,
      note(t("Your voice never leaves this computer: Parakeet turns it into text here. Only the words after « OK Coucou » go to Anthropic with your Claude key, and to ElevenLabs if you picked it for the answers.")),
      note(t("An email is never sent by Coucou: your mail app opens with it written, and you send it.")),
    ),
  ];
}
