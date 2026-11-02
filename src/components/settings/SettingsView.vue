<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import {
  state,
  setDefaultPhp,
  setPhpPath,
  openPhpIni,
  restartPhp,
  loadPhpExtensions,
  togglePhpExtension,
  restartPhpForExtensions,
  askRemovePhp,
  loadPhpCatalog,
  installPhp,
  installDatabase,
  databaseAction,
  toggleDatabaseAutostart,
  resetDatabasePassword,
  askRemoveDatabase,
  openBackups,
  openDataFolder,
  openDataRoot,
  openWorkspace,
  setWorkspace,
  moveDataHome,
  promptDialog,
  trustCaddy,
  setApachePort,
  setComposerGlobalPath,
  installBinary,
  setStartAtLogin,
  clearDownloads,
  exportConfig,
  importConfig,
  type SettingsSubTab,
} from "../../store/devku";
import { open } from "@tauri-apps/plugin-dialog";
import { formatBytes } from "../../log-format";

onMounted(() => {
  if (!state.phpCatalog) {
    void loadPhpCatalog();
  }
  if (state.settingsTab === "php" && !state.phpExtensions) {
    const target = defaultExtensionTarget();
    if (target) void loadPhpExtensions(target);
  }
});

const installedPhp = computed(() => state.snapshot?.installed_php ?? []);
const defaultPhp = computed(() => state.snapshot?.default_php ?? "");
const phpPath = computed(() => state.snapshot?.php_path);
const database = computed(() => state.snapshot?.database);
const composerGlobal = computed(() => state.snapshot?.composer_global);
const apache = computed(() => state.snapshot?.apache);
const installingApache = computed(() => state.download?.target === "apache");
const apachePort = ref(8080);

watch(
  () => apache.value?.port,
  (port) => {
    if (port) apachePort.value = port;
  },
  { immediate: true },
);

async function saveApachePort() {
  const port = Number(apachePort.value);
  if (!Number.isInteger(port) || port < 1024) {
    state.notice = "Port Apache harus 1024 atau lebih.";
    return;
  }
  await setApachePort(port);
}

const dataLocation = computed(() => state.snapshot?.data_location);
const workspace = computed(() => state.snapshot?.workspace);
const movePercent = computed(() => {
  const progress = state.moveProgress;
  if (!progress || !progress.total) return 0;
  return Math.min(100, Math.round((progress.done / progress.total) * 100));
});

/// Moving the data folder copies hundreds of MB, so it always goes through a confirmation
/// that names the source and target folder.
async function pickDataFolder(): Promise<void> {
  if (dataLocation.value?.locked || state.busy === "move-data") return;
  try {
    const selected = await open({
      title: "Pilih folder data baru (harus kosong)",
      multiple: false,
      directory: true,
    });
    if (typeof selected !== "string") return;

    const answer = await promptDialog({
      title: "Pindahkan folder data",
      body: "Isi folder data disalin ke folder baru, service dihentikan selama penyalinan, lalu DevKu dimulai ulang. Folder lama dikosongkan setelah salinan selesai.",
      rows: [
        { label: "Dari", value: state.snapshot?.home_path ?? "" },
        { label: "Ke", value: selected },
      ],
      choices: [{ id: "move", label: "Pindahkan", variant: "primary" }],
      tone: "warning",
      cancelLabel: "Batal",
    });
    if (answer?.choice !== "move") return;

    await moveDataHome(selected);
  } catch (err) {
    state.notice = `Gagal memilih folder data: ${String(err)}`;
  }
}

async function pickWorkspace(): Promise<void> {
  try {
    const selected = await open({
      title: "Pilih folder workspace",
      multiple: false,
      directory: true,
      defaultPath: workspace.value?.exists ? workspace.value.path : undefined,
    });
    if (typeof selected !== "string") return;
    await setWorkspace(selected);
  } catch (err) {
    state.notice = `Gagal memilih folder workspace: ${String(err)}`;
  }
}

async function resetWorkspace(): Promise<void> {
  await setWorkspace(null);
}

/// Version picked in the Extensions tab: the one the user chose, or the global default,
/// or the highest one installed (installed_php list is ascending).
const extensionTarget = computed(() => defaultExtensionTarget());
const extVersion = computed(() => state.phpExtensionsVersion || extensionTarget.value);

function defaultExtensionTarget(): string {
  const installed = state.snapshot?.installed_php ?? [];
  const preferred = state.snapshot?.default_php ?? "";
  if (preferred && installed.includes(preferred)) return preferred;
  return installed[installed.length - 1] ?? "";
}

function chooseExtensionVersion(version: string): void {
  void loadPhpExtensions(version);
}

// Extensions are read from the local disk, so load them every time the PHP tab
// is opened, with no cached list. A deleted one must not stay selected.
watch(
  () => [state.settingsTab, installedPhp.value.join(","), defaultPhp.value] as const,
  ([tab]) => {
    if (tab !== "php" || state.busy) return;
    if (!state.phpExtensions && !state.phpExtensionsLoading) {
      const target = defaultExtensionTarget();
      if (target) void loadPhpExtensions(target);
    }
  },
);

function setTab(tab: SettingsSubTab) {
  state.settingsTab = tab;
}
</script>

