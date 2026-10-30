import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

type Status = "starting" | "ready" | "backoff" | "failed" | "stopped";

interface SiteView {
  id: string;
  name: string;
  domain: string;
  php: string;
  framework: string;
  enabled: boolean;
  path: string;
  port: number | null;
  status: Status;
  detail: string | null;
}

interface Snapshot {
  tld: string;
  running: boolean;
  caddy_installed: boolean;
  installed_php: string[];
  sites: SiteView[];
  services: ServiceView[];
}

interface ServiceView {
  name: string;
  status: Status;
  detail: string | null;
}

interface Detection {
  framework: string;
  label: string;
  document_root: string;
  project_root: string;
}

interface Wizard {
  step: 1 | 2 | 3;
  path: string;
  detection: Detection | null;
  php: string;
  name: string;
  domain: string;
  error: string | null;
  checking: boolean;
}

let snapshot: Snapshot | null = null;
let loading = true;
let bootError: string | null = null;
let notice: string | null = null;
let busy: string | null = null;
let selectedId: string | null = null;
let pendingRemove: string | null = null;
let wizard: Wizard | null = null;

let logFor: string | null = null;
let logLines: string[] = [];
let logError: string | null = null;

let pollTimer: number | null = null;
let lastSnapshotJson = "";

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const esc = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (ch) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch] ?? ch,
  );

const FRAMEWORK_LABEL: Record<string, string> = {
  laravel: "Laravel",
  codeigniter4: "CodeIgniter 4",
  codeigniter3: "CodeIgniter 3",
  php: "PHP",
};

const STATUS_WORD: Record<Status, string> = {
  ready: "hidup",
  starting: "mulai",
  backoff: "ulang",
  failed: "gagal",
  stopped: "berhenti",
};

function frameworkLabel(key: string): string {
  return FRAMEWORK_LABEL[key] ?? key;
}

/* ---------------------------------------------------------------- data */

async function load(): Promise<void> {
  loading = true;
  bootError = null;
  render();

  try {
    const next = await invoke<Snapshot>("snapshot");
    applySnapshot(next);
  } catch (err) {
    bootError = String(err);
  } finally {
    loading = false;
    render();
    schedulePoll();
  }
}

function applySnapshot(next: Snapshot): void {
  const json = JSON.stringify(next);
  const changed = json !== lastSnapshotJson;
  lastSnapshotJson = json;
  snapshot = next;

  if (selectedId && !next.sites.some((site) => site.id === selectedId)) {
    selectedId = null;
  }

  if (changed && !wizard) {
    render();
  }
}

async function refresh(): Promise<void> {
  if (busy) return;
  try {
    const next = await invoke<Snapshot>("snapshot");
    applySnapshot(next);
    if (selectedId && !wizard) await loadLog(selectedId);
  } catch {
    // Gangguan sesaat saat start/stop bukan alasan menutupi layar dengan galat.
  }
}

function schedulePoll(): void {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
  }
  // Selalu memantau: service bisa menyala sendiri saat aplikasi dibuka, dan
  // supervisor bisa memulai ulang pool kapan saja.
  pollTimer = window.setInterval(() => {
    void refresh();
  }, 2000);
}

async function loadLog(id: string): Promise<void> {
  try {
    const lines = await invoke<string[]>("tail_log", { name: id, lines: 200 });
    logFor = id;
    logLines = lines;
    logError = null;
  } catch (err) {
    logFor = id;
    logLines = [];
    logError = String(err);
  }
  paintLog();
}

function paintLog(): void {
  const node = document.getElementById("log");
  if (!node) return;

  if (logError) {
    node.className = "log log-empty";
    node.textContent = logError;
    return;
  }
  if (logFor !== selectedId || logLines.length === 0) {
    node.className = "log log-empty";
    node.textContent =
      logFor === selectedId
        ? "Belum ada baris log. Log muncul setelah Caddy melayani permintaan pertama."
        : "Memuat log...";
    return;
  }

  node.className = "log";
  node.textContent = logLines.join("\n");
  node.scrollTop = node.scrollHeight;
}

/* -------------------------------------------------------------- render */

function render(): void {
  renderHeader();
  renderNotice();
  renderList();
  renderDetail();
  renderStatusbar();
}

