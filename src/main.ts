import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { openDialog } from "./dialog";
import type { DialogCheckbox } from "./dialog";
import { formatBytes, formatDuration, parseAccessLog, statusTone } from "./log-format";
import type { AccessLog } from "./log-format";
import * as terminalPanel from "./terminal";

type Status = "starting" | "ready" | "backoff" | "failed" | "stopped";

interface SiteView {
  id: string;
  name: string;
  domain: string;
  php: string;
  framework: string;
  enabled: boolean;
  path: string;
  root: string;
  port: number | null;
  status: Status;
  detail: string | null;
}

interface Snapshot {
  tld: string;
  running: boolean;
  caddy_installed: boolean;
  https_trusted: boolean;
  onboarded: boolean;
  installed_php: string[];
  default_php: string;
  php_path: PathStatus;
  composer_installed: boolean;
  composer_version: string | null;
  composer_php: string | null;
  composer_php_missing: boolean;
  composer_global: ComposerGlobal;
  config_path: string;
  home_path: string;
  start_at_login: boolean;
  sites: SiteView[];
  services: ServiceView[];
  database: DatabaseView;
}

interface PathStatus {
  dir: string;
  registered: boolean;
  shim_version: string | null;
  broken: boolean;
}

interface ComposerGlobal {
  dir: string;
  registered: boolean;
  exists: boolean;
  tools: string[];
}

interface ServiceView {
  name: string;
  status: Status;
  detail: string | null;
}

interface RuntimeInfo {
  name: string;
  version: string | null;
  source: string;
  path: string | null;
}

interface OpenTarget {
  id: string;
  label: string;
  available: boolean;
}

interface PhpCatalogEntry {
  branch: string;
  version: string;
  filename: string;
  sha256: string;
  size: string;
  installed: boolean;
  local: boolean;
}

interface PhpCatalogView {
  entries: PhpCatalogEntry[];
  warning: string | null;
}

interface DownloadProgress {
  target: string;
  version: string;
  received: number;
  total: number | null;
  stage: string;
}

interface ImportReport {
  imported: number;
  skipped: string[];
  disabled: string[];
  active_php_cleared: string | null;
}

interface ImportResponse {
  report: ImportReport;
  snapshot: Snapshot;
}

interface PhpRemoval {
  version: string | null;
  disabled_sites: string[];
  default_php: string;
  composer_moved: boolean;
  removed_all: boolean;
}

interface PhpRemovalResponse {
  removal: PhpRemoval;
  snapshot: Snapshot;
}

interface DatabaseView {
  engine: string;
  label: string;
  version: string;
  installed: boolean;
  initialized: boolean;
  port: number;
  autostart: boolean;
  status: Status;
  detail: string | null;
  data_path: string;
  bin_path: string;
  has_password: boolean;
}

interface DatabaseRemoval {
  removed_data: boolean;
}

interface DatabaseRemovalResponse {
  removal: DatabaseRemoval;
  backup: string | null;
  snapshot: Snapshot;
}

interface SiteCommand {
  id: string;
  label: string;
  command: string;
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
/// Kabar berhasil, ditampilkan di banner yang sama tapi dengan warna netral.
let infoNotice: string | null = null;
let busy: string | null = null;
let selectedId: string | null = null;
let wizard: Wizard | null = null;

let logFor: string | null = null;
let logLines: string[] = [];
let logError: string | null = null;

let runtimesList: RuntimeInfo[] | null = null;
let runtimesError: string | null = null;

let phpCatalog: PhpCatalogEntry[] | null = null;
let phpCatalogError: string | null = null;
let phpCatalogWarning: string | null = null;
let phpCatalogLoading = false;
let download: DownloadProgress | null = null;

let commandsFor: string | null = null;
let siteCommands: SiteCommand[] | null = null;
let commandsError: string | null = null;

/// Hasil ekspor/impor config. Ditampilkan di baris Konfigurasi, bukan banner
/// galat, karena impor yang berhasil bukan kegagalan.
let configNotice: string | null = null;

/// Menu "Buka" yang sedang terbuka. Dipisah dari data situs supaya menutup
/// menu tidak perlu memuat ulang snapshot.
let openMenuFor: string | null = null;
let openMenuItems: OpenTarget[] | null = null;
let openMenuError: string | null = null;

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

  if (!bootError) void loadRuntimes();
}

async function loadRuntimes(): Promise<void> {
  try {
    runtimesList = await invoke<RuntimeInfo[]>("runtimes");
    runtimesError = null;
  } catch (err) {
    runtimesList = [];
    runtimesError = String(err);
  }

  if (!wizard && !selectedId) renderDetail();
}

async function loadPhpCatalog(): Promise<void> {
  if (phpCatalog !== null || phpCatalogLoading) return;

  phpCatalogLoading = true;
  phpCatalogError = null;
  paintCatalog();

  try {
    const view = await invoke<PhpCatalogView>("php_catalog");
    phpCatalog = view.entries;
    phpCatalogWarning = view.warning;
  } catch (err) {
    phpCatalog = [];
    phpCatalogWarning = null;
    phpCatalogError = String(err);
  } finally {
    phpCatalogLoading = false;
    paintCatalog();
  }
}

/// Katalog hidup di dua tempat (wizard dan panel runtime), jadi yang diperbarui
/// saat progres berjalan hanya kotaknya, bukan seluruh panel.
function paintCatalog(): void {
  const node = document.getElementById("php-catalog");
  if (node) node.innerHTML = renderPhpCatalog();
}

/// Kotak yang isinya bergantung pada unduhan yang sedang berjalan.
function paintDownloads(): void {
  paintCatalog();

  const binaries = document.getElementById("binaries");
  if (binaries) binaries.innerHTML = renderBinaries();

  const caddy = document.getElementById("onboarding-caddy");
  if (caddy) caddy.innerHTML = caddyAction();

  const database = document.getElementById("database-panel");
  if (database) database.innerHTML = renderDatabase();
}

function percentOf(job: DownloadProgress): number {
  return job.total ? Math.min(100, (job.received / job.total) * 100) : 0;
}

function downloadButton(target: "caddy" | "bun" | "composer"): string {
  const job = download?.target === target ? download : null;
  if (job) {
    return `<span class="catalog-progress">
        <span class="progress"><span style="width:${percentOf(job)}%"></span></span>
        <span class="hint">${esc(stageLabel(job))}</span>
        <button class="btn btn-quiet" type="button" data-action="cancel-download">Batal</button>
      </span>`;
  }

  const installable =
    target === "caddy"
      ? !snapshot?.caddy_installed
      : target === "bun"
        ? !hasBun()
        : !snapshot?.composer_installed || snapshot.composer_php_missing;
  if (!installable) return "";

  const label = target === "composer" && snapshot?.composer_installed ? "Perbaiki" : "Pasang";
  return `<button class="btn" type="button" data-action="install-${target}" ${
    download ? "disabled" : ""
  }>${label}</button>`;
}

function hasBun(): boolean {
  return (runtimesList ?? []).some((runtime) => runtime.name === "bun" && runtime.version);
}