<template>
  <div class="settings-page" aria-label="Halaman Pengaturan">
    <!-- Settings Sidebar Tabs -->
    <aside class="settings-sidebar">
      <div class="sidebar-header">
        <h2 class="sidebar-title">Pengaturan</h2>
      </div>

      <nav class="settings-nav">
        <button
          class="nav-item"
          :class="{ active: state.settingsTab === 'php' }"
          type="button"
          @click="setTab('php')"
        >
          <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
            <path d="M14 4.5V14a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2h5.5L14 4.5ZM13 5h-3a1 1 0 0 1-1-1V1H4a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5Z"/>
          </svg>
          <span>PHP & Runtime</span>
          <span v-if="installedPhp.length" class="nav-count">{{ installedPhp.length }}</span>
        </button>

        <button
          class="nav-item"
          :class="{ active: state.settingsTab === 'database' }"
          type="button"
          @click="setTab('database')"
        >
          <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 1c3.866 0 7 1.343 7 3v8c0 1.657-3.134 3-7 3s-7-1.343-7-3V4c0-1.657 3.134-3 7-3ZM2 6.435V8c0 1.258 2.532 2.308 6 2.477V8.912C4.532 8.743 2 7.693 2 6.435ZM14 8V6.435c0 1.258-2.532 2.308-6 2.477v1.565c3.468-.169 6-1.219 6-2.477ZM8 2.5C4.686 2.5 2 3.395 2 4.5S4.686 6.5 8 6.5s6-.895 6-2-2.686-2-6-2Zm6 8c0 1.258-2.532 2.308-6 2.477v1.565c3.468-.169 6-1.219 6-2.477V10.5ZM2 10.5V12c0 1.258 2.532 2.308 6 2.477V12.912C4.532 12.743 2 11.693 2 10.435Z"/>
          </svg>
          <span>Database (MariaDB)</span>
          <span
            v-if="database"
            class="status-mini-dot"
            :class="{ 'dot-ok': database.status === 'ready', 'dot-bad': database.status !== 'ready' }"
          ></span>
        </button>

        <button
          class="nav-item"
          :class="{ active: state.settingsTab === 'server' }"
          type="button"
          @click="setTab('server')"
        >
          <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
            <path d="M0 2.5A1.5 1.5 0 0 1 1.5 1h13A1.5 1.5 0 0 1 16 2.5v3A1.5 1.5 0 0 1 14.5 7h-13A1.5 1.5 0 0 1 0 5.5v-3Zm1.5-.5a.5.5 0 0 0-.5.5v3a.5.5 0 0 0 .5.5h13a.5.5 0 0 0 .5-.5v-3a.5.5 0 0 0-.5-.5h-13ZM0 10.5A1.5 1.5 0 0 1 1.5 9h13a1.5 1.5 0 0 1 1.5 1.5v3a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 0 13.5v-3Zm1.5-.5a.5.5 0 0 0-.5.5v3a.5.5 0 0 0 .5.5h13a.5.5 0 0 0 .5-.5v-3a.5.5 0 0 0-.5-.5h-13Z"/>
          </svg>
          <span>Web Server (Caddy)</span>
        </button>

        <button
          class="nav-item"
          :class="{ active: state.settingsTab === 'apache' }"
          type="button"
          @click="setTab('apache')"
        >
          <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0Zm5.9 7h-2.4a12.6 12.6 0 0 0-1-4.2A6.5 6.5 0 0 1 13.9 7ZM8 1.5c.7.9 1.3 2.6 1.4 5.5H6.6c.1-2.9.7-4.6 1.4-5.5ZM2.1 7a6.5 6.5 0 0 1 3.4-4.2 12.6 12.6 0 0 0-1 4.2H2.1Zm0 2h2.4c.1 1.6.4 3 1 4.2A6.5 6.5 0 0 1 2.1 9Zm4.5 0h2.8c-.1 2.9-.7 4.6-1.4 5.5-.7-.9-1.3-2.6-1.4-5.5Zm4.8 0h2.4a6.5 6.5 0 0 1-3.4 4.2c.6-1.2 1-2.6 1-4.2Z"/>
          </svg>
          <span>Apache</span>
        </button>

        <button
          class="nav-item"
          :class="{ active: state.settingsTab === 'system' }"
          type="button"
          @click="setTab('system')"
        >
          <svg class="nav-icon" viewBox="0 0 16 16" fill="currentColor">
            <path d="M9.405 1.05c-.413-1.4-2.397-1.4-2.81 0l-.1.34a1.464 1.464 0 0 1-2.105.872l-.31-.17c-1.283-.698-2.686.705-1.987 1.987l.169.311c.446.82.023 1.841-.872 2.105l-.34.1c-1.4.413-1.4 2.397 0 2.81l.34.1a1.464 1.464 0 0 1 .872 2.105l-.17.31c-.698 1.283.705 2.686 1.987 1.987l.311-.169a1.464 1.464 0 0 1 2.105.872l.1.34c.413 1.4 2.397 1.4 2.81 0l.1-.34a1.464 1.464 0 0 1 2.105-.872l.31.17c1.283.698 2.686-.705 1.987-1.987l-.169-.311a1.464 1.464 0 0 1 .872-2.105l.34-.1c1.4-.413 1.4-2.397 0-2.81l-.34-.1a1.464 1.464 0 0 1-.872-2.105l.17-.31c.698-1.283-.705-2.686-1.987-1.987l-.311.169a1.464 1.464 0 0 1-2.105-.872l-.1-.34ZM8 10.93a2.929 2.929 0 1 1 0-5.86 2.929 2.929 0 0 1 0 5.858Z"/>
          </svg>
          <span>Sistem & Integrasi</span>
        </button>
      </nav>
    </aside>

    <!-- Settings Main Panel -->
    <main class="settings-content">
      <!-- 1. TAB: PHP -->
      <section v-if="state.settingsTab === 'php'" class="tab-pane">
        <header class="pane-header">
          <div>
            <h2 class="pane-title">Versi PHP & FastCGI</h2>
            <p class="pane-desc">Kelola runtime PHP yang terpasang di komputer Anda dan pasang versi baru dari arsip resmi.</p>
          </div>
        </header>

        <!-- PHP Default & PATH Card -->
        <div class="card">
          <div class="card-head">
            <h3 class="card-title">Pengaturan PHP Global</h3>
          </div>
          <div class="card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">PHP Default</span>
                <span class="setting-desc">Versi PHP default yang digunakan saat membuat situs baru atau menjalankan Composer global.</span>
              </div>
              <div class="setting-ctrl">
                <select
                  class="select-input"
                  :value="defaultPhp"
                  :disabled="state.busy === 'default-php' || installedPhp.length === 0"
                  @change="(e) => setDefaultPhp((e.target as HTMLSelectElement).value)"
                >
                  <option v-for="ver in installedPhp" :key="ver" :value="ver">PHP {{ ver }}</option>
                  <option v-if="!installedPhp.length" value="" disabled>Belum ada PHP terpasang</option>
                </select>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Integrasi PATH Terminal Windows</span>
                <span class="setting-desc">
                  Mengarahkan perintah <code>php</code> di Command Prompt, PowerShell, dan terminal VS Code ke versi default DevKu.
                </span>
                <span v-if="phpPath?.registered" class="status-pill pill-ok">
                  Terdaftar: {{ phpPath.shim_version }}
                </span>
              </div>
              <div class="setting-ctrl">
                <button
                  v-if="!phpPath?.registered"
                  class="btn btn-outline"
                  type="button"
                  :disabled="state.busy === 'php-path' || !installedPhp.length"
                  @click="setPhpPath(true)"
                >
                  Hubungkan ke PATH
                </button>
                <button
                  v-else
                  class="btn btn-quiet"
                  type="button"
                  :disabled="state.busy === 'php-path'"
                  @click="setPhpPath(false)"
                >
                  Lepas dari PATH
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Installed PHP Versions Card -->
        <div class="card">
          <div class="card-head">
            <h3 class="card-title">Versi Terpasang ({{ installedPhp.length }})</h3>
          </div>
          <div class="card-body">
            <div v-if="installedPhp.length === 0" class="empty-box-inline">
              <p>Belum ada versi PHP yang terpasang di DevKu. Pasang versi di bawah ini.</p>
            </div>

            <div v-else class="version-list">
              <div v-for="ver in installedPhp" :key="ver" class="version-row">
                <div class="version-left">
                  <span class="version-tag">PHP {{ ver }}</span>
                  <span v-if="ver === defaultPhp" class="badge-default">Default</span>
                  <span v-if="phpPath?.shim_version === ver && phpPath.registered" class="badge-path">PATH</span>
                </div>

                <div class="version-actions">
                  <button class="btn btn-sm btn-quiet" type="button" @click="openPhpIni(ver)">
                    php.ini
                  </button>
                  <button
                    class="btn btn-sm btn-quiet"
                    type="button"
                    :disabled="state.busy === 'restart-php'"
                    @click="restartPhp(ver)"
                  >
                    Restart Pool
                  </button>
                  <button
                    class="btn btn-sm btn-danger-text"
                    type="button"
                    :disabled="state.busy === 'remove-php'"
                    @click="askRemovePhp(ver)"
                  >
                    Hapus
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- PHP Extensions Card -->
        <div class="card">
          <div class="card-head">
            <div>
              <h3 class="card-title">Ekstensi PHP</h3>
              <p class="card-subtitle">Hidup/matikan ekstensi di php.ini versi terpilih.</p>
            </div>
            <select
              v-if="installedPhp.length"
              class="select-input"
              :value="extVersion"
              aria-label="Versi PHP untuk daftar ekstensi"
              @change="chooseExtensionVersion(($event.target as HTMLSelectElement).value)"
            >
              <option v-for="ver in installedPhp" :key="ver" :value="ver">PHP {{ ver }}</option>
            </select>
          </div>
          <div class="card-body">
            <div v-if="!installedPhp.length" class="empty-box-inline">
              <p>Belum ada PHP terpasang, jadi tidak ada ekstensi untuk dikelola. Pasang versi di katalog di bawah.</p>
            </div>
            <div v-else-if="state.phpExtensionsLoading" class="empty-box-inline">
              <span class="spinner"></span>
              <p>Membaca daftar ekstensi PHP {{ extVersion }}...</p>
            </div>
            <div v-else-if="state.phpExtensionsError" class="empty-box-inline">
              <p class="text-bad">{{ state.phpExtensionsError }}</p>
              <button class="btn btn-sm btn-quiet" type="button" @click="loadPhpExtensions(extVersion)">
                Coba lagi
              </button>
            </div>
            <div v-else-if="!state.phpExtensions?.entries.length" class="empty-box-inline">
              <p>Folder ext PHP {{ extVersion }} tidak berisi dll ekstensi.</p>
            </div>
            <template v-else>
              <div class="ext-list">
                <div v-for="entry in state.phpExtensions.entries" :key="entry.name" class="ext-row">
                  <div class="ext-info">
                    <code class="ext-name">{{ entry.name }}</code>
                    <span class="ext-meta">
                      <span v-if="entry.zend" class="ext-kind">zend</span>
                      <span v-if="!entry.available" class="ext-missing">dll tidak ada</span>
                    </span>
                  </div>
                  <button
                    class="btn-toggle"
                    :class="{ active: entry.enabled }"
                    type="button"
                    :aria-pressed="entry.enabled"
                    :disabled="state.busy === 'php-ext' || (!entry.available && !entry.enabled)"
                    :aria-label="entry.name"
                    @click="togglePhpExtension(entry.name, !entry.enabled)"
                  >
                    <span class="toggle-slider"></span>
                  </button>
                </div>
              </div>
              <div v-if="state.phpExtensions.running" class="ext-restart">
                <span>Pool PHP {{ state.phpExtensions.version }} sedang hidup. Nyalakan ulang pool agar perubahan ekstensi berlaku.</span>
                <button
                  class="btn btn-sm btn-quiet"
                  type="button"
                  :disabled="state.busy === 'restart-php' || state.busy === 'php-ext'"
                  @click="restartPhpForExtensions"
                >
                  Restart Pool
                </button>
              </div>
            </template>
          </div>
        </div>

        <!-- Available Versions Catalog -->
        <div class="card">
          <div class="card-head">
            <div>
              <h3 class="card-title">Katalog Versi Resmi PHP</h3>
              <p class="card-subtitle">Unduh dan pasang versi PHP Windows x64 Non-Thread-Safe (NTS) resmi.</p>
            </div>
            <button
              class="btn btn-sm btn-quiet"
              type="button"
              :disabled="state.phpCatalogLoading"
              @click="loadPhpCatalog"
            >
              Segarkan Katalog
            </button>
          </div>
          <div class="card-body">
            <div v-if="state.phpCatalogLoading" class="empty-box-inline">
              <span class="spinner"></span>
              <p>Memeriksa ketersediaan versi...</p>
            </div>
            <div v-else-if="state.phpCatalogError" class="empty-box-inline text-bad">
              <p>{{ state.phpCatalogError }}</p>
            </div>
            <div v-else class="catalog-grid">
              <div
                v-for="entry in state.phpCatalog"
                :key="entry.version"
                class="catalog-card"
                :class="{ 'card-installed': entry.installed }"
              >
                <div class="cat-left">
                  <span class="cat-version">PHP {{ entry.version }}</span>
                  <span class="cat-meta">{{ entry.size }} · {{ entry.branch }}</span>
                </div>
                <div class="cat-action">
                  <span v-if="entry.installed" class="badge-installed">Terpasang</span>
                  <button
                    v-else
                    class="btn btn-sm btn-primary"
                    type="button"
                    :disabled="Boolean(state.download)"
                    @click="installPhp(entry.version)"
                  >
                    Pasang
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- 2. TAB: DATABASE (MARIADB) -->
      <section v-else-if="state.settingsTab === 'database'" class="tab-pane">
        <header class="pane-header">
          <div>
            <h2 class="pane-title">Database Lokal (MariaDB)</h2>
            <p class="pane-desc">Instance MariaDB bawaan DevKu untuk kebutuhan basis data lokal Anda.</p>
          </div>
        </header>

        <div v-if="!database?.installed" class="card empty-card">
          <h3>MariaDB Belum Terpasang</h3>
          <p>Pasang MariaDB untuk mengaktifkan database lokal otomatis tanpa konfigurasi rumit.</p>
          <button
            class="btn btn-primary"
            type="button"
            :disabled="state.busy === 'install-db'"
            @click="installDatabase"
          >
            {{ state.busy === 'install-db' ? 'Memasang MariaDB...' : 'Pasang MariaDB Sekarang' }}
          </button>
        </div>

        <div v-else class="card-stack">
          <!-- Status & Controls Card -->
          <div class="card">
            <div class="card-head">
              <div class="title-with-badge">
                <h3 class="card-title">{{ database.label }} {{ database.version }}</h3>
                <span
                  class="status-pill"
                  :class="{
                    'pill-ready': database.status === 'ready',
                    'pill-stopped': database.status === 'stopped',
                    'pill-failed': database.status === 'failed',
                  }"
                >
                  {{ database.status === 'ready' ? 'Berjalan' : 'Berhenti' }}
                </span>
              </div>

              <div class="card-actions">
                <button
                  v-if="database.status !== 'ready'"
                  class="btn btn-primary btn-sm"
                  type="button"
                  :disabled="state.busy === 'database'"
                  @click="databaseAction('start_database')"
                >
                  Nyalakan Database
                </button>
                <button
                  v-else
                  class="btn btn-danger-quiet btn-sm"
                  type="button"
                  :disabled="state.busy === 'database'"
                  @click="databaseAction('stop_database')"
                >
                  Hentikan
                </button>
              </div>
            </div>

            <div class="card-body">
              <div class="setting-row">
                <div class="setting-info">
                  <span class="setting-name">Port MySQL / MariaDB</span>
                  <span class="setting-desc">Port koneksi lokal (default 3306). Host: <code>127.0.0.1</code> atau <code>localhost</code></span>
                </div>
                <div class="setting-ctrl">
                  <span class="font-mono bold-text">{{ database.port }}</span>
                </div>
              </div>

              <div class="setting-row">
                <div class="setting-info">
                  <span class="setting-name">Autostart Database</span>
                  <span class="setting-desc">Nyalakan MariaDB secara otomatis setiap kali DevKu dibuka.</span>
                </div>
                <div class="setting-ctrl">
                  <button
                    class="btn-toggle"
                    :class="{ active: database.autostart }"
                    type="button"
                    @click="toggleDatabaseAutostart"
                  >
                    <span class="toggle-slider"></span>
                  </button>
                </div>
              </div>

              <div class="setting-row">
                <div class="setting-info">
                  <span class="setting-name">Password Root</span>
                  <span class="setting-desc">
                    Status: {{ database.has_password ? 'Password aktif' : 'Tanpa password (kosong)' }}
                  </span>
                </div>
                <div class="setting-ctrl">
                  <button class="btn btn-outline btn-sm" type="button" @click="resetDatabasePassword">
                    Ubah Password
                  </button>
                </div>
              </div>
            </div>
          </div>

          <!-- Folders & Backup Card -->
          <div class="card">
            <div class="card-head">
              <h3 class="card-title">Penyimpanan & Cadangan</h3>
            </div>
            <div class="card-body">
              <div class="setting-row">
                <div class="setting-info">
                  <span class="setting-name">Folder Data Database</span>
                  <span class="setting-desc font-mono">{{ database.data_path }}</span>
                </div>
                <div class="setting-ctrl">
                  <button class="btn btn-sm btn-quiet" type="button" @click="openDataFolder">
                    Buka di Explorer
                  </button>
                </div>
              </div>

              <div class="setting-row">
                <div class="setting-info">
                  <span class="setting-name">Folder Cadangan (Backups)</span>
                  <span class="setting-desc">Tempat file export/backup otomatis (.sql) disimpan.</span>
                </div>
                <div class="setting-ctrl">
                  <button class="btn btn-sm btn-quiet" type="button" @click="openBackups">
                    Buka Folder Cadangan
                  </button>
                </div>
              </div>
            </div>
            <div class="card-foot danger-foot">
              <span class="foot-warning">Hapus instalasi database dan seluruh binary MariaDB</span>
              <button class="btn btn-sm btn-danger" type="button" @click="askRemoveDatabase">
                Hapus MariaDB
              </button>
            </div>
          </div>
        </div>
      </section>

      <!-- 3. TAB: WEB SERVER (CADDY) -->
      <section v-else-if="state.settingsTab === 'server'" class="tab-pane">
        <header class="pane-header">
          <div>
            <h2 class="pane-title">Web Server & Domain Resolver (Caddy)</h2>
            <p class="pane-desc">Caddy melayani routing domain lokal otomatis dengan sertifikat TLS/HTTPS terpercaya.</p>
          </div>
        </header>

        <div class="card">
          <div class="card-head">
            <h3 class="card-title">Konfigurasi Jaringan & Domain</h3>
          </div>
          <div class="card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Domain TLD Lokal</span>
                <span class="setting-desc">Semua situs beralamat di <code>*.{{ state.snapshot?.tld ?? 'test' }}</code>, termasuk situs yang memakai backend Apache.</span>
              </div>
              <div class="setting-ctrl">
                <span class="badge-tld">.{{ state.snapshot?.tld ?? 'test' }}</span>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Port Server Web</span>
                <span class="setting-desc">Port standar HTTP dan HTTPS yang digunakan Caddy.</span>
              </div>
              <div class="setting-ctrl">
                <span class="font-mono">HTTP: 80 / HTTPS: 443</span>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Otoritas Sertifikat HTTPS (Root CA)</span>
                <span class="setting-desc">
                  Pasang sertifikat root lokal agar browser seperti Chrome, Edge, dan Firefox mengenali domain lokal sebagai HTTPS aman.
                </span>
                <span v-if="state.snapshot?.https_trusted" class="status-pill pill-ok">
                  Sertifikat Terpercaya
                </span>
                <span v-else class="status-pill pill-warn">
                  Belum Dipercayai
                </span>
              </div>
              <div class="setting-ctrl">
                <button
                  class="btn btn-outline"
                  type="button"
                  :disabled="state.busy === 'trust-caddy'"
                  @click="trustCaddy"
                >
                  Percayai Sertifikat Root
                </button>
              </div>
            </div>
          </div>
        </div>

      </section>

      <!-- 4. TAB: APACHE -->
      <section v-else-if="state.settingsTab === 'apache'" class="tab-pane">
        <header class="pane-header">
          <div>
            <h2 class="pane-title">Apache</h2>
            <p class="pane-desc">
              Apache berjalan sebagai backend internal di belakang Caddy. Akses situs tetap lewat
              domain di port 80/443, termasuk HTTPS; port Apache tidak pernah dibuka ke luar.
            </p>
          </div>
        </header>

        <div class="card">
          <div class="card-head">
            <div>
              <h3 class="card-title">Instalasi</h3>
              <p class="card-subtitle">
                Diambil dari Apache Lounge (build Win64), diverifikasi SHA-256, lalu dipasang di
                <code>engines/apache</code>. Tidak ada folder yang perlu dipilih.
              </p>
            </div>
          </div>
          <div class="card-body">
            <div v-if="!apache" class="setting-row">
              <div class="setting-info">
                <span class="setting-desc">Membaca status Apache...</span>
              </div>
            </div>

            <template v-else>
              <div class="setting-row">
                <div class="setting-info">
                  <span class="setting-name">Status</span>
                  <span v-if="apache.installed" class="setting-desc">
                    Apache {{ apache.version ?? 'terpasang' }} siap dipakai.
                  </span>
                  <span v-else class="setting-desc">
                    Belum terpasang. Unduhan sekitar 14 MB dan butuh koneksi internet.
                  </span>
                  <span class="status-pill" :class="apache.installed ? 'pill-ok' : 'pill-warn'">
                    {{ apache.installed ? 'Siap' : 'Belum terpasang' }}
                  </span>
                  <span v-if="apache.installed" class="setting-desc font-mono">
                    {{ apache.root }}
                  </span>
                  <span v-if="apache.status === 'failed'" class="setting-desc text-bad">
                    Apache gagal berjalan{{ apache.detail ? `: ${apache.detail}` : '.' }}
                  </span>
                </div>
                <div class="setting-ctrl">
                  <button
                    class="btn btn-sm btn-outline"
                    type="button"
                    :disabled="!!state.download"
                    @click="installBinary('apache')"
                  >
                    {{
                      installingApache
                        ? 'Memasang...'
                        : apache.installed
                          ? 'Pasang Ulang'
                          : 'Pasang Apache'
                    }}
                  </button>
                </div>
              </div>

              <div class="setting-row">
                <div class="setting-info">
                  <span class="setting-name">Port Internal Apache</span>
                  <span class="setting-desc">
                    httpd hanya mendengarkan <code>127.0.0.1:{{ apache.port }}</code>. Caddy
                    meneruskan situs Apache ke sini, jadi URL situs tetap
                    <code>http://domain</code> tanpa port. 80, 443, dan 2019 dipakai Caddy.
                  </span>
                </div>
                <div class="setting-ctrl flex-gap">
                  <input
                    v-model.number="apachePort"
                    class="input input-port"
                    type="number"
                    min="1024"
                    max="65535"
                    aria-label="Port Apache"
                    :disabled="state.busy === 'apache-port'"
                  />
                  <button
                    class="btn btn-sm btn-outline"
                    type="button"
                    :disabled="state.busy === 'apache-port' || apachePort === apache.port"
                    @click="saveApachePort"
                  >
                    Simpan
                  </button>
                </div>
              </div>

              <div class="setting-row">
                <div class="setting-info">
                  <span class="setting-name">Cakupan</span>
                  <span class="setting-desc">
                    httpd hanya menyala selama ada situs yang memakai Apache, dan mati lagi saat
                    situs terakhir pindah ke Caddy. Semua akses tetap lewat Caddy di 80/443. Build
                    Apache Lounge membutuhkan Visual C++ Redistributable 2015-2022; biasanya sudah
                    ada jika Laragon atau XAMPP pernah dipakai.
                  </span>
                </div>
              </div>
            </template>
          </div>
        </div>
      </section>

      <!-- 5. TAB: SISTEM & INTEGRASI -->
      <section v-else-if="state.settingsTab === 'system'" class="tab-pane">
        <header class="pane-header">
          <div>
            <h2 class="pane-title">Sistem, Toolchain & Integrasi</h2>
            <p class="pane-desc">Status binary eksternal, konfigurasi aplikasi, dan integrasi environment Windows.</p>
          </div>
        </header>

        <!-- Toolchain Detection -->
        <div class="card">
          <div class="card-head">
            <h3 class="card-title">Binary & Toolchain Pengembang</h3>
          </div>
          <div class="card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Bun Runtime (JS / TS)</span>
                <span class="setting-desc">Digunakan untuk eksekusi script TypeScript dan package manager super cepat.</span>
              </div>
              <div class="setting-ctrl">
                <button
                  v-if="!state.runtimesList?.some(r => r.name === 'bun' && r.version)"
                  class="btn btn-sm btn-outline"
                  type="button"
                  @click="installBinary('bun')"
                >
                  Pasang Bun
                </button>
                <span v-else class="badge-installed">Terdeteksi</span>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Composer (PHP Package Manager)</span>
                <span class="setting-desc">
                  Composer {{ state.snapshot?.composer_version ?? '' }}
                  <template v-if="state.snapshot?.composer_php">
                    (menggunakan PHP {{ state.snapshot.composer_php }})
                  </template>
                </span>
              </div>
              <div class="setting-ctrl">
                <button
                  v-if="!state.snapshot?.composer_installed"
                  class="btn btn-sm btn-outline"
                  type="button"
                  @click="installBinary('composer')"
                >
                  Pasang Composer
                </button>
                <span v-else class="badge-installed">Terpasang</span>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Composer Global Binaries PATH</span>
                <span class="setting-desc">Mengarahkan tool global (seperti laravel installer) ke PATH terminal.</span>
              </div>
              <div class="setting-ctrl">
                <button
                  v-if="!composerGlobal?.registered"
                  class="btn btn-sm btn-outline"
                  type="button"
                  :disabled="state.busy === 'composer-global'"
                  @click="setComposerGlobalPath(true)"
                >
                  Hubungkan PATH
                </button>
                <button
                  v-else
                  class="btn btn-sm btn-quiet"
                  type="button"
                  :disabled="state.busy === 'composer-global'"
                  @click="setComposerGlobalPath(false)"
                >
                  Lepaskan PATH
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Storage & Workspace -->
        <div class="card">
          <div class="card-head">
            <h3 class="card-title">Penyimpanan & Workspace</h3>
          </div>
          <div class="card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Folder Data</span>
                <span class="setting-desc">
                  <code class="path-value">{{ state.snapshot?.home_path }}</code>
                </span>
                <span class="setting-desc">
                  <template v-if="dataLocation?.source === 'environment'">
                    Dikunci variabel <code>DEVKU_HOME</code>, jadi tidak bisa diubah dari sini.
                  </template>
                  <template v-else-if="dataLocation?.source === 'custom'">
                    Dipindahkan lewat halaman ini. Penunjuk tersimpan di
                    <code>{{ dataLocation.pointer }}</code>.
                  </template>
                  <template v-else-if="dataLocation?.source === 'invalid'">
                    Berkas penunjuk tidak terbaca, sementara memakai folder bawaan
                    <code>{{ dataLocation.default_home }}</code>.
                  </template>
                  <template v-else> Lokasi bawaan aplikasi, tidak ada penunjuk kustom. </template>
                </span>
              </div>
              <div class="setting-ctrl flex-gap">
                <button class="btn btn-sm btn-outline" type="button" @click="openDataRoot()">
                  Buka
                </button>
                <button
                  class="btn btn-sm btn-outline"
                  type="button"
                  :disabled="dataLocation?.locked || state.busy === 'move-data'"
                  @click="pickDataFolder()"
                >
                  {{ state.busy === 'move-data' ? 'Memindahkan...' : 'Ubah Lokasi' }}
                </button>
              </div>
            </div>

            <div
              v-if="state.moveProgress"
              class="move-progress"
              role="progressbar"
              aria-label="Kemajuan pemindahan folder data"
              :aria-valuenow="movePercent"
              aria-valuemin="0"
              aria-valuemax="100"
            >
              <div class="move-progress-track">
                <div class="move-progress-fill" :style="{ width: `${movePercent}%` }"></div>
              </div>
              <span class="move-progress-label">
                {{ movePercent }}% ·
                {{ formatBytes(state.moveProgress.done) }} / {{ formatBytes(state.moveProgress.total) }}
              </span>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Folder Workspace</span>
                <span class="setting-desc">
                  <code class="path-value">{{ workspace?.path }}</code>
                </span>
                <span class="setting-desc">
                  <template v-if="!workspace?.exists">
                    Folder belum ada dan dibuat otomatis saat dipakai.
                  </template>
                  <template v-else>
                    Folder induk project. Wizard memakai folder ini sebagai titik awal, dan project di
                    dalamnya tetap didaftarkan sendiri.
                  </template>
                </span>
              </div>
              <div class="setting-ctrl flex-gap">
                <button class="btn btn-sm btn-outline" type="button" @click="openWorkspace()">
                  Buka
                </button>
                <button
                  class="btn btn-sm btn-outline"
                  type="button"
                  :disabled="state.busy === 'workspace'"
                  @click="pickWorkspace()"
                >
                  {{ workspace?.custom ? 'Ganti' : 'Pilih' }}
                </button>
                <button
                  v-if="workspace?.custom"
                  class="btn btn-sm btn-quiet"
                  type="button"
                  :disabled="state.busy === 'workspace'"
                  @click="resetWorkspace()"
                >
                  Bawaan
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- DevKu App Configuration & Backups -->
        <div class="card">
          <div class="card-head">
            <h3 class="card-title">Pengaturan DevKu</h3>
          </div>
          <div class="card-body">
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Jalankan Otomatis Saat Windows Login</span>
                <span class="setting-desc">DevKu akan berjalan di latar belakang (tray) saat komputer dinyalakan.</span>
              </div>
              <div class="setting-ctrl">
                <button
                  class="btn-toggle"
                  :class="{ active: state.snapshot?.start_at_login }"
                  type="button"
                  @click="setStartAtLogin(!state.snapshot?.start_at_login)"
                >
                  <span class="toggle-slider"></span>
                </button>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Cadangkan & Pulihkan Pengaturan</span>
                <span class="setting-desc">Ekspor atau impor daftar situs dan konfigurasi ke berkas JSON.</span>
              </div>
              <div class="setting-ctrl flex-gap">
                <button class="btn btn-sm btn-quiet" type="button" @click="exportConfig">
                  Ekspor JSON
                </button>
                <button class="btn btn-sm btn-quiet" type="button" @click="importConfig">
                  Impor JSON
                </button>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-name">Cache Berkas Unduhan</span>
                <span class="setting-desc">
                  {{ state.snapshot?.downloads.files ?? 0 }} berkas ({{ formatBytes(state.snapshot?.downloads.bytes ?? 0) }})
                </span>
              </div>
              <div class="setting-ctrl">
                <button
                  class="btn btn-sm btn-quiet"
                  type="button"
                  :disabled="!state.snapshot?.downloads.files || state.busy === 'clear-downloads'"
                  @click="clearDownloads"
                >
                  Bersihkan Cache
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>
    </main>
  </div>
