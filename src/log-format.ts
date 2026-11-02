/// A single Caddy access log line, already condensed. Request headers are not
/// shown because they do not help when reading a site's traffic.
export interface AccessLog {
  date: string;
  time: string;
  method: string;
  uri: string;
  status: number;
  durationMs: number | null;
  size: number | null;
}

/// A Caddy access log line looks like:
/// `2026/10/30 17:17:18.161\tINFO\t...\thandled request\t{...json...}`.
/// Other lines return null and are shown as-is, so PHP logs or
/// Caddy messages are not lost.
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

export type RawLogLevel = "error" | "warn" | "info";

export function classifyRawLine(line: string): RawLogLevel {
  const lower = line.toLowerCase();
  if (
    lower.includes("fatal error") ||
    lower.includes("parse error") ||
    lower.includes("uncaught exception") ||
    lower.includes("error:") ||
    lower.includes("level=error") ||
    lower.includes("\terror\t")
  ) {
    return "error";
  }
  if (
    lower.includes("warning:") ||
    lower.includes("deprecated:") ||
    lower.includes("level=warn") ||
    lower.includes("\twarn\t")
  ) {
    return "warn";
  }
  return "info";
}
