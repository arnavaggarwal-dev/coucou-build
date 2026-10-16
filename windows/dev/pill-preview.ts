// The dictation pill in a plain browser: Tauri is faked and `pill-state`
// events are sent from here. `npx vite`, then open /dev/pill-preview.html
// sized like the pill's window (520x170 at the bottom, 480x320 on a side);
// ?edge=right|left|bottom&look=toolbar. Not shipped.

const handlers = new Map<number, (e: unknown) => void>();
const listeners = new Map<string, number[]>();
let next = 1;
(window as any).__TAURI_INTERNALS__ = {
  metadata: { currentWindow: { label: "pill" }, currentWebview: { windowLabel: "pill", label: "pill" } },
  transformCallback: (fn: (e: unknown) => void) => {
    handlers.set(next, fn);
    return next++;
  },
  unregisterCallback: () => {},
  convertFileSrc: (p: string) => p,
  invoke: async (cmd: string, args: Record<string, any> = {}) => {
    if (cmd === "plugin:event|listen") {
      listeners.set(args.event, [...(listeners.get(args.event) ?? []), args.handler]);
      return args.handler;
    }
    (window as any).calls = [...((window as any).calls ?? []), cmd];
    return null;
  },
};
(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
(window as any).emit = (event: string, payload: unknown) =>
  (listeners.get(event) ?? []).forEach((id) => handlers.get(id)?.({ event, id, payload }));

await import("../src/pill/main");

const params = new URLSearchParams(location.search);
const edge = params.get("edge") ?? "right";
setTimeout(() => {
  (window as any).emit("pill-visible", true);
  (window as any).emit("pill-state", { phase: "idle", edge, key: "right_ctrl", noteStartedAt: null, announcement: null, enabled: true, voice: null });
}, 200);