</template>

<style scoped>
.settings-page {
  flex: 1;
  display: flex;
  background: var(--bg);
  height: 100%;
  overflow: hidden;
}

.settings-sidebar {
  width: 240px;
  flex: 0 0 240px;
  background: var(--surface);
  border-right: 1px solid var(--line);
  display: flex;
  flex-direction: column;
}

.sidebar-header {
  padding: 16px 18px 12px;
  border-bottom: 1px solid var(--line);
}

.sidebar-title {
  margin: 0;
  font-size: 14px;
  font-weight: 650;
  color: var(--fg);
}

.settings-nav {
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: var(--muted);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s, color 0.1s;
}

.nav-item:hover {
  background: var(--bg);
  color: var(--fg);
}

.nav-item.active {
  background: var(--bg);
  color: var(--fg);
  font-weight: 600;
}

.nav-icon {
  width: 14px;
  height: 14px;
  opacity: 0.75;
}

.nav-count {
  margin-left: auto;
  font-size: 10px;
  background: var(--line);
  padding: 1px 6px;
  border-radius: 8px;
  color: var(--muted);
}

.status-mini-dot {
  margin-left: auto;
  width: 6px;
  height: 6px;
  border-radius: 50%;
}

.dot-ok { background: var(--ok); }
.dot-bad { background: var(--idle); }