function renderStatusbar(): void {
  const node = el("statusbar");

  if (loading || bootError || !snapshot) {
    node.hidden = true;
    node.innerHTML = "";
    return;
  }

  node.hidden = false;

  if (!snapshot.running) {
    node.innerHTML =
      '<span class="status-item"><span class="dot" data-status="stopped"></span>service berhenti</span>';
    return;
  }

  const items = (snapshot.services ?? [])
    .map(
      (service) =>
        `<span class="status-item"><span class="dot" data-status="${
          service.status
        }"></span>${esc(service.name)} <span class="status-word">${
          STATUS_WORD[service.status] ?? service.status
        }</span>${service.detail ? ` <span>· ${esc(service.detail)}</span>` : ""}</span>`,
    )
    .join("");

  node.innerHTML = items || '<span class="status-item">tidak ada service</span>';
}

function renderHeader(): void {
  const stateNode = el("global-state");
  const actions = el("global-actions");

  if (loading) {
    stateNode.textContent = "Membaca konfigurasi...";
    actions.innerHTML = "";
    return;
  }

  if (bootError) {
    stateNode.innerHTML = '<span class="dot" data-status="failed"></span>Config tidak terbaca';
    actions.innerHTML = '<button class="btn" type="button" data-action="retry">Coba lagi</button>';
    return;
  }

  const running = snapshot?.running ?? false;
  const caddyMissing = snapshot && !snapshot.caddy_installed;

  stateNode.innerHTML = running
    ? '<span class="dot" data-status="ready"></span>Berjalan'
    : '<span class="dot" data-status="stopped"></span>Berhenti';

  if (running) {
    actions.innerHTML = `<button class="btn" type="button" data-action="open-dashboard">Buka dasbor</button>
      <button class="btn" type="button" data-action="stop" ${
        busy ? "disabled" : ""
      }>${busy === "stop" ? "Menghentikan..." : "Hentikan"}</button>`;
    return;
  }

  const disabled = busy || caddyMissing;
  const title = caddyMissing ? "Caddy belum terpasang" : "";

  actions.innerHTML = `<button class="btn btn-primary" type="button" data-action="start" ${
    disabled ? "disabled" : ""
  } ${title ? `title="${esc(title)}"` : ""}>${
    busy === "start" ? "Memulai..." : "Jalankan"
  }</button>`;
}

function renderNotice(): void {
  const node = el("notice");
  if (!notice) {
    node.hidden = true;
    node.innerHTML = "";
    return;
  }

  node.hidden = false;
  node.dataset.kind = "error";
  node.innerHTML = `
    <span class="notice-text">${esc(notice)}</span>
    <button class="btn btn-quiet" type="button" data-action="dismiss-notice">Tutup</button>`;
}

