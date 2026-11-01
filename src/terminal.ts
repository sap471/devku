import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

let terminal: Terminal | null = null;
let fitAddon: FitAddon | null = null;
let sessionId: number | null = null;
let unlisten: UnlistenFn[] = [];

export function isOpen(): boolean {
  return terminal !== null;
}

export async function open(cwd: string | null, targetHost?: HTMLElement | null): Promise<void> {
  const host = targetHost ?? document.getElementById("terminal");
  if (!host) return;

  if (terminal) {
    fitAddon?.fit();
    terminal.focus();
    return;
  }

  const instance = new Terminal({
    fontFamily: 'ui-monospace, "Cascadia Code", "JetBrains Mono", Consolas, monospace',
    fontSize: 12,
    cursorBlink: true,
    theme: themeColors(),
  });
  const fit = new FitAddon();
  instance.loadAddon(fit);
  instance.open(host);
  fit.fit();

  terminal = instance;
  fitAddon = fit;

  const id = await invoke<number>("open_terminal", {
    cwd,
    cols: instance.cols,
    rows: instance.rows,
  });
  sessionId = id;

  unlisten.push(
    await listen<string>(`pty://${id}`, (event) => {
      instance.write(event.payload);
    }),
  );
  unlisten.push(
    await listen(`pty://${id}/exit`, () => {
      instance.write("\r\n[shell berhenti]\r\n");
    }),
  );

  instance.onData((data) => {
    void invoke("write_terminal", { id, data });
  });
  instance.onResize(({ cols, rows }) => {
    void invoke("resize_terminal", { id, cols, rows });
  });

  window.addEventListener("resize", refit);
  instance.focus();
}

export async function close(): Promise<void> {
  window.removeEventListener("resize", refit);
  unlisten.forEach((fn) => fn());
  unlisten = [];

  if (sessionId !== null) {
    try {
      await invoke("close_terminal", { id: sessionId });
    } catch {
      // Shell might have exited already.
    }
  }

  sessionId = null;
  terminal?.dispose();
  terminal = null;
  fitAddon = null;
}

export function toggle(cwd: string | null, targetHost?: HTMLElement | null): void {
  if (terminal) {
    void close();
  } else {
    void open(cwd, targetHost);
  }
}

export async function run(cwd: string | null, command: string, targetHost?: HTMLElement | null): Promise<void> {
  const fresh = !isOpen();
  await open(cwd, targetHost);

  const id = sessionId;
  if (id === null) return;

  if (fresh) {
    // PowerShell needs a brief moment after spawn before accepting input lines.
    await new Promise((resolve) => window.setTimeout(resolve, 900));
  }

  const cd = cwd ? `Set-Location -LiteralPath '${cwd.replace(/'/g, "''")}'` : "";
  const data = cd ? `${cd}\r${command}\r` : `${command}\r`;

  await invoke("write_terminal", { id, data });
  terminal?.focus();
}

export function refit(): void {
  fitAddon?.fit();
}

function themeColors() {
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  return dark
    ? {
        background: "#0c0d0f",
        foreground: "#e7e7ea",
        cursor: "#f59e0b",
        selectionBackground: "rgba(245, 158, 11, 0.25)",
      }
    : {
        background: "#ffffff",
        foreground: "#18181b",
        cursor: "#b45309",
        selectionBackground: "rgba(180, 83, 9, 0.2)",
      };
}
