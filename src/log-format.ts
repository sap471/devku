/// Satu baris access log Caddy yang sudah diringkas. Header permintaan tidak
/// ikut ditampilkan karena tidak membantu saat membaca lalu lintas situs.
export interface AccessLog {
  date: string;
  time: string;
  method: string;
  uri: string;
  status: number;
  durationMs: number | null;
  size: number | null;
}

/// Baris access log Caddy berbentuk:
/// `2026/10/30 17:17:18.161\tINFO\t...\thandled request\t{...json...}`.
/// Baris lain mengembalikan null dan ditampilkan apa adanya, jadi log PHP atau
/// pesan Caddy tidak hilang.
export function parseAccessLog(line: string): AccessLog | null {
  const marker = "\thandled request\t";
  const at = line.indexOf(marker);
  if (at === -1) return null;

  const stamp = line.slice(0, at);
  const space = stamp.indexOf(" ");
  if (space === -1) return null;

  const date = stamp.slice(0, space).replace(/\//g, "-");
  const clock = stamp.slice(space + 1).split(".")[0];
  if (!/^\d{2}:\d{2}:\d{2}$/.test(clock)) return null;

  let payload: {
    request?: { method?: unknown; uri?: unknown };
    status?: unknown;
    duration?: unknown;
    size?: unknown;
  };
  try {
    payload = JSON.parse(line.slice(at + marker.length));
  } catch {
    return null;
  }

  const method = typeof payload.request?.method === "string" ? payload.request.method : null;
  const uri = typeof payload.request?.uri === "string" ? payload.request.uri : null;
  const status = typeof payload.status === "number" ? payload.status : null;
  if (!method || !uri || status === null) return null;

  return {
    date,
    time: clock,
    method,
    uri,
    status,
    durationMs: typeof payload.duration === "number" ? payload.duration * 1000 : null,
    size: typeof payload.size === "number" ? payload.size : null,
  };
}

export function statusTone(status: number): "ok" | "warn" | "bad" {
  if (status >= 500) return "bad";
  if (status >= 400) return "warn";
  return "ok";
}

export function formatDuration(ms: number): string {
  return ms < 10 ? `${ms.toFixed(1).replace(".", ",")} ms` : `${Math.round(ms)} ms`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1).replace(".", ",")} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1).replace(".", ",")} MB`;
}