function renderBinaries(): string {
  const caddy = snapshot?.caddy_installed
    ? '<span class="tag">terpasang</span>'
    : '<span class="tag">belum</span>';

  const bunSource = (runtimesList ?? []).find((runtime) => runtime.name === "bun");
  const bun = bunSource?.version
    ? `<span class="tag">${esc(bunSource.source)}</span>`
    : '<span class="tag">belum</span>';

  const composer = snapshot?.composer_installed
    ? '<span class="tag">terpasang</span>'
    : '<span class="tag">belum</span>';

  const composerHint = !snapshot?.composer_installed
    ? "pengelola paket PHP, memakai php di PATH"
    : snapshot.composer_php_missing
      ? "php tidak ditemukan; pasang atau daftarkan PHP dulu"
      : `${
          snapshot.composer_version ? `Composer ${esc(snapshot.composer_version)}` : "Composer"
        }${snapshot.composer_php ? ` · php ${esc(snapshot.composer_php)}` : ""}`;

  const global = snapshot?.composer_global;
  const tools = global?.tools ?? [];
  const toolNames =
    tools.slice(0, 3).join(", ") + (tools.length > 3 ? ` +${tools.length - 3}` : "");
  const globalDetail = !global?.exists
    ? "folder belum ada"
    : tools.length === 0
      ? "belum ada paket global"
      : toolNames;

  return `
    <div class="path-row">
      <span class="catalog-version">Caddy</span>
      ${caddy}
      <span class="hint">server web, port 80 dan 443</span>
      <span class="spacer"></span>
      ${downloadButton("caddy")}
    </div>
    <div class="path-row">
      <span class="catalog-version">Bun</span>
      ${bun}
      <span class="hint">${
        bunSource?.version
          ? esc(bunSource.path ?? "sistem")
          : "bisa dipakai untuk TypeScript dan skrip package.json"
      }</span>
      <span class="spacer"></span>
      ${downloadButton("bun")}
    </div>
    <div class="path-row">
      <span class="catalog-version">Composer</span>
      ${composer}
      <span class="hint">${composerHint}</span>
      <span class="spacer"></span>
      ${downloadButton("composer")}
    </div>
    <div class="path-row">
      <span class="catalog-version">Composer global</span>
      ${global?.registered ? '<span class="tag">di PATH</span>' : '<span class="tag">belum</span>'}
      <span class="hint">${esc(global?.dir ?? "")} · ${esc(globalDetail)}</span>
      <span class="spacer"></span>
      ${
        global?.registered
          ? `<button class="btn btn-quiet" type="button" data-action="disable-composer-global" ${
              busy ? "disabled" : ""
            }>Lepas</button>`
          : `<button class="btn" type="button" data-action="enable-composer-global" ${
              global?.exists && !busy ? "" : "disabled"
            }>Daftarkan</button>`
      }
    </div>
    <div class="path-row">
      <span class="catalog-version">HTTPS lokal</span>
      ${snapshot?.https_trusted ? '<span class="tag">terpercaya</span>' : '<span class="tag">belum</span>'}
      <span class="hint">${httpsHint()}</span>
      <span class="spacer"></span>
      <button class="btn" type="button" data-action="trust-caddy" ${
        snapshot?.caddy_installed && snapshot?.running ? "" : "disabled"
      }>Percayai</button>
    </div>`;
}

function httpsHint(): string {
  if (!snapshot?.caddy_installed) return "pasang Caddy dulu";
  if (!snapshot.running) return "start semua dulu; caddy trust membaca CA dari service yang hidup";
  return "setiap situs juga tersedia di https://domain";
}

function renderPhpCatalog(): string {
  if (phpCatalogError) {
    return `<p class="field-error">${esc(phpCatalogError)}</p>`;
  }
  if (phpCatalog === null) {
    return '<p class="hint">Mengambil daftar versi dari windows.php.net...</p>';
  }

  const warning = phpCatalogWarning
    ? `<p class="hint">${esc(phpCatalogWarning)}</p>`
    : "";

  const available = phpCatalog.filter((entry) => !entry.installed);

  if (available.length === 0) {
    return `${warning}<p class="hint">Semua versi di daftar sudah terpasang.</p>`;
  }

  const rows = available
    .map((entry) => {
      const job =
        download?.target === "php" && download.version === entry.version ? download : null;
      const action = job
        ? `<span class="catalog-progress">
            <span class="progress"><span style="width:${percentOf(job)}%"></span></span>
            <span class="hint">${esc(stageLabel(job))}</span>
            <button class="btn btn-quiet" type="button" data-action="cancel-download">Batal</button>
          </span>`
        : `<button class="btn" type="button" data-action="install-php" data-version="${esc(
            entry.version,
          )}" ${download ? "disabled" : ""}>Pasang</button>`;

      return `<div class="catalog-row"><span class="catalog-version">${esc(
        entry.version,
      )}</span><span class="hint">${
        entry.local ? "folder DevKu" : esc(entry.size)
      }</span>${action}</div>`;
    })
    .join("");

  return `${warning}<div class="catalog">${rows}</div>`;
}

/// Daftar versi terpasang beserta aksi per versi. Katalog di bawahnya hanya
/// menampilkan versi yang belum terpasang, jadi tidak ada baris ganda.
function renderInstalledPhp(): string {
  const installed = snapshot?.installed_php ?? [];
  if (installed.length === 0) return "";

  const path = snapshot?.php_path;
  const rows = installed
    .map((version) => {
      const tags = [
        version === snapshot?.default_php ? '<span class="tag">default</span>' : "",
        path?.registered && path.shim_version === version
          ? '<span class="tag">di PATH</span>'
          : "",
      ].join("");

      const service = snapshot?.services.find((item) => item.name === `php-${version}`);
      const live = service?.status === "ready";

      return `<div class="php-row">
        <span class="catalog-version">${esc(version)}</span>
        ${tags}${live ? '<span class="tag">hidup</span>' : ""}
        <span class="spacer"></span>
        <button class="btn btn-quiet" type="button" data-action="edit-php-ini" data-version="${esc(
          version,
        )}" ${busy ? "disabled" : ""}>Edit php.ini</button>
        <button class="btn btn-quiet" type="button" data-action="restart-php" data-version="${esc(
          version,
        )}" ${busy || !snapshot?.running ? "disabled" : ""}>${
          live ? "Mulai ulang" : "Mulai"
        }</button>
        <button class="btn btn-quiet btn-danger" type="button" data-action="ask-remove-php" data-version="${esc(
          version,
        )}" ${busy ? "disabled" : ""}>Hapus</button>
      </div>`;
    })
    .join("");

  return `<div class="log-head"><h3>Terpasang</h3></div><div class="catalog">${rows}</div>`;
}