.settings-content {
  flex: 1;
  overflow-y: auto;
  padding: 24px 32px;
}

.tab-pane {
  display: flex;
  flex-direction: column;
  gap: 20px;
  max-width: 800px;
}

.pane-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
}

.pane-title {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  color: var(--fg);
}

.pane-desc {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--muted);
}

.card-stack {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.card {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  overflow: hidden;
}

.card-head {
  padding: 12px 18px;
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.card-title {
  margin: 0;
  font-size: 13px;
  font-weight: 650;
  color: var(--fg);
}

.card-subtitle {
  margin: 2px 0 0;
  font-size: 11px;
  color: var(--muted);
}

.card-body {
  padding: 8px 18px;
  display: flex;
  flex-direction: column;
}

.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 0;
  border-bottom: 1px solid var(--line);
  gap: 20px;
}

.setting-row:last-child {
  border-bottom: none;
}

.setting-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.setting-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--fg);
}

.setting-desc {
  font-size: 11px;
  color: var(--muted);
  line-height: 1.4;
  word-break: break-word;
}

.path-value {
  font-family: var(--font-mono);
  word-break: break-all;
}

.move-progress {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 4px 0 12px;
}

.move-progress-track {
  height: 6px;
  border-radius: 999px;
  background: var(--line);
  overflow: hidden;
}

