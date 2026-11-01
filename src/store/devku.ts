import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  Snapshot,
  SiteView,
  RuntimeInfo,
  PhpCatalogEntry,
  PhpCatalogView,
  DownloadProgress,
  ImportResponse,
  ImportReport,
  PhpRemoval,
  PhpRemovalResponse,
  DatabaseRemovalResponse,
  SiteCommand,
  OpenTarget,
  DialogSpec,
  DialogCheckbox,
  DialogOutcome,
} from "../types";

export type MainTab = "sites" | "settings";
export type SiteSubTab = "general" | "logs" | "commands";
export type SettingsSubTab = "php" | "database" | "server" | "system";

export interface ActiveDialogState {
  spec: DialogSpec;
  values: Record<string, string>;
  checked: Record<string, boolean>;
  resolve: (outcome: DialogOutcome | null) => void;
}

export const FRAMEWORK_LABEL: Record<string, string> = {
  laravel: "Laravel",
  codeigniter4: "CodeIgniter 4",
  codeigniter3: "CodeIgniter 3",
  php: "PHP",
};

export const STATUS_WORD: Record<string, string> = {
  ready: "hidup",
  starting: "mulai",
  backoff: "ulang",
  failed: "gagal",
  stopped: "berhenti",
};

export function frameworkLabel(key: string): string {
  return FRAMEWORK_LABEL[key] ?? key;
}

// Reactive store
export const state = reactive({
  snapshot: null as Snapshot | null,
  loading: true,
  bootError: null as string | null,
  notice: null as string | null,
  infoNotice: null as string | null,
  busy: null as string | null,

  // Navigation
  activeTab: "sites" as MainTab,
  selectedSiteId: null as string | null,
  siteSubTab: "general" as SiteSubTab,
  settingsTab: "php" as SettingsSubTab,

  // Panels & Dialogs
  terminalOpen: false,
  wizardOpen: false,
  activeDialog: null as ActiveDialogState | null,

  // Logs & Commands
  logFor: null as string | null,
  logLines: [] as string[],
  logError: null as string | null,

  // Runtimes & Catalogs
  runtimesList: null as RuntimeInfo[] | null,
  runtimesError: null as string | null,

  phpCatalog: null as PhpCatalogEntry[] | null,
  phpCatalogLoading: false,
  phpCatalogError: null as string | null,
  phpCatalogWarning: null as string | null,

  download: null as DownloadProgress | null,

  siteCommands: null as SiteCommand[] | null,
  commandsFor: null as string | null,
  commandsError: null as string | null,

  openMenuItems: null as OpenTarget[] | null,
  openMenuFor: null as string | null,
  openMenuError: null as string | null,

  configNotice: null as string | null,
});

let pollTimer: number | null = null;
let lastSnapshotJson = "";
let downloadUnlisten: UnlistenFn | null = null;

export const currentSite = computed<SiteView | null>(() => {
  if (!state.snapshot || !state.selectedSiteId) return null;
  return state.snapshot.sites.find((s) => s.id === state.selectedSiteId) ?? null;
});

export const isGlobalRunning = computed<boolean>(() => {
  return Boolean(state.snapshot?.running);
});

export function clearNotices(): void {
  state.notice = null;
  state.infoNotice = null;
  state.configNotice = null;
}

export function promptDialog(spec: DialogSpec): Promise<DialogOutcome | null> {
  return new Promise((resolve) => {
    const values: Record<string, string> = {};
    const checked: Record<string, boolean> = {};

    (spec.fields ?? []).forEach((f) => {
      values[f.id] = f.value ?? "";
    });
    (spec.checkboxes ?? []).forEach((c) => {
      checked[c.id] = Boolean(c.checked);
    });

    state.activeDialog = {
      spec,
      values,
      checked,
      resolve: (outcome) => {
        state.activeDialog = null;
        resolve(outcome);
      },
    };
  });
}

function applySnapshot(next: Snapshot): void {
  const json = JSON.stringify(next);
  if (json === lastSnapshotJson) return;

  state.snapshot = next;
  lastSnapshotJson = json;

  // Keep selected site valid
  if (state.selectedSiteId && !next.sites.some((s) => s.id === state.selectedSiteId)) {
    state.selectedSiteId = null;
  }
}

export async function refresh(): Promise<void> {
  try {
    const next = await invoke<Snapshot>("snapshot");
    applySnapshot(next);
  } catch (err) {
    // If backend is dropping, keep UI informed
    state.notice = `Gagal menyegarkan status: ${String(err)}`;
  }
}