function renderList(): void {
  const list = el("site-list");

  if (loading) {
    list.innerHTML = '<div class="state"><p>Membaca konfigurasi...</p></div>';
    return;
  }

  if (bootError) {
    list.innerHTML = `
      <div class="state">
        <h2>Config tidak bisa dibaca</h2>
        <p>DevKu tidak mengubah apa pun. Perbaiki berkas di <code>%APPDATA%\\DevKu\\config.json</code>, lalu coba lagi.</p>
        <p class="hint">${esc(bootError)}</p>
        <button class="btn" type="button" data-action="retry">Coba lagi</button>
      </div>`;
    return;
  }

  if (!snapshot || snapshot.sites.length === 0) {
    list.innerHTML = `
      <div class="state">
        <h2>Belum ada situs</h2>
        <p>Tambahkan folder project. DevKu mengenali Laravel, CodeIgniter 3, CodeIgniter 4, atau PHP biasa dari isi foldernya.</p>
        <button class="btn btn-primary" type="button" data-action="open-wizard">Tambah situs</button>
      </div>`;
    return;
  }

  list.innerHTML = snapshot.sites
    .map((site) => {
      const port = site.port ? `:${site.port}` : "port -";
      const statusWord = STATUS_WORD[site.status];
      const showStatus = site.status !== "ready";

      return `
        <div class="site-row" role="listitem">
          <span class="rail" data-status="${site.status}" aria-hidden="true"></span>
          <button class="row-main btn-quiet" type="button" data-action="select" data-id="${esc(
            site.id,
          )}" aria-current="${site.id === selectedId}">
            <span class="row-name">${esc(site.name)}</span>
            <span class="row-meta">${esc(site.domain)}<span class="sep">·</span>php ${esc(
              site.php,
            )}<span class="sep">·</span>${esc(port)}${
              showStatus ? `<span class="sep">·</span>${statusWord}` : ""
            }</span>
          </button>
          <span class="row-actions">
            ${
              pendingRemove === site.id
                ? `<button class="btn btn-quiet btn-danger" type="button" data-action="confirm-remove" data-id="${esc(
                    site.id,
                  )}">Hapus permanen?</button>
                   <button class="btn btn-quiet" type="button" data-action="cancel-remove">Batal</button>`
                : `<button class="btn btn-quiet" type="button" data-action="toggle-site" data-id="${esc(
                    site.id,
                  )}">${site.enabled ? "Matikan" : "Aktifkan"}</button>
                   <button class="btn btn-quiet" type="button" data-action="ask-remove" data-id="${esc(
                     site.id,
                   )}">Hapus</button>`
            }
          </span>
        </div>`;
    })
    .join("");
}

function renderDetail(): void {
  const pane = el("detail");

  if (wizard) {
    pane.innerHTML = renderWizard();
    bindWizard();
    return;
  }

  const site = snapshot?.sites.find((item) => item.id === selectedId);
  if (!site) {
    pane.innerHTML = `
      <div class="state">
        <h2>Detail situs</h2>
        <p>Pilih salah satu situs untuk melihat domain, port PHP, folder, dan log-nya.</p>
      </div>`;
    return;
  }

  const status = `${STATUS_WORD[site.status]}${site.detail ? ` (${site.detail})` : ""}`;

  pane.innerHTML = `
    <div class="detail-head">
      <h2>${esc(site.name)}</h2>
    </div>
    <div class="detail-body">
      <dl class="facts">
        <dt>Domain</dt><dd>${esc(site.domain)}</dd>
        <dt>PHP</dt><dd>${esc(site.php)}</dd>
        <dt>Framework</dt><dd>${esc(frameworkLabel(site.framework))}</dd>
        <dt>Port</dt><dd>${site.port ? `127.0.0.1:${site.port}` : "-"}</dd>
        <dt>Folder</dt><dd>${esc(site.path)}</dd>
        <dt>Status</dt><dd>${esc(status)}</dd>
      </dl>

      <div class="log-head">
        <h3>Log</h3>
        <button class="btn" type="button" data-action="refresh-log">Muat ulang</button>
      </div>
      <pre class="log log-empty" id="log"></pre>
    </div>`;

  paintLog();
  if (logFor !== site.id) {
    logFor = null;
    void loadLog(site.id);
  }
}

function renderWizard(): string {
  const w = wizard!;
  const tld = snapshot?.tld ?? "localhost";
  const steps = [1, 2, 3]
    .map((step) => `<span aria-current="${w.step === step}">${step}</span>`)
    .join(" · ");

  const head = `
    <div class="detail-head"><h2>Tambah situs</h2></div>
    <div class="detail-body">
      <div class="wizard-steps">Langkah ${steps}</div>`;

  if (w.step === 1) {
    const error = w.error ? `<p class="field-error">${esc(w.error)}</p>` : "";
    return `${head}
      <div class="field">
        <label class="field-label" for="w-path">Folder project</label>
        <input class="input input-mono" id="w-path" value="${esc(w.path)}" placeholder="D:\\projects\\app" />
      </div>
      <div class="wizard-actions">
        <button class="btn" type="button" data-action="pick-folder">Pilih folder</button>
      </div>
      ${error}
      <div class="wizard-actions">
        <span class="spacer"></span>
        <button class="btn" type="button" data-action="close-wizard">Batal</button>
        <button class="btn btn-primary" type="button" data-action="wizard-next" ${
          w.checking || !w.path.trim() ? "disabled" : ""
        }>${w.checking ? "Memeriksa..." : "Lanjut"}</button>
      </div>
    </div>`;
  }

  const detected = w.detection;
  if (!detected) {
    return `${head}
      <div class="state"><p>Folder belum diperiksa.</p></div>
      <div class="wizard-actions">
        <button class="btn" type="button" data-action="wizard-back">Kembali</button>
      </div>
    </div>`;
  }

  if (w.step === 2) {
    const options = (snapshot?.installed_php ?? [])
      .map(
        (version) =>
          `<option value="${esc(version)}" ${version === w.php ? "selected" : ""}>${esc(
            version,
          )}</option>`,
      )
      .join("");

    const phpBlock = options
      ? `<select class="input" id="w-php">${options}</select>`
      : `<p class="field-error">Belum ada PHP terpasang. Pasang satu versi dulu lewat <code>devku-cli install-php</code>.</p>`;

    return `${head}
      <dl class="facts">
        <dt>Framework</dt><dd>${esc(detected.label)}</dd>
        <dt>Akar web</dt><dd>${esc(detected.document_root)}</dd>
      </dl>

      <div class="field">
        <label class="field-label" for="w-name">Nama</label>
        <input class="input" id="w-name" value="${esc(w.name)}" />
      </div>

      <div class="field">
        <label class="field-label" for="w-domain">Domain</label>
        <span class="domain-row">
          <input class="input input-mono" id="w-domain" value="${esc(w.domain)}" />
          <span class="domain-suffix">.${esc(tld)}</span>
        </span>
      </div>

      <div class="field">
        <label class="field-label" for="w-php">Versi PHP</label>
        ${phpBlock}
      </div>

      ${w.error ? `<p class="field-error">${esc(w.error)}</p>` : ""}

      <div class="wizard-actions">
        <button class="btn" type="button" data-action="wizard-back">Kembali</button>
        <span class="spacer"></span>
        <button class="btn" type="button" data-action="close-wizard">Batal</button>
        <button class="btn btn-primary" type="button" data-action="wizard-next" ${
          !options || busy ? "disabled" : ""
        }>Lanjut</button>
      </div>
    </div>`;
  }

  const domain = w.domain.trim() ? `${w.domain.trim()}.${tld}` : "-";
  return `${head}
    <dl class="summary">
      <dt>Nama</dt><dd>${esc(w.name || "-")}</dd>
      <dt>Domain</dt><dd>${esc(domain)}</dd>
      <dt>Framework</dt><dd>${esc(detected.label)}</dd>
      <dt>Akar web</dt><dd>${esc(detected.document_root)}</dd>
      <dt>PHP</dt><dd>${esc(w.php || "-")}</dd>
    </dl>
    ${w.error ? `<p class="field-error">${esc(w.error)}</p>` : ""}
    <div class="wizard-actions">
      <button class="btn" type="button" data-action="wizard-back">Kembali</button>
      <span class="spacer"></span>
      <button class="btn" type="button" data-action="close-wizard">Batal</button>
      <button class="btn btn-primary" type="button" data-action="wizard-submit" ${
        busy ? "disabled" : ""
      }>${busy === "add-site" ? "Menambahkan..." : "Tambah situs"}</button>
    </div>
  </div>`;
}

/* --------------------------------------------------------- wizard binds */

function bindWizard(): void {
  const w = wizard;
  if (!w) return;

  const path = document.getElementById("w-path") as HTMLInputElement | null;
  path?.addEventListener("input", () => {
    w.path = path.value;
    const next = document.querySelector(
      '[data-action="wizard-next"]',
    ) as HTMLButtonElement | null;
    if (next) next.disabled = w.checking || !w.path.trim();
    w.error = null;
  });

  const name = document.getElementById("w-name") as HTMLInputElement | null;
  name?.addEventListener("input", () => {
    w.name = name.value;
    w.error = null;
  });

  const domain = document.getElementById("w-domain") as HTMLInputElement | null;
  domain?.addEventListener("input", () => {
    w.domain = domain.value;
    w.error = null;
  });

  const php = document.getElementById("w-php") as HTMLSelectElement | null;
  php?.addEventListener("change", () => {
    w.php = php.value;
  });

  path?.focus();
}

/* -------------------------------------------------------------- actions */

function openWizard(): void {
  pendingRemove = null;
  notice = null;
  wizard = {
    step: 1,
    path: "",
    detection: null,
    php: snapshot?.installed_php[snapshot.installed_php.length - 1] ?? "",
    name: "",
    domain: "",
    error: null,
    checking: false,
  };
  render();
}

async function pickFolder(): Promise<void> {
  if (!wizard) return;
  const picked = await open({ directory: true, multiple: false, title: "Pilih folder project" });
  if (typeof picked !== "string") return;

  wizard.path = picked;
  wizard.error = null;
  render();
}

async function wizardNext(): Promise<void> {
  const w = wizard;
  if (!w) return;

  if (w.step === 1) {
    const path = w.path.trim();
    if (!path) {
      w.error = "Isi folder project dulu.";
      render();
      return;
    }

    w.checking = true;
    w.error = null;
    render();

    try {
      const found = await invoke<Detection | null>("detect_project", { path });
      w.checking = false;

      if (!found) {
        w.error =
          "Tidak ada tanda project PHP di folder itu. Cari folder yang berisi index.php, artisan, atau spark.";
        render();
        return;
      }

      w.detection = found;
      w.step = 2;
      const folder = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "site";
      w.name = folder;
      w.domain = slug(folder);
      if (!w.php) w.php = snapshot?.installed_php[snapshot.installed_php.length - 1] ?? "";
      render();
    } catch (err) {
      w.checking = false;
      w.error = String(err);
      render();
    }
    return;
  }

  if (w.step === 2) {
    if (!w.php) {
      w.error = "Pilih versi PHP dulu.";
      render();
      return;
    }
    if (!w.domain.trim()) {
      w.error = "Domain tidak boleh kosong.";
      render();
      return;
    }
    w.error = null;
    w.step = 3;
    render();
    return;
  }
}

async function wizardSubmit(): Promise<void> {
  const w = wizard;
  if (!w || !w.detection) return;

  busy = "add-site";
  w.error = null;
  render();

  try {
    const next = await invoke<Snapshot>("add_site", {
      draft: {
        path: w.detection.project_root,
        php: w.php,
        name: w.name.trim() || undefined,
        domain: w.domain.trim() ? `${w.domain.trim()}.${snapshot?.tld ?? "localhost"}` : undefined,
        framework: w.detection.framework,
      },
    });

    const submitted = `${w.domain.trim()}.${snapshot?.tld ?? "localhost"}`;
    applySnapshot(next);
    selectedId = next.sites.find((site) => site.domain === submitted)?.id ?? null;

    wizard = null;
    busy = null;
    notice = null;
    render();
    if (selectedId) void loadLog(selectedId);
  } catch (err) {
    busy = null;
    w.error = String(err);
    render();
  }
}

async function start(): Promise<void> {
  busy = "start";
  notice = null;
  render();

  try {
    const next = await invoke<Snapshot>("start");
    applySnapshot(next);
    schedulePoll();
  } catch (err) {
    notice = `Gagal menjalankan: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function stop(): Promise<void> {
  busy = "stop";
  render();

  try {
    const next = await invoke<Snapshot>("stop");
    applySnapshot(next);
    schedulePoll();
  } catch (err) {
    notice = `Gagal menghentikan: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function toggleSite(id: string): Promise<void> {
  const site = snapshot?.sites.find((item) => item.id === id);
  if (!site) return;

  busy = "toggle";
  try {
    const next = await invoke<Snapshot>("set_site_enabled", {
      id,
      enabled: !site.enabled,
    });
    applySnapshot(next);
    render();
  } catch (err) {
    notice = `Gagal mengubah ${site.name}: ${String(err)}`;
    render();
  } finally {
    busy = null;
  }
}

async function removeSite(id: string): Promise<void> {
  busy = "remove";
  try {
    const next = await invoke<Snapshot>("remove_site", { id });
    pendingRemove = null;
    applySnapshot(next);
    if (selectedId === id) selectedId = null;
    render();
  } catch (err) {
    notice = `Gagal menghapus situs: ${String(err)}`;
    render();
  } finally {
    busy = null;
  }
}

/* ---------------------------------------------------------------- misc */

function slug(value: string): string {
  return (
    value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "site"
  );
}

document.addEventListener("click", (event) => {
  const target = (event.target as HTMLElement).closest("[data-action]") as HTMLElement | null;
  if (!target) return;

  const action = target.dataset.action;
  const id = target.dataset.id;

  switch (action) {
    case "open-wizard":
      openWizard();
      break;
    case "close-wizard":
      wizard = null;
      busy = null;
      render();
      break;
    case "pick-folder":
      void pickFolder();
      break;
    case "wizard-next":
      void wizardNext();
      break;
    case "wizard-back":
      if (!wizard) break;
      wizard.step = (wizard.step === 1 ? 1 : wizard.step - 1) as 1 | 2 | 3;
      wizard.error = null;
      render();
      break;
    case "wizard-submit":
      void wizardSubmit();
      break;
    case "select":
      if (id) {
        selectedId = id;
        logFor = null;
        render();
      }
      break;
    case "toggle-site":
      if (id) void toggleSite(id);
      break;
    case "ask-remove":
      pendingRemove = id ?? null;
      render();
      break;
    case "confirm-remove":
      if (id) void removeSite(id);
      break;
    case "cancel-remove":
      pendingRemove = null;
      render();
      break;
    case "refresh-log":
      if (selectedId) void loadLog(selectedId);
      break;
    case "start":
      void start();
      break;
    case "open-dashboard":
      void openUrl("http://localhost/");
      break;
    case "stop":
      void stop();
      break;
    case "retry":
      void load();
      break;
    case "dismiss-notice":
      notice = null;
      render();
      break;
  }
});

if (document.readyState === "loading") {
  window.addEventListener("DOMContentLoaded", () => void load());
} else {
  void load();
}