.move-progress-fill {
  height: 100%;
  background: var(--accent);
  transition: width 120ms linear;
}

.move-progress-label {
  font-size: 11px;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}

.setting-ctrl {
  flex-shrink: 0;
}

.flex-gap {
  display: flex;
  gap: 8px;
}

.input-port {
  width: 96px;
  text-align: right;
}

.select-input {
  height: 30px;
  padding: 0 10px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--fg);
  font-size: 12px;
}

.status-pill {
  display: inline-block;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 600;
  margin-top: 4px;
  width: fit-content;
}

.pill-ok {
  background: rgba(74, 222, 128, 0.15);
  color: var(--ok);
}

.pill-warn {
  background: rgba(245, 158, 11, 0.15);
  color: var(--accent);
}

.pill-stopped {
  background: var(--line);
  color: var(--muted);
}

.pill-ready {
  background: rgba(74, 222, 128, 0.15);
  color: var(--ok);
}

.pill-failed {
  background: rgba(248, 113, 113, 0.15);
  color: var(--bad);
}

.version-list {
  display: flex;
  flex-direction: column;
}

.version-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 0;
  border-bottom: 1px solid var(--line);
}

.version-row:last-child {
  border-bottom: none;
}

.version-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.version-tag {
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
  color: var(--fg);
}