export async function load(): Promise<void> {
  state.loading = true;
  state.bootError = null;

  try {
    const next = await invoke<Snapshot>("snapshot");
    applySnapshot(next);
  } catch (err) {
    state.bootError = String(err);
  } finally {
    state.loading = false;
    schedulePoll();
  }

  if (!state.bootError) {
    void loadRuntimes();
    if (!downloadUnlisten) {
      listen<DownloadProgress>("download://progress", (event) => {
        state.download = event.payload;
      }).then((un) => {
        downloadUnlisten = un;
      });
    }
  }
}

function schedulePoll(): void {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
  }
  pollTimer = window.setInterval(() => {
    void refresh();
  }, 2000);
}

export async function startAll(): Promise<void> {
  state.busy = "start";
  state.notice = null;
  try {
    const next = await invoke<Snapshot>("start");
    applySnapshot(next);
  } catch (err) {
    state.notice = `Gagal memulai: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function stopAll(): Promise<void> {
  state.busy = "stop";
  state.notice = null;
  try {
    const next = await invoke<Snapshot>("stop");
    applySnapshot(next);
  } catch (err) {
    state.notice = `Gagal menghentikan: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function trustCaddy(): Promise<void> {
  state.busy = "trust-caddy";
  state.notice = null;
  try {
    const next = await invoke<Snapshot>("trust_caddy");
    applySnapshot(next);
    state.infoNotice = "Sertifikat root Caddy berhasil dipercaya.";
  } catch (err) {
    state.notice = `Gagal mempercayai sertifikat Caddy: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export function selectSite(id: string | null): void {
  state.selectedSiteId = id;
  state.logFor = null;
  state.openMenuFor = null;
  if (id) {
    void loadLog(id);
    void loadCommands(id);
  }
}

export async function toggleSite(id: string): Promise<void> {
  const site = state.snapshot?.sites.find((s) => s.id === id);
  if (!site) return;

  state.busy = "toggle-site";
  try {
    const next = await invoke<Snapshot>("set_site_enabled", {
      id,
      enabled: !site.enabled,
    });
    applySnapshot(next);
  } catch (err) {
    state.notice = `Gagal mengubah status situs: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function askRemoveSite(id: string): Promise<void> {
  const site = state.snapshot?.sites.find((s) => s.id === id);
  if (!site) return;

  const answer = await promptDialog({
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

  state.busy = "remove-site";
  try {
    const next = await invoke<Snapshot>("remove_site", { id });
    applySnapshot(next);
    if (state.selectedSiteId === id) state.selectedSiteId = null;
    state.infoNotice = `Situs ${site.name} berhasil dihapus.`;
  } catch (err) {
    state.notice = `Gagal menghapus situs: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function setSitePhp(siteId: string, version: string): Promise<void> {
  state.busy = "set-php";
  state.notice = null;
  try {
    const next = await invoke<Snapshot>("set_site_php", { id: siteId, version });
    applySnapshot(next);
    state.infoNotice = `Versi PHP untuk situs berhasil diubah ke ${version}.`;
  } catch (err) {
    state.notice = `Gagal mengganti versi PHP: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function loadOpenTargets(id: string): Promise<void> {
  try {
    state.openMenuItems = await invoke<OpenTarget[]>("open_targets", { id });
    state.openMenuError = null;
  } catch (err) {
    state.openMenuItems = [];
    state.openMenuError = String(err);
  }
}

export async function toggleOpenMenu(id: string): Promise<void> {
  if (state.openMenuFor === id) {
    state.openMenuFor = null;
    return;
  }
  state.openMenuFor = id;
  state.openMenuItems = null;
  state.openMenuError = null;
  await loadOpenTargets(id);
}

export async function runOpenIn(targetId: string): Promise<void> {
  const siteId = state.openMenuFor ?? state.selectedSiteId;
  if (!siteId) return;

  try {
    await invoke("open_in", { id: siteId, target: targetId });
  } catch (err) {
    state.notice = `Gagal membuka: ${String(err)}`;
  } finally {
    state.openMenuFor = null;
  }
}

export async function loadLog(id: string): Promise<void> {
  try {
    const lines = await invoke<string[]>("tail_log", { name: id, lines: 200 });
    state.logFor = id;
    state.logLines = lines;
    state.logError = null;
  } catch (err) {
    state.logFor = id;
    state.logLines = [];
    state.logError = String(err);
  }
}

export async function loadCommands(id: string): Promise<void> {
  if (state.commandsFor === id && state.siteCommands !== null) return;

  state.commandsFor = id;
  state.siteCommands = null;
  state.commandsError = null;

  try {
    const list = await invoke<SiteCommand[]>("site_commands", { id });
    if (state.commandsFor === id) state.siteCommands = list;
  } catch (err) {
    if (state.commandsFor === id) {
      state.siteCommands = [];
      state.commandsError = String(err);
    }
  }
}

export async function loadRuntimes(): Promise<void> {
  try {
    state.runtimesList = await invoke<RuntimeInfo[]>("runtimes");
    state.runtimesError = null;
  } catch (err) {
    state.runtimesList = [];
    state.runtimesError = String(err);
  }
}

export async function loadPhpCatalog(): Promise<void> {
  if (state.phpCatalogLoading) return;
  state.phpCatalogLoading = true;
  state.phpCatalogError = null;
  state.phpCatalogWarning = null;

  try {
    const view = await invoke<PhpCatalogView>("php_catalog");
    state.phpCatalog = view.entries;
    state.phpCatalogWarning = view.warning;
  } catch (err) {
    state.phpCatalog = [];
    state.phpCatalogError = String(err);
  } finally {
    state.phpCatalogLoading = false;
  }
}

export async function installPhp(version: string): Promise<void> {
  state.notice = null;
  state.download = { target: "php", version, received: 0, total: null, stage: "unduh" };

  try {
    const next = await invoke<Snapshot>("install_php", { version });
    applySnapshot(next);
    state.infoNotice = `PHP ${version} berhasil dipasang.`;
    void loadPhpCatalog();
  } catch (err) {
    state.notice = `Gagal memasang PHP ${version}: ${String(err)}`;
  } finally {
    state.download = null;
  }
}

export async function cancelDownload(): Promise<void> {
  await invoke("cancel_download").catch(() => {});
  state.download = null;
}

export async function setDefaultPhp(version: string): Promise<void> {
  state.busy = "default-php";
  state.notice = null;
  try {
    const next = await invoke<Snapshot>("set_default_php", { version });
    applySnapshot(next);
    state.infoNotice = `PHP default diubah ke ${version}.`;
  } catch (err) {
    state.notice = `Gagal mengubah PHP default: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function setPhpPath(enable: boolean, version?: string): Promise<void> {
  state.busy = "php-path";
  state.notice = null;
  try {
    const next = enable
      ? await invoke<Snapshot>("enable_php_path", { version })
      : await invoke<Snapshot>("disable_php_path");
    applySnapshot(next);
    state.infoNotice = enable
      ? `PHP ${next.php_path.shim_version ?? version ?? ""} disambungkan ke PATH.`
      : "PHP dilepas dari PATH.";
  } catch (err) {
    state.notice = `Gagal mengatur PATH: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function openPhpIni(version: string): Promise<void> {
  try {
    const editor = await invoke<string>("open_php_ini", { version });
    state.infoNotice = `php.ini ${version} dibuka di ${editor}.`;
  } catch (err) {
    state.notice = `Gagal membuka php.ini: ${String(err)}`;
  }
}

export async function restartPhp(version: string): Promise<void> {
  state.busy = "restart-php";
  state.notice = null;
  try {
    const next = await invoke<Snapshot>("restart_php", { version });
    applySnapshot(next);
    state.infoNotice = `PHP ${version} berhasil dimulai ulang.`;
  } catch (err) {
    state.notice = `Gagal memulai ulang PHP ${version}: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function askRemovePhp(version: string): Promise<void> {
  const path = state.snapshot?.php_path;
  const onPath = Boolean(path?.registered && path.shim_version === version);
  const sites = (state.snapshot?.sites ?? []).filter((s) => s.php === version);
  const enabled = sites.filter((s) => s.enabled);
  const isDefault = state.snapshot?.default_php === version;

  const rows = [{ label: "Versi", value: version }];
  if (sites.length > 0) {
    rows.push({ label: "Dipakai", value: sites.map((s) => s.name).join(", ") });
  }
  if (onPath) {
    rows.push({ label: "Shim PATH", value: `php → ${version}` });
  }
  if (isDefault) {
    rows.push({ label: "PHP default", value: "Pindah ke versi terbaru yang tersisa" });
  }

  const sentences = ["Folder versi ini dihapus dari disk. Folder project tidak disentuh."];
  if (enabled.length > 0) {
    sentences.push(`${enabled.length} situs dinonaktifkan karena PHP-nya hilang.`);
  }
  if (onPath) {
    sentences.push("Versi ini dipakai perintah php di PATH, jadi tidak bisa dihapus sendirian.");
  }

  const answer = await promptDialog({
    title: onPath ? `PHP ${version} dipakai PATH` : `Hapus PHP ${version}?`,
    body: sentences.join(" "),
    rows,
    choices: onPath
      ? [{ id: "all", label: "Hapus semua PHP", variant: "danger" }]
      : [{ id: "remove", label: "Hapus PHP", variant: "danger" }],
    tone: "danger",
  });

  if (!answer) return;

  state.busy = "remove-php";
  state.notice = null;

  try {
    const result =
      answer.choice === "all"
        ? await invoke<PhpRemovalResponse>("remove_all_php")
        : await invoke<PhpRemovalResponse>("remove_php", { version });
    applySnapshot(result.snapshot);
    state.infoNotice = summarizeRemoval(result.removal);
    void loadPhpCatalog();
  } catch (err) {
    state.notice = `Gagal menghapus PHP: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

function summarizeRemoval(removal: PhpRemoval): string {
  const parts: string[] = [];
  if (removal.removed_all) {
    parts.push("Semua PHP dihapus dari disk dan dilepas dari PATH.");
  } else if (removal.version) {
    parts.push(`PHP ${removal.version} dihapus dari disk.`);
  }
  if (removal.disabled_sites?.length > 0) {
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

export async function installDatabase(): Promise<void> {
  state.busy = "install-db";
  state.notice = null;
  try {
    const next = await invoke<Snapshot>("install_database");
    applySnapshot(next);
    state.infoNotice = "MariaDB berhasil dipasang.";
  } catch (err) {
    state.notice = `Gagal memasang MariaDB: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function databaseAction(action: "start_database" | "stop_database"): Promise<void> {
  state.busy = "database";
  state.notice = null;
  try {
    const next = await invoke<Snapshot>(action);
    applySnapshot(next);
  } catch (err) {
    state.notice = `Gagal mengoperasikan database: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function toggleDatabaseAutostart(): Promise<void> {
  const db = state.snapshot?.database;
  if (!db) return;
  state.busy = "db-autostart";
  try {
    const next = await invoke<Snapshot>("set_database_autostart", {
      enable: !db.autostart,
    });
    applySnapshot(next);
  } catch (err) {
    state.notice = `Gagal mengubah autostart database: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function resetDatabasePassword(): Promise<void> {
  const answer = await promptDialog({
    title: "Atur password root",
    body: "Password berlaku untuk semua akun root. Kosongkan untuk kembali tanpa password.",
    fields: [
      {
        id: "password",
        label: "Password baru",
        type: "password",
        placeholder: "Kosong berarti tanpa password",
      },
    ],
    choices: [{ id: "save", label: "Simpan", variant: "primary" }],
  });

  if (answer?.choice !== "save") return;

  const password = answer.values.password ?? "";
  state.busy = "database";
  state.notice = null;

  try {
    const next = await invoke<Snapshot>("reset_database_password", { password });
    applySnapshot(next);
    state.infoNotice = password ? "Password root diperbarui." : "Password root dikosongkan.";
  } catch (err) {
    state.notice = `Gagal mengubah password root: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function askRemoveDatabase(): Promise<void> {
  const db = state.snapshot?.database;
  if (!db) return;

  const canBackup = Boolean(state.snapshot?.running && db.installed && db.initialized);
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

  const answer = await promptDialog({
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

  state.busy = "database";
  state.notice = null;

  try {
    const result = await invoke<DatabaseRemovalResponse>("remove_database", {
      removeData: Boolean(answer.checked.data),
      backup: Boolean(answer.checked.backup),
    });
    applySnapshot(result.snapshot);

    if (result.backup) {
      state.infoNotice = `${db.label} dihapus. Backup disimpan di ${result.backup}.`;
      await invoke<string>("open_backups").catch(() => "");
    } else if (result.removal.removed_data) {
      state.infoNotice = `${db.label} dan datanya dihapus.`;
    } else {
      state.infoNotice = `${db.label} dihapus. Data tetap disimpan di ${db.data_path}.`;
    }
  } catch (err) {
    state.notice = `Gagal menghapus ${db.label}: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function openBackups(): Promise<void> {
  try {
    await invoke("open_backups");
  } catch (err) {
    state.notice = `Gagal membuka folder backup: ${String(err)}`;
  }
}

export async function openDataFolder(): Promise<void> {
  const path = state.snapshot?.database.data_path;
  if (!path) return;
  try {
    await invoke("open_in", { id: path, target: "explorer" });
  } catch (err) {
    state.notice = `Gagal membuka folder data: ${String(err)}`;
  }
}

export async function installBinary(target: "caddy" | "bun" | "composer"): Promise<void> {
  state.notice = null;
  state.download = { target, version: "", received: 0, total: null, stage: "daftar" };

  try {
    const next = await invoke<Snapshot>(`install_${target}`);
    applySnapshot(next);
    state.infoNotice = `${target.toUpperCase()} berhasil dipasang.`;
    void loadRuntimes();
  } catch (err) {
    state.notice = `Gagal memasang ${target}: ${String(err)}`;
  } finally {
    state.download = null;
  }
}

export async function setComposerGlobalPath(enable: boolean): Promise<void> {
  state.busy = "composer-global";
  state.notice = null;
  try {
    const next = enable
      ? await invoke<Snapshot>("enable_composer_global_path")
      : await invoke<Snapshot>("disable_composer_global_path");
    applySnapshot(next);
    state.infoNotice = enable
      ? "Global tool Composer disambungkan ke PATH."
      : "Global tool Composer dilepas dari PATH.";
  } catch (err) {
    state.notice = `Gagal mengatur PATH Composer: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function setStartAtLogin(enable: boolean): Promise<void> {
  state.busy = "start-login";
  state.configNotice = null;
  try {
    const next = await invoke<Snapshot>("set_start_at_login", { enable });
    applySnapshot(next);
    state.infoNotice = enable
      ? "DevKu akan dijalankan saat Windows login."
      : "DevKu tidak lagi dijalankan saat Windows login.";
  } catch (err) {
    state.notice = `Gagal mengatur startup Windows: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function clearDownloads(): Promise<void> {
  state.busy = "clear-downloads";
  try {
    const next = await invoke<Snapshot>("clear_downloads");
    applySnapshot(next);
    state.infoNotice = "Cache berkas unduhan telah dibersihkan.";
  } catch (err) {
    state.notice = `Gagal membersihkan unduhan: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function exportConfig(): Promise<void> {
  const path = await save({
    title: "Ekspor konfigurasi DevKu",
    defaultPath: "devku-config.json",
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (typeof path !== "string") return;

  state.busy = "export-config";
  try {
    await invoke("export_config", { path });
    state.configNotice = `Konfigurasi berhasil disimpan ke ${path}.`;
  } catch (err) {
    state.notice = `Gagal mengekspor config: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

export async function importConfig(): Promise<void> {
  const path = await open({
    title: "Pilih berkas konfigurasi",
    multiple: false,
    directory: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (typeof path !== "string") return;

  const answer = await promptDialog({
    title: "Impor konfigurasi",
    body: "Impor mengganti daftar situs, PHP default, dan TLD di mesin ini. Folder project tidak disentuh. Situs yang foldernya tidak ada akan dilewati.",
    rows: [{ label: "Berkas", value: path }],
    choices: [{ id: "import", label: "Impor", variant: "primary" }],
    tone: "warning",
  });
  if (answer?.choice !== "import") return;

  state.busy = "import-config";
  try {
    const result = await invoke<ImportResponse>("import_config", { path });
    applySnapshot(result.snapshot);
    state.configNotice = summarizeImport(result.report);
  } catch (err) {
    state.notice = `Gagal mengimpor config: ${String(err)}`;
  } finally {
    state.busy = null;
  }
}

function summarizeImport(report: ImportReport): string {
  const parts = [`${report.imported} situs diimpor`];
  if (report.skipped.length) {
    parts.push(`Dilewati: ${report.skipped.join(", ")} (folder tidak ada)`);
  }
  if (report.disabled.length) {
    parts.push(`Nonaktif: ${report.disabled.join(", ")} (PHP belum terpasang)`);
  }
  if (report.active_php_cleared) {
    parts.push(`PHP default dikosongkan (${report.active_php_cleared} belum terpasang)`);
  }
  return parts.join(". ") + ".";
}
