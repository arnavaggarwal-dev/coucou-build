// Small pieces every settings section draws with.

import { h } from "../views/dom";
import { t } from "../i18n/i18n";

export function statusDot(ok: boolean): HTMLElement {
  return h("i", { class: "dot", style: `background:${ok ? "#22c55e" : "#f4505e"}` });
}

export function renderDiff(text: string): HTMLElement {
  const box = h("div", { class: "diff" });
  for (const line of text.split("\n")) {
    const cls = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "ctx";
    box.append(h("div", { class: cls, text: line }));
  }
  return box;
}

/** An on/off switch, announced as one. */
export function toggle(on: boolean, onChange: (v: boolean) => void): HTMLButtonElement {
  const el = h("button", { class: on ? "switch on" : "switch", role: "switch", "aria-checked": String(on) });
  el.addEventListener("click", () => {
    const next = !el.classList.contains("on");
    el.classList.toggle("on", next);
    el.setAttribute("aria-checked", String(next));
    onChange(next);
  });
  return el;
}

type Part = Node | string | null | undefined | false;

/** A card: its heading (none when ""), an optional description, then its settings. */
export function card(title: string, desc: string | null, ...children: Part[]): HTMLElement {
  return h("section", {},
    title ? h("h2", {}, h("span", { text: title })) : null,
    desc ? h("div", { class: "hint", text: desc }) : null,
    ...children,
  );
}

let uid = 0;

/**
 * One setting: its name and a short description on the left, its control on
 * the right. Switches, fields and lists without a label of their own are
 * labelled by the name.
 */
export function item(label: string, desc: string | Node | null, ...controls: Part[]): HTMLElement {
  const id = `setting-${++uid}`;
  for (const c of controls) {
    if (!(c instanceof HTMLElement) || c.hasAttribute("aria-label") || c.hasAttribute("aria-labelledby")) continue;
    if (c.classList.contains("switch") || /^(INPUT|SELECT)$/.test(c.tagName)) c.setAttribute("aria-labelledby", id);
  }
  const about = typeof desc === "string" ? h("div", { class: "item-desc", text: desc }) : desc;
  if (about instanceof HTMLElement) about.classList.add("item-desc");
  return h("div", { class: "item" },
    h("div", { class: "item-text" }, h("div", { class: "item-label", id, text: label }), about),
    h("div", { class: "item-ctl" }, ...controls),
  );
}

/** What most people never change, folded away under a labelled disclosure. */
export function disclosure(summary: string, ...children: Part[]): HTMLDetailsElement {
  return h("details", { class: "disclosure" },
    h("summary", { text: summary }),
    h("div", { class: "disclosure-body" }, ...children),
  );
}

export function advanced(...children: Part[]): HTMLDetailsElement {
  return disclosure(t("Advanced"), ...children);
}

/** Two to four choices side by side: native radios, so arrow keys move between them. */
export function segmented(
  label: string,
  options: [value: string, text: string][],
  onChange: (value: string) => void,
): { el: HTMLElement; set(value: string): void } {
  const name = `seg-${++uid}`;
  const inputs = options.map(([value]) => {
    const input = h("input", { type: "radio", name, value }) as HTMLInputElement;
    input.addEventListener("change", () => input.checked && onChange(value));
    return input;
  });
  const el = h("div", { class: "segmented", role: "radiogroup", "aria-label": label },
    ...options.map(([, text], i) => h("label", {}, inputs[i], h("span", { text }))),
  );
  return { el, set: (value) => inputs.forEach((i) => (i.checked = i.value === value)) };
}