.badge-default {
  font-size: 10px;
  padding: 1px 5px;
  background: var(--accent);
  color: var(--on-accent);
  border-radius: 3px;
  font-weight: 600;
}

.badge-path {
  font-size: 10px;
  padding: 1px 5px;
  background: var(--line);
  color: var(--fg);
  border-radius: 3px;
  font-family: var(--font-mono);
}

.version-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.catalog-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 10px;
  padding: 10px 0;
}

.catalog-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
}

.cat-left {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.cat-version {
  font-size: 12px;
  font-weight: 600;
  font-family: var(--font-mono);
  color: var(--fg);
}

.cat-meta {
  font-size: 10px;
  color: var(--muted);
}

.badge-installed {
  font-size: 10px;
  color: var(--ok);
  font-weight: 600;
}

.badge-tld {
  font-family: var(--font-mono);
  font-size: 13px;
  font-weight: 600;
  padding: 2px 8px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
}

.empty-card {
  padding: 32px;
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

.empty-box-inline {
  padding: 20px;
  text-align: center;
  font-size: 12px;
  color: var(--muted);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}

.card-foot {
  padding: 10px 18px;
  background: var(--bg);
  border-top: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.foot-warning {
  font-size: 11px;
  color: var(--muted);
}

.bold-text {
  font-weight: 600;
}

.font-mono {
  font-family: var(--font-mono);
}

.text-bad {
  color: var(--bad);
}

.btn {
  height: 30px;
  padding: 0 12px;
  font-size: 12px;
  font-weight: 500;
  border-radius: 4px;
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--fg);
  cursor: pointer;
}

.btn-primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--on-accent);
}