function renderPhpSettings(): string {
  const installed = snapshot?.installed_php ?? [];
  const path = snapshot?.php_path;

  const options = installed
    .map(
      (version) =>
        `<option value="${esc(version)}" ${
          version === snapshot?.default_php ? "selected" : ""
        }>${esc(version)}</option>`,
    )
    .join("");

  const defaultField = options
    ? `<div class="field">
        <label class="field-label" for="default-php">PHP default</label>
        <select class="input" id="default-php" ${busy ? "disabled" : ""}>${options}</select>
        <p class="hint">Dipakai untuk situs baru dan untuk perintah <code>php</code> di PATH. Perubahan php.ini berlaku setelah pool versi itu dimulai ulang.</p>
      </div>`
    : '<p class="hint">Belum ada PHP terpasang. Pasang satu versi di daftar bawah.</p>';

  const list = renderInstalledPhp();

  if (!path) return `${defaultField}${list}`;

  const entry = `<code class="path-value">${esc(path.dir)}</code>`;

  if (path.registered) {
    const target = path.shim_version
      ? `php → ${esc(path.shim_version)}`
      : "shim belum dibuat";

    return `${defaultField}${list}
      <div class="path-row">
        <span class="tag">di PATH</span>
        ${entry}
        <span class="hint">${target}</span>
        <button class="btn btn-quiet" type="button" data-action="disable-php-path" ${
          busy ? "disabled" : ""
        }>Lepas</button>
      </div>
      ${
        path.broken
          ? '<p class="field-error">Shim menunjuk versi yang sudah tidak terpasang. Pilih PHP default lain untuk memperbaikinya.</p>'
          : ""
      }
      <p class="hint">Terminal yang sudah terbuka perlu dibuka ulang agar PATH baru terbaca.</p>`;
  }

  return `${defaultField}${list}
    <div class="path-row">
      <span class="tag">belum di PATH</span>
      ${entry}
      <button class="btn" type="button" data-action="enable-php-path" ${
        installed.length > 0 && !busy ? "" : "disabled"
      }>Daftarkan</button>
    </div>
    <p class="hint">Menambahkan folder itu ke PATH pengguna, tanpa admin dan bisa dilepas lagi. Perintah <code>php</code> lalu bisa dipakai di terminal mana pun.</p>`;
}

/// Panel database lokal. Terpisah dari PHP karena situs tetap jalan walau
/// database mati.
function renderDatabase(): string {
  const db = snapshot?.database;
  if (!db) return "";

  const job = download?.target === "mariadb" ? download : null;
  const rows: string[] = [];

  if (!db.installed) {
    rows.push(`<div class="path-row">
      <span class="catalog-version">${esc(db.label)}</span>
      <span class="tag">belum</span>
      <span class="hint">server database lokal, port ${db.port}</span>
      <span class="spacer"></span>
      ${
        job
          ? `<span class="catalog-progress">
              <span class="progress"><span style="width:${percentOf(job)}%"></span></span>
              <span class="hint">${esc(stageLabel(job))}</span>
              <button class="btn btn-quiet" type="button" data-action="cancel-download">Batal</button>
            </span>`
          : `<button class="btn" type="button" data-action="install-database" ${
              download ? "disabled" : ""
            }>Pasang</button>`
      }
    </div>`);
    return rows.join("");
  }

  const live = db.status === "ready";
  rows.push(`<div class="path-row">
    <span class="catalog-version">${esc(db.label)} ${esc(db.version)}</span>
    <span class="tag">${live ? "hidup" : STATUS_WORD[db.status] ?? "berhenti"}</span>
    <span class="hint">${esc(db.data_path)}</span>
    <span class="spacer"></span>
    ${
      live
        ? `<button class="btn btn-quiet" type="button" data-action="stop-database" ${
            busy ? "disabled" : ""
          }>Stop</button>`
        : `<button class="btn btn-quiet" type="button" data-action="start-database" ${
            busy ? "disabled" : ""
          }>Mulai</button>`
    }
    <button class="btn btn-quiet btn-danger" type="button" data-action="ask-remove-database" ${
      busy ? "disabled" : ""
    }>Hapus</button>
  </div>`);

  if (db.detail) {
    rows.push(`<p class="field-error">${esc(db.detail)}</p>`);
  }

  rows.push(`<div class="path-row">
    <span class="catalog-version">Koneksi</span>
    <code class="path-value">127.0.0.1:${db.port}</code>
    <span class="hint">user root${db.has_password ? ", password diatur" : ", tanpa password"}</span>
    <span class="spacer"></span>
    <button class="btn btn-quiet" type="button" data-action="reset-db-password" ${
      busy || !snapshot?.running ? "disabled" : ""
    }>Atur password</button>
  </div>`);

  rows.push(`<div class="path-row">
    <span class="catalog-version">Nyalakan otomatis</span>
    ${db.autostart ? '<span class="tag">aktif</span>' : '<span class="tag">tidak</span>'}
    <span class="hint">Ikut menyala saat service jalan dan mati saat Stop semua.</span>
    <span class="spacer"></span>
    <button class="btn btn-quiet" type="button" data-action="toggle-db-autostart" ${
      busy ? "disabled" : ""
    }>${db.autostart ? "Matikan" : "Aktifkan"}</button>
  </div>`);

  return rows.join("");
}

function bindRuntime(): void {
  const select = document.getElementById("default-php") as HTMLSelectElement | null;
  select?.addEventListener("change", () => {
    if (select.value) void setDefaultPhp(select.value);
  });
}

function bindSiteDetail(site: SiteView): void {
  const select = document.getElementById("site-php") as HTMLSelectElement | null;
  select?.addEventListener("change", () => {
    if (select.value && select.value !== site.php) {
      void setSitePhp(site.id, select.value);
    }
  });
}

