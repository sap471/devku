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

export async function open(cwd: string | null): Promise<void> {
  const pane = document.getElementById("terminal-pane");
  const host = document.getElementById("terminal");
  if (!pane || !host) return;

  pane.hidden = false;
  document.getElementById("terminal-toggle")?.setAttribute("aria-expanded", "true");

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
  const pane = document.getElementById("terminal-pane");
  if (pane) pane.hidden = true;
  document.getElementById("terminal-toggle")?.setAttribute("aria-expanded", "false");

  window.removeEventListener("resize", refit);
  unlisten.forEach((fn) => fn());
  unlisten = [];

  if (sessionId !== null) {
    try {
      await invoke("close_terminal", { id: sessionId });
    } catch {
      // Shell bisa sudah keluar sendiri; menutup tab tetap berhasil.
    }
  }

  sessionId = null;
  terminal?.dispose();
  terminal = null;
  fitAddon = null;
}

export function toggle(cwd: string | null): void {
  if (terminal) {
    void close();
  } else {
    void open(cwd);
  }
}

/// Membuka terminal bila perlu, lalu mengetik satu perintah ke shell yang sama.
/// `cd` dikirim lebih dulu supaya perintah selalu berjalan di folder situs yang
/// sedang dipilih, bukan di folder situs sebelumnya.
export async function run(cwd: string | null, command: string): Promise<void> {
  const fresh = !isOpen();
  await open(cwd);

  const id = sessionId;
  if (id === null) return;

  if (fresh) {
    // PowerShell belum siap menerima baris perintah tepat setelah spawn.
    await new Promise((resolve) => window.setTimeout(resolve, 900));
  }

  const cd = cwd ? `Set-Location -LiteralPath '${cwd.replace(/'/g, "''")}'` : "";
  const data = cd ? `${cd}\r${command}\r` : `${command}\r`;

  await invoke("write_terminal", { id, data });
  terminal?.focus();
}

function refit(): void {
  fitAddon?.fit();
}

function themeColors() {
  const styles = getComputedStyle(document.documentElement);
  const read = (name: string) => styles.getPropertyValue(name).trim();
  return {
    background: read("--bg"),
    foreground: read("--fg"),
    cursor: read("--accent"),
    selectionBackground: read("--line"),
  };
}