.btn-outline {
  border-color: var(--accent);
  color: var(--accent);
  background: transparent;
}

.btn-outline:hover {
  background: rgba(245, 158, 11, 0.1);
}

.btn-quiet {
  background: transparent;
  border-color: var(--line);
  color: var(--fg);
}

.btn-sm {
  height: 24px;
  padding: 0 8px;
  font-size: 11px;
}

.btn-danger {
  background: var(--bad);
  border-color: var(--bad);
  color: #ffffff;
}

.btn-danger-text {
  background: none;
  border-color: transparent;
  color: var(--bad);
}

.btn-danger-text:hover {
  background: rgba(185, 28, 28, 0.1);
}

.btn-danger-quiet {
  background: none;
  border: 1px solid var(--bad);
  color: var(--bad);
}

.btn-toggle {
  width: 28px;
  height: 16px;
  background: var(--line);
  border-radius: 8px;
  border: none;
  cursor: pointer;
  position: relative;
  padding: 0;
  transition: background 0.15s;
}

.btn-toggle.active {
  background: var(--ok);
}

.toggle-slider {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: #ffffff;
  transition: transform 0.15s;
}

.btn-toggle.active .toggle-slider {
  transform: translateX(12px);
}

/* Extension list: one row per dll, scrolls when it grows long. */
.ext-list {
  max-height: 340px;
  overflow-y: auto;
  border: 1px solid var(--line);
  border-radius: 8px;
}

.ext-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 7px 12px;
}

.ext-row + .ext-row {
  border-top: 1px solid var(--line);
}

.ext-info {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.ext-name {
  font-size: 0.8125rem;
}

.ext-meta {
  display: inline-flex;
  gap: 8px;
}

.ext-kind {
  color: var(--muted);
  font-size: 0.6875rem;
}

.ext-missing {
  color: var(--bad);
  font-size: 0.6875rem;
}

.ext-restart {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 10px;
  color: var(--muted);
}
</style>