async function setSitePhp(id: string, php: string): Promise<void> {
  busy = "site-php";
  notice = null;
  render();

  try {
    const next = await invoke<Snapshot>("set_site_php", { id, php });
    applySnapshot(next);
  } catch (err) {
    notice = `Gagal memakai PHP ${php}: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function setDefaultPhp(version: string): Promise<void> {
  busy = "default-php";

  try {
    const next = await invoke<Snapshot>("set_default_php", { php: version });
    applySnapshot(next);
  } catch (err) {
    notice = `Gagal mengubah PHP default: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function setPhpPath(enable: boolean): Promise<void> {
  busy = "php-path";
  notice = null;
  render();

  try {
    const next = await invoke<Snapshot>(enable ? "enable_php_path" : "disable_php_path");
    applySnapshot(next);
  } catch (err) {
    notice = enable
      ? `Gagal mendaftarkan PHP ke PATH: ${String(err)}`
      : `Gagal melepas PHP dari PATH: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function setComposerGlobalPath(enable: boolean): Promise<void> {
  busy = "composer-global";
  notice = null;
  render();

  try {
    const next = await invoke<Snapshot>(
      enable ? "enable_composer_global_path" : "disable_composer_global_path",
    );
    applySnapshot(next);
  } catch (err) {
    notice = enable
      ? `Gagal mendaftarkan Composer global ke PATH: ${String(err)}`
      : `Gagal melepas Composer global dari PATH: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

function renderCommands(site: SiteView): string {
  const loaded = commandsFor === site.id;
  const tsButton = `<button class="btn" type="button" data-action="run-ts" ${
    hasBun() ? "" : "disabled"
  }>Jalankan .ts</button>`;

  if (!loaded || (siteCommands === null && !commandsError)) {
    return '<p class="hint">Menyiapkan perintah...</p>';
  }
  if (commandsError) {
    return `<p class="field-error">${esc(commandsError)}</p>`;
  }

  const buttons = (siteCommands ?? [])
    .map(
      (item) =>
        `<button class="btn" type="button" data-action="run-command" data-command="${esc(
          item.command,
        )}">${esc(item.label)}</button>`,
    )
    .join("");

  const empty = buttons
    ? ""
    : '<p class="hint">Tidak ada yang bisa dijalankan. Folder ini tidak punya composer.json, artisan, spark, atau package.json.</p>';

  const bunNote = hasBun()
    ? ""
    : '<p class="hint">Pasang Bun di dasbor untuk menjalankan berkas TypeScript.</p>';

  return `${empty}
    ${buttons ? `<div class="command-row">${buttons}</div>` : ""}
    <div class="command-row">${tsButton}</div>
    ${bunNote}`;
}

/// Menu "Buka" untuk satu situs. Daftar aplikasi cukup dideteksi sekali;
/// hasilnya disimpan sampai aplikasi ditutup.
async function toggleOpenMenu(siteId: string): Promise<void> {
  if (openMenuFor === siteId) {
    openMenuFor = null;
    render();
    return;
  }

  openMenuFor = siteId;
  openMenuError = null;
  render();

  if (!openMenuItems) {
    await loadOpenTargets(siteId);
  } else {
    focusOpenMenu();
  }
}

async function loadOpenTargets(siteId: string): Promise<void> {
  openMenuItems = null;
  openMenuError = null;
  render();

  try {
    const items = await invoke<OpenTarget[]>("open_targets");
    if (openMenuFor === siteId) openMenuItems = items;
  } catch (err) {
    if (openMenuFor === siteId) openMenuError = String(err);
  }
  render();
  if (openMenuFor === siteId) focusOpenMenu();
}

function focusOpenMenu(): void {
  document.querySelector<HTMLElement>(".menu-item:not(:disabled)")?.focus();
}

function renderOpenMenu(site: SiteView): string {
  if (openMenuFor !== site.id) return "";

  const body = openMenuError
    ? `<p class="menu-hint">${esc(openMenuError)}</p>
       <button class="menu-item" type="button" data-action="reload-open-targets">Coba lagi</button>`
    : openMenuItems === null
      ? '<p class="menu-hint">Memeriksa aplikasi yang terpasang...</p>'
      : openMenuItems
          .map(
            (item) =>
              `<button class="menu-item" type="button" role="menuitem" data-action="open-in" data-target="${esc(
                item.id,
              )}" ${item.available ? "" : "disabled"}>${esc(item.label)}${
                item.available ? "" : '<span class="menu-note">tidak terpasang</span>'
              }</button>`,
          )
          .join("");

  return `<div class="menu" role="menu" aria-label="Buka project">${body}</div>`;
}

async function runOpen(target: string): Promise<void> {
  const site = snapshot?.sites.find((item) => item.id === openMenuFor);
  openMenuFor = null;
  render();
  if (!site) return;

  try {
    await invoke("open_in", { kind: "site", id: site.id, target });
  } catch (err) {
    notice = `Gagal membuka ${target}: ${String(err)}`;
    render();
  }
}

async function openDataFolder(): Promise<void> {
  try {
    await invoke("open_in", { kind: "data", id: null, target: "explorer" });
  } catch (err) {
    notice = `Gagal membuka folder data: ${String(err)}`;
    render();
  }
}

function paintCommands(): void {
  const node = document.getElementById("commands");
  const site = snapshot?.sites.find((item) => item.id === selectedId);
  if (node && site) node.innerHTML = renderCommands(site);
}

async function loadCommands(id: string): Promise<void> {
  if (commandsFor === id) return;

  commandsFor = id;
  siteCommands = null;
  commandsError = null;
  paintCommands();

  try {
    const list = await invoke<SiteCommand[]>("site_commands", { id });
    // Situs lain bisa dipilih saat permintaan ini masih berjalan.
    if (commandsFor === id) siteCommands = list;
  } catch (err) {
    if (commandsFor === id) {
      siteCommands = [];
      commandsError = String(err);
    }
  }

  paintCommands();
}

async function runCommand(command: string): Promise<void> {
  const site = snapshot?.sites.find((item) => item.id === selectedId);
  if (!site) return;
  await terminalPanel.run(site.root, command);
}

async function runTs(): Promise<void> {
  const site = snapshot?.sites.find((item) => item.id === selectedId);
  if (!site || !hasBun()) return;

  const file = await open({
    title: "Pilih berkas TypeScript",
    multiple: false,
    filters: [{ name: "TypeScript", extensions: ["ts", "tsx", "mts"] }],
  });
  if (typeof file !== "string") return;

  await terminalPanel.run(site.root, `bun "${file}"`);
}

async function installBinary(target: "caddy" | "bun" | "composer"): Promise<void> {
  notice = null;
  download = { target, version: "", received: 0, total: null, stage: "daftar" };
  render();

  try {
    const next = await invoke<Snapshot>(`install_${target}`);
    applySnapshot(next);
  } catch (err) {
    const label = target === "caddy" ? "Caddy" : target === "bun" ? "Bun" : "Composer";
    notice = `Gagal memasang ${label}: ${String(err)}`;
  } finally {
    download = null;
    render();
    void loadRuntimes();
  }
}

/// Menutup panel panduan, baik setelah selesai maupun saat dilewati.
async function finishOnboarding(): Promise<void> {
  busy = "onboarding";
  notice = null;
  render();

  try {
    const next = await invoke<Snapshot>("finish_onboarding");
    applySnapshot(next);
  } catch (err) {
    notice = `Gagal menutup panduan: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function exportConfig(): Promise<void> {
  const path = await save({
    title: "Simpan config DevKu",
    defaultPath: "devku-config.json",
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (!path) return;

  busy = "export-config";
  configNotice = null;
  render();

  try {
    await invoke("export_config", { path });
    configNotice = `Config diekspor ke ${path}`;
  } catch (err) {
    notice = `Gagal mengekspor config: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function importConfig(): Promise<void> {
  const path = await open({
    title: "Pilih config DevKu",
    multiple: false,
    directory: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (typeof path !== "string") return;

  const answer = await openDialog({
    title: "Impor config",
    body: "Impor mengganti daftar situs, PHP default, dan TLD di mesin ini. Folder project tidak disentuh. Situs yang foldernya tidak ada akan dilewati.",
    rows: [{ label: "Berkas", value: path }],
    choices: [{ id: "import", label: "Impor", variant: "primary" }],
    tone: "warning",
  });
  if (answer?.choice !== "import") return;

  busy = "import-config";
  notice = null;
  configNotice = null;
  render();

  try {
    const result = await invoke<ImportResponse>("import_config", { path });
    applySnapshot(result.snapshot);
    configNotice = summarizeImport(result.report);
  } catch (err) {
    notice = `Gagal mengimpor config: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

function summarizeImport(report: ImportReport): string {
  const parts = [`${report.imported} situs diimpor`];
  if (report.skipped.length) {
    parts.push(`dilewati: ${report.skipped.join(", ")} (folder tidak ada)`);
  }
  if (report.disabled.length) {
    parts.push(`nonaktif: ${report.disabled.join(", ")} (PHP belum terpasang)`);
  }
  if (report.active_php_cleared) {
    parts.push(`PHP default dikosongkan (${report.active_php_cleared} belum terpasang)`);
  }
  return `${parts.join(". ")}.`;
}

async function setStartAtLogin(enable: boolean): Promise<void> {
  busy = "start-login";
  configNotice = null;
  render();

  try {
    const next = await invoke<Snapshot>("set_start_at_login", { enabled: enable });
    applySnapshot(next);
    configNotice = enable
      ? "DevKu akan dibuka saat login berikutnya."
      : "DevKu tidak lagi dibuka saat login.";
  } catch (err) {
    notice = `Gagal mengubah autostart: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function trustCaddy(): Promise<void> {  notice = null;
  render();
  try {
    const next = await invoke<Snapshot>("trust_caddy");
    applySnapshot(next);
  } catch (err) {
    notice = `Gagal memercayai sertifikat: ${String(err)}`;
  }
  render();
}

function stageLabel(job: DownloadProgress): string {  if (job.stage === "unduh") {
    return job.total
      ? `${megabytes(job.received)} / ${megabytes(job.total)}`
      : `${megabytes(job.received)} terunduh`;
  }
  if (job.stage === "pasang") return "memasang...";
  if (job.stage === "selesai") return "selesai";
  return "menyiapkan...";
}

const megabytes = (bytes: number) => `${(bytes / 1_048_576).toFixed(1).replace(".", ",")} MB`;

async function installPhp(version: string): Promise<void> {
  notice = null;
  download = { target: "php", version, received: 0, total: null, stage: "daftar" };
  render();

  try {
    const next = await invoke<Snapshot>("install_php", { version });
    applySnapshot(next);

    if (wizard && !wizard.php && next.installed_php.includes(version)) {
      wizard.php = version;
    }
  } catch (err) {
    notice = `Gagal memasang PHP ${version}: ${String(err)}`;
  } finally {
    download = null;
    // Penanda "terpasang" berubah, jadi daftar diambil ulang.
    phpCatalog = null;
    render();
    void loadPhpCatalog();
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

  let lastDate = "";
  node.className = "log";
  node.innerHTML = logLines
    .map((line) => {
      const entry = parseAccessLog(line);
      if (!entry) {
        const raw = line.length > 500 ? `${line.slice(0, 500)}...` : line;
        return `<div class="log-raw">${esc(raw)}</div>`;
      }

      // Tanggal hanya ditulis sekali per kelompok supaya kolom jam tetap sempit.
      const head =
        entry.date === lastDate
          ? ""
          : `<div class="log-date">${esc(entry.date)}</div>`;
      lastDate = entry.date;
      return head + renderLogRow(entry);
    })
    .join("");
  node.scrollTop = node.scrollHeight;
}

function renderLogRow(entry: AccessLog): string {
  const meta = [
    entry.durationMs === null ? "" : formatDuration(entry.durationMs),
    entry.size === null ? "" : formatBytes(entry.size),
  ]
    .filter(Boolean)
    .join(" · ");

  return `<div class="log-row">
    <span class="log-time">${esc(entry.time)}</span>
    <span class="log-status" data-tone="${statusTone(entry.status)}">${entry.status}</span>
    <span class="log-method">${esc(entry.method)}</span>
    <span class="log-uri">${esc(entry.uri)}</span>
    <span class="log-meta">${esc(meta)}</span>
  </div>`;
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
  const message = notice ?? infoNotice;
  if (!message) {
    node.hidden = true;
    node.innerHTML = "";
    return;
  }

  node.hidden = false;
  node.dataset.kind = notice ? "error" : "info";
  node.innerHTML = `
    <span class="notice-text">${esc(message)}</span>
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
    const firstRun = snapshot !== null && !snapshot.onboarded;
    list.innerHTML = firstRun
      ? `<div class="state">
        <h2>Belum ada situs</h2>
        <p>Selesaikan panduan di panel kanan untuk menambahkan situs pertama.</p>
      </div>`
      : `<div class="state">
        <h2>Belum ada situs</h2>
        <p>Tambahkan folder project. DevKu mengenali Laravel, CodeIgniter 3, CodeIgniter 4, atau PHP biasa dari isi foldernya.</p>
        <button class="btn btn-primary" type="button" data-action="open-wizard">Tambah situs</button>
      </div>`;
    return;
  }

  // Dasbor selalu ada di sini supaya panel runtime bisa dibuka lagi tanpa
  // reload webview.
  const dashboard = `
    <div class="site-row">
      <span class="rail" aria-hidden="true"></span>
      <button class="row-main btn-quiet" type="button" data-action="dashboard" aria-current="${
        selectedId === null
      }">
        <span class="row-name">Dasbor</span>
        <span class="row-meta">runtime, PHP, dan versi tersedia</span>
      </button>
    </div>`;

  list.innerHTML =
    dashboard +
    snapshot.sites
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
            <button class="btn btn-quiet" type="button" data-action="toggle-site" data-id="${esc(
              site.id,
            )}">${site.enabled ? "Matikan" : "Aktifkan"}</button>
            <button class="btn btn-quiet" type="button" data-action="ask-remove" data-id="${esc(
              site.id,
            )}">Hapus</button>
          </span>
        </div>`;
    })
    .join("");
}

/// Ekspor/impor config untuk berbagi environment antar developer.
function renderConfigSection(): string {
  const status = configNotice
    ? `<p class="hint config-status" role="status">${esc(configNotice)}</p>`
    : "";

  return `
    <div class="log-head">
      <h3>Konfigurasi</h3>
    </div>
    <div class="path-row">
      <span class="catalog-version">Mulai saat login</span>
      ${snapshot?.start_at_login ? '<span class="tag">aktif</span>' : '<span class="tag">tidak</span>'}
      <span class="hint">Dibuka tanpa jendela saat login, service langsung dinyalakan.</span>
      <span class="spacer"></span>
      ${
        snapshot?.start_at_login
          ? `<button class="btn btn-quiet" type="button" data-action="disable-start-login" ${
              busy ? "disabled" : ""
            }>Matikan</button>`
          : `<button class="btn" type="button" data-action="enable-start-login" ${
              busy ? "disabled" : ""
            }>Aktifkan</button>`
      }
    </div>
    <div class="path-row">
      <span class="catalog-version">config.json</span>
      <span class="hint config-path">${esc(snapshot?.config_path ?? "config.json")}</span>
      <span class="spacer"></span>
      <button class="btn" type="button" data-action="export-config" ${
        busy ? "disabled" : ""
      }>Ekspor</button>
      <button class="btn" type="button" data-action="import-config" ${
        busy ? "disabled" : ""
      }>Impor</button>
    </div>
    <div class="path-row">
      <span class="catalog-version">Folder data</span>
      <span class="hint config-path">${esc(snapshot?.home_path ?? "")}</span>
      <span class="spacer"></span>
      <button class="btn" type="button" data-action="open-data-folder">Buka folder</button>
    </div>
    <p class="hint">Impor mengganti daftar situs. Folder project tidak disentuh; situs yang foldernya tidak ada dilewati, dan situs dengan PHP belum terpasang dibuat nonaktif.</p>
    ${status}`;
}

function caddyAction(): string {  return snapshot?.caddy_installed
    ? '<span class="tag">terpasang</span>'
    : downloadButton("caddy");
}

/// Panduan saat pertama dibuka. Status tiap langkah dibaca dari snapshot, bukan
/// disimpan terpisah, jadi tidak bisa basi setelah pemasangan selesai.
function renderOnboarding(): string {
  const php = snapshot?.installed_php ?? [];
  const caddyReady = snapshot?.caddy_installed ?? false;
  const ready = caddyReady && php.length > 0;

  const httpsRow = snapshot?.https_trusted
    ? '<p class="hint">HTTPS lokal sudah dipercaya browser.</p>'
    : `<p class="hint">HTTPS lokal opsional. <button class="btn btn-quiet" type="button" data-action="trust-caddy" ${
        caddyReady && snapshot?.running ? "" : "disabled"
      }>Percayai sekarang</button>${
        caddyReady && snapshot?.running ? "" : ` ${esc(httpsHint())}`
      }</p>`;

  const phpBody = php.length
    ? `<div class="onboard-tags">${php
        .map((version) => `<span class="tag">${esc(version)}</span>`)
        .join("")}</div>
       <p class="hint">Versi terbaru dipakai untuk situs baru. Versi lain bisa ditambah dari dasbor.</p>`
    : `<p class="hint">Belum ada PHP. Pilih satu versi di bawah; DevKu mengunduh lalu memasangnya.</p>
       <div id="php-catalog">${renderPhpCatalog()}</div>`;

  return `
    <div class="detail-head"><h2>Mulai</h2></div>
    <div class="detail-body">
      <p class="hint onboard-intro">Tiga langkah ini cukup untuk membuka situs pertama. Semua pengaturan tetap bisa diubah dari dasbor.</p>

      <div class="onboard-step" data-done="${caddyReady}">
        <span class="onboard-num" aria-hidden="true">1</span>
        <div class="onboard-main">
          <h3>Web server</h3>
          <p class="hint">Caddy melayani semua domain di port 80 dan 443, tanpa admin.</p>
          ${caddyReady ? httpsRow : ""}
        </div>
        <span id="onboarding-caddy">${caddyAction()}</span>
      </div>

      <div class="onboard-step" data-done="${php.length > 0}">
        <span class="onboard-num" aria-hidden="true">2</span>
        <div class="onboard-main">
          <h3>PHP</h3>
          ${phpBody}
        </div>
      </div>

      <div class="onboard-step" data-done="${ready}">
        <span class="onboard-num" aria-hidden="true">3</span>
        <div class="onboard-main">
          <h3>Situs pertama</h3>
          <p class="hint">${
            ready
              ? "Pilih folder project. Laravel, CodeIgniter, dan PHP biasa dikenali otomatis."
              : "Pasang Caddy dan satu versi PHP dulu, lalu langkah ini terbuka."
          }</p>
        </div>
      </div>

      <div class="onboard-foot">
        <button class="btn btn-primary" type="button" data-action="open-wizard" ${
          ready && !busy ? "" : "disabled"
        }>Tambah situs</button>
        <button class="btn btn-quiet" type="button" data-action="finish-onboarding" ${
          busy ? "disabled" : ""
        }>Lewati panduan</button>
      </div>
    </div>`;
}

function renderDetail(): void {
  const pane = el("detail");

  if (wizard) {
    pane.innerHTML = renderWizard();
    bindWizard();
    if (document.getElementById("php-catalog")) void loadPhpCatalog();
    return;
  }

  const firstRun = snapshot !== null && !snapshot.onboarded && snapshot.sites.length === 0;
  if (firstRun) {
    pane.innerHTML = renderOnboarding();
    if (document.getElementById("php-catalog")) void loadPhpCatalog();
    return;
  }

  const site = snapshot?.sites.find((item) => item.id === selectedId);
  if (!site) {
    const rows = (runtimesList ?? [])
      .map((runtime) => {
        const installed = runtime.source !== "tidak ada" && runtime.version;
        const value = installed
          ? `<span>${esc(runtime.version!)}</span> · ${esc(runtime.source)}${
              runtime.path ? ` · ${esc(runtime.path)}` : ""
            }`
          : "tidak terpasang";
        return `<dt>${esc(runtime.name)}</dt><dd>${value}</dd>`;
      })
      .join("");

    const body = runtimesError
      ? `<p class="field-error">${esc(runtimesError)}</p>`
      : runtimesList === null
        ? "<p class=\"hint\">Memeriksa runtime...</p>"
        : rows
          ? `<dl class="facts">${rows}</dl>`
          : '<p class="hint">Tidak ada runtime yang terdeteksi.</p>';

    pane.innerHTML = `
      <div class="detail-head"><h2>Runtime</h2></div>
      <div class="detail-body">
        <p class="hint">Pilih salah satu situs untuk melihat domain, port PHP, folder, dan log-nya.</p>
        <div class="log-head">
          <h3>Runtime sistem</h3>
          <button class="btn" type="button" data-action="refresh-runtimes">Muat ulang</button>
        </div>
        ${body}

        <div class="log-head">
          <h3>Binary dan PATH</h3>
        </div>
        <div id="binaries">${renderBinaries()}</div>

        <div class="log-head">
          <h3>PHP</h3>
          <button class="btn" type="button" data-action="refresh-php">Muat ulang</button>
        </div>
        ${renderPhpSettings()}
        <div class="log-head"><h3>Pasang versi</h3></div>
        <div id="php-catalog">${renderPhpCatalog()}</div>
        <div class="log-head"><h3>Database</h3></div>
        <div id="database-panel">${renderDatabase()}</div>
        ${renderConfigSection()}
      </div>`;
    bindRuntime();
    void loadPhpCatalog();
    return;
  }

  const status = `${STATUS_WORD[site.status]}${site.detail ? ` (${site.detail})` : ""}`;

  const installed = snapshot?.installed_php ?? [];
  const phpOptions = installed
    .map(
      (version) =>
        `<option value="${esc(version)}" ${version === site.php ? "selected" : ""}>${esc(
          version,
        )}</option>`,
    )
    .join("");
  const phpMissing = !installed.includes(site.php);
  const canOpen = site.enabled && site.status === "ready";

  const phpField = installed.length
    ? `<div class="field">
        <label class="field-label" for="site-php">Versi PHP</label>
        <select class="input" id="site-php" ${busy ? "disabled" : ""}>
          ${
            phpMissing
              ? `<option value="${esc(site.php)}" selected>${esc(site.php)} (tidak terpasang)</option>`
              : ""
          }
          ${phpOptions}
        </select>
        ${phpMissing ? `<p class="field-error">PHP ${esc(site.php)} tidak terpasang. Pilih versi lain.</p>` : ""}
        <p class="hint">Saat diganti, pool versi baru dinyalakan lalu yang lama dimatikan.</p>
      </div>`
    : "";

  pane.innerHTML = `
    <div class="detail-head">
      <h2>${esc(site.name)}</h2>
      <span class="spacer"></span>
      <div class="menu-wrap">
        <button class="btn btn-quiet" type="button" data-action="toggle-open-menu" data-id="${esc(
          site.id,
        )}" aria-haspopup="menu" aria-expanded="${openMenuFor === site.id}">Buka<span class="caret" aria-hidden="true">▾</span></button>
        ${renderOpenMenu(site)}
      </div>
    </div>
    <div class="detail-body">
      <dl class="facts">
        <dt>Domain</dt>
        <dd class="domain-cell">
          <span>${esc(site.domain)}</span>
          <span class="domain-links">
            <button class="btn btn-quiet" type="button" data-action="open-site" data-url="http://${esc(
              site.domain,
            )}" aria-label="Buka http://${esc(site.domain)}" ${canOpen ? "" : "disabled"}>http</button>
            <button class="btn btn-quiet" type="button" data-action="open-site" data-url="https://${esc(
              site.domain,
            )}" aria-label="Buka https://${esc(site.domain)}" ${canOpen ? "" : "disabled"}>https</button>
          </span>
          ${canOpen ? "" : '<span class="hint">situs belum siap</span>'}
        </dd>
        <dt>Framework</dt><dd>${esc(frameworkLabel(site.framework))}</dd>
        <dt>Port</dt><dd>${site.port ? `127.0.0.1:${site.port}` : "-"}</dd>
        <dt>Folder</dt><dd>${esc(site.root)}</dd>
        ${
          site.root !== site.path
            ? `<dt>Akar web</dt><dd>${esc(site.path)}</dd>`
            : ""
        }
        <dt>Status</dt><dd>${esc(status)}</dd>
      </dl>

      ${phpField}

      <div class="log-head">
        <h3>Perintah</h3>
        <button class="btn" type="button" data-action="refresh-commands">Muat ulang</button>
      </div>
      <div id="commands">${renderCommands(site)}</div>

      <div class="log-head">
        <h3>Log</h3>
        <span class="hint">200 baris terakhir</span>
        <button class="btn" type="button" data-action="refresh-log">Muat ulang</button>
      </div>
      <div class="log log-empty" id="log"></div>
    </div>`;

  bindSiteDetail(site);
  void loadCommands(site.id);
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
      : '<p class="hint">Belum ada PHP terpasang. Pilih versi di bawah, DevKu yang mengunduh dan memasangnya.</p>';

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

      <div class="log-head">
        <h3>Pasang versi</h3>
        <button class="btn" type="button" data-action="refresh-php">Muat ulang</button>
      </div>
      <div id="php-catalog">${renderPhpCatalog()}</div>

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
        // Tanpa penanda PHP: tetap lanjut, akar web = folder yang dipilih.
        w.detection = {
          framework: "lain",
          label: "Lain (tanpa PHP)",
          document_root: path,
          project_root: path,
        };
      } else {
        w.detection = found;
      }
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

    // Situs pertama menutup panduan, jadi flag-nya tidak tertinggal bohong.
    if (!next.onboarded) {
      void invoke<Snapshot>("finish_onboarding").then(applySnapshot).catch(() => {});
    }

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

async function askRemoveSite(id: string): Promise<void> {
  const site = snapshot?.sites.find((item) => item.id === id);
  if (!site) return;

  const answer = await openDialog({
    title: `Hapus situs ${site.name}?`,
    body: "Situs dihapus dari daftar dan berhenti dilayani. Folder project tidak disentuh.",
    rows: [
      { label: "Domain", value: site.domain },
      { label: "PHP", value: site.php },
      { label: "Folder", value: site.root },
    ],
    choices: [{ id: "remove", label: "Hapus situs", variant: "danger" }],
    tone: "danger",
  });
  if (answer?.choice !== "remove") return;
  void removeSite(id);
}

async function removeSite(id: string): Promise<void> {
  busy = "remove";
  try {
    const next = await invoke<Snapshot>("remove_site", { id });
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

async function editPhpIni(version: string): Promise<void> {
  try {
    const editor = await invoke<string>("open_php_ini", { version });
    infoNotice = `php.ini ${version} dibuka di ${editor}.`;
    notice = null;
  } catch (err) {
    notice = `Gagal membuka php.ini: ${String(err)}`;
    infoNotice = null;
  }
  render();
}

async function restartPhp(version: string): Promise<void> {
  busy = "restart-php";
  notice = null;
  infoNotice = null;
  render();

  try {
    const next = await invoke<Snapshot>("restart_php", { version });
    applySnapshot(next);
    infoNotice = `PHP ${version} sudah dimulai ulang.`;
  } catch (err) {
    notice = `Gagal memulai ulang PHP ${version}: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function askRemovePhp(version: string): Promise<void> {
  const path = snapshot?.php_path;
  const onPath = Boolean(path?.registered && path.shim_version === version);
  const sites = (snapshot?.sites ?? []).filter((site) => site.php === version);
  const enabled = sites.filter((site) => site.enabled);
  const isDefault = snapshot?.default_php === version;

  const rows = [{ label: "Versi", value: version }];
  if (sites.length > 0) {
    rows.push({ label: "Dipakai", value: sites.map((site) => site.name).join(", ") });
  }
  if (onPath) {
    rows.push({ label: "Shim PATH", value: `php → ${version}` });
  }
  if (isDefault) {
    rows.push({ label: "PHP default", value: "pindah ke versi terbaru yang tersisa" });
  }

  const sentences = ["Folder versi ini dihapus dari disk. Folder project tidak disentuh."];
  if (enabled.length > 0) {
    sentences.push(`${enabled.length} situs dinonaktifkan karena PHP-nya hilang.`);
  }
  if (onPath) {
    sentences.push("Versi ini dipakai perintah php di PATH, jadi tidak bisa dihapus sendirian.");
  }

  const answer = await openDialog({
    title: onPath ? `PHP ${version} dipakai PATH` : `Hapus PHP ${version}?`,
    body: sentences.join(" "),
    rows,
    choices: onPath
      ? [{ id: "all", label: "Hapus semua PHP", variant: "danger" }]
      : [{ id: "remove", label: "Hapus PHP", variant: "danger" }],
    tone: "danger",
  });
  if (!answer) return;

  busy = "remove-php";
  notice = null;
  infoNotice = null;
  render();

  try {
    const result =
      answer.choice === "all"
        ? await invoke<PhpRemovalResponse>("remove_all_php")
        : await invoke<PhpRemovalResponse>("remove_php", { version });
    applySnapshot(result.snapshot);
    infoNotice = summarizeRemoval(result.removal);
  } catch (err) {
    notice = `Gagal menghapus PHP: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

function summarizeRemoval(removal: PhpRemoval): string {
  const parts: string[] = [];
  if (removal.removed_all) {
    parts.push("Semua PHP dihapus dari disk dan dilepas dari PATH.");
  } else if (removal.version) {
    parts.push(`PHP ${removal.version} dihapus dari disk.`);
  }
  if (removal.disabled_sites.length > 0) {
    parts.push(`${removal.disabled_sites.length} situs dinonaktifkan.`);
  }
  parts.push(
    removal.default_php
      ? `PHP default sekarang ${removal.default_php}.`
      : "Belum ada PHP terpasang.",
  );
  if (removal.composer_moved) {
    parts.push("Composer diarahkan ke PHP default baru.");
  }
  return parts.join(" ");
}

async function installDatabase(): Promise<void> {
  notice = null;
  infoNotice = null;
  download = { target: "mariadb", version: "", received: 0, total: null, stage: "daftar" };
  render();

  try {
    const next = await invoke<Snapshot>("install_database");
    applySnapshot(next);
    infoNotice = "MariaDB terpasang dan siap dipakai.";
  } catch (err) {
    notice = `Gagal memasang MariaDB: ${String(err)}`;
  } finally {
    download = null;
    render();
  }
}

async function databaseAction(command: string): Promise<void> {
  busy = "database";
  notice = null;
  render();

  try {
    const next = await invoke<Snapshot>(command);
    applySnapshot(next);
  } catch (err) {
    notice = `Gagal mengubah database: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function toggleDatabaseAutostart(): Promise<void> {
  const enabled = !(snapshot?.database.autostart ?? false);
  busy = "database";
  notice = null;
  render();

  try {
    const next = await invoke<Snapshot>("set_database_autostart", { enabled });
    applySnapshot(next);
  } catch (err) {
    notice = `Gagal mengubah autostart database: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function resetDatabasePassword(): Promise<void> {
  const answer = await openDialog({
    title: "Atur password root",
    body: "Password berlaku untuk semua akun root. Kosongkan untuk kembali tanpa password.",
    fields: [
      {
        id: "password",
        label: "Password baru",
        type: "password",
        placeholder: "kosong berarti tanpa password",
      },
    ],
    choices: [{ id: "save", label: "Simpan", variant: "primary" }],
  });
  if (answer?.choice !== "save") return;

  const password = answer.values.password ?? "";
  busy = "database";
  notice = null;
  render();

  try {
    const next = await invoke<Snapshot>("reset_database_password", { password });
    applySnapshot(next);
    infoNotice = password ? "Password root diperbarui." : "Password root dikosongkan.";
  } catch (err) {
    notice = `Gagal mengubah password root: ${String(err)}`;
  } finally {
    busy = null;
    render();
  }
}

async function askRemoveDatabase(): Promise<void> {
  const db = snapshot?.database;
  if (!db) return;

  const canBackup = Boolean(snapshot?.running && db.installed && db.initialized);
  const checkboxes: DialogCheckbox[] = [
    {
      id: "data",
      label: "Hapus juga data",
      hint: "Database dan tabel di folder data ikut terhapus.",
    },
  ];
  if (canBackup) {
    checkboxes.push({
      id: "backup",
      label: "Backup dulu ke file .sql",
      hint: "Semua database buatan user disimpan ke folder backup DevKu.",
      checked: true,
      requires: "data",
    });
  }

  const answer = await openDialog({
    title: `Hapus ${db.label}?`,
    body: canBackup
      ? "Binari dihapus dari disk. Folder project tidak disentuh."
      : "Binari dihapus dari disk. Backup belum bisa dibuat karena service belum berjalan.",
    rows: [
      { label: "Versi", value: db.version },
      { label: "Folder data", value: db.data_path },
    ],
    checkboxes,
    choices: [{ id: "remove", label: "Hapus", variant: "danger" }],
    tone: "danger",
  });
  if (answer?.choice !== "remove") return;

  busy = "database";
  notice = null;
  render();

  try {
    const result = await invoke<DatabaseRemovalResponse>("remove_database", {
      removeData: Boolean(answer.checked.data),
      backup: Boolean(answer.checked.backup),
    });
    applySnapshot(result.snapshot);

    if (result.backup) {
      infoNotice = `${db.label} dihapus. Backup disimpan di ${result.backup}.`;
      // Folder backup dibuka supaya berkasnya langsung terlihat.
      await invoke<string>("open_backups").catch(() => "");
    } else if (result.removal.removed_data) {
      infoNotice = `${db.label} dan datanya dihapus.`;
    } else {
      infoNotice = `${db.label} dihapus. Data tetap disimpan di ${db.data_path}.`;
    }
  } catch (err) {
    notice = `Gagal menghapus ${db.label}: ${String(err)}`;
  } finally {
    busy = null;
    render();
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
    case "finish-onboarding":
      void finishOnboarding();
      break;
    case "export-config":
      void exportConfig();
      break;
    case "import-config":
      void importConfig();
      break;
    case "enable-start-login":
      void setStartAtLogin(true);
      break;
    case "disable-start-login":
      void setStartAtLogin(false);
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
    case "dashboard":
      selectedId = null;
      logFor = null;
      openMenuFor = null;
      render();
      break;
    case "select":
      if (id) {
        selectedId = id;
        logFor = null;
        openMenuFor = null;
        render();
      }
      break;
    case "toggle-open-menu":
      if (id) void toggleOpenMenu(id);
      break;
    case "reload-open-targets":
      if (openMenuFor) void loadOpenTargets(openMenuFor);
      break;
    case "open-in": {
      const openTarget = target.dataset.target;
      if (openTarget) void runOpen(openTarget);
      break;
    }
    case "open-data-folder":
      void openDataFolder();
      break;
    case "toggle-site":
      if (id) void toggleSite(id);
      break;
    case "ask-remove":
      if (id) void askRemoveSite(id);
      break;
    case "edit-php-ini": {
      const version = target.dataset.version;
      if (version) void editPhpIni(version);
      break;
    }
    case "restart-php": {
      const version = target.dataset.version;
      if (version) void restartPhp(version);
      break;
    }
    case "ask-remove-php": {
      const version = target.dataset.version;
      if (version) void askRemovePhp(version);
      break;
    }
    case "install-database":
      void installDatabase();
      break;
    case "start-database":
      void databaseAction("start_database");
      break;
    case "stop-database":
      void databaseAction("stop_database");
      break;
    case "toggle-db-autostart":
      void toggleDatabaseAutostart();
      break;
    case "reset-db-password":
      void resetDatabasePassword();
      break;
    case "ask-remove-database":
      void askRemoveDatabase();
      break;
    case "refresh-log":
      if (selectedId) void loadLog(selectedId);
      break;
    case "refresh-runtimes":
      runtimesList = null;
      renderDetail();
      void loadRuntimes();
      break;
    case "refresh-php":
      phpCatalog = null;
      phpCatalogError = null;
      paintCatalog();
      void loadPhpCatalog();
      break;
    case "install-php": {
      const version = target.dataset.version;
      if (version) void installPhp(version);
      break;
    }
    case "cancel-download":
      void invoke("cancel_download");
      break;
    case "enable-php-path":
      void setPhpPath(true);
      break;
    case "disable-php-path":
      void setPhpPath(false);
      break;
    case "enable-composer-global":
      void setComposerGlobalPath(true);
      break;
    case "disable-composer-global":
      void setComposerGlobalPath(false);
      break;
    case "install-caddy":
      void installBinary("caddy");
      break;
    case "install-bun":
      void installBinary("bun");
      break;
    case "install-composer":
      void installBinary("composer");
      break;
    case "trust-caddy":
      void trustCaddy();
      break;
    case "run-command": {
      const command = target.dataset.command;
      if (command) void runCommand(command);
      break;
    }
    case "run-ts":
      void runTs();
      break;
    case "refresh-commands":
      if (selectedId) {
        commandsFor = null;
        void loadCommands(selectedId);
      }
      break;
    case "start":
      void start();
      break;
    case "open-dashboard":
      void openUrl("http://localhost/");
      break;
    case "open-site": {
      const url = target.dataset.url;
      if (url) void openUrl(url);
      break;
    }
    case "toggle-terminal": {
      const active = snapshot?.sites.find((site) => site.id === selectedId);
      terminalPanel.toggle(active?.root ?? null);
      break;
    }
    case "stop":
      void stop();
      break;
    case "retry":
      void load();
      break;
    case "dismiss-notice":
      notice = null;
      infoNotice = null;
      render();
      break;
  }
});

// Klik di luar menu menutupnya. Listener ini dipasang setelah handler utama,
// jadi klik pada tombol menu sendiri sudah ditangani lebih dulu.
document.addEventListener("click", (event) => {
  if (!openMenuFor) return;
  const node = event.target as HTMLElement | null;
  if (node?.closest(".menu-wrap")) return;
  openMenuFor = null;
  render();
});

document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape" || !openMenuFor) return;
  openMenuFor = null;
  render();
});

void listen<DownloadProgress>("download://progress", (event) => {
  download = event.payload;
  paintDownloads();
});

if (document.readyState === "loading") {
  window.addEventListener("DOMContentLoaded", () => void load());
} else {
  void load();
}
