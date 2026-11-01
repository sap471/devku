<script setup lang="ts">
import { computed, onMounted } from "vue";
import {
  state,
  setDefaultPhp,
  setPhpPath,
  openPhpIni,
  restartPhp,
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
  trustCaddy,
  setComposerGlobalPath,
  installBinary,
  setStartAtLogin,
  clearDownloads,
  exportConfig,
  importConfig,
  type SettingsSubTab,
} from "../../store/devku";
import { formatBytes } from "../../log-format";

onMounted(() => {
  if (!state.phpCatalog) {
    void loadPhpCatalog();
  }
});

const installedPhp = computed(() => state.snapshot?.installed_php ?? []);
const defaultPhp = computed(() => state.snapshot?.default_php ?? "");
const phpPath = computed(() => state.snapshot?.php_path);
const database = computed(() => state.snapshot?.database);
const composerGlobal = computed(() => state.snapshot?.composer_global);

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
                <span class="setting-desc">Semua situs lokal akan beralamat di <code>*.{{ state.snapshot?.tld ?? 'test' }}</code></span>
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

      <!-- 4. TAB: SISTEM & INTEGRASI -->
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
}

.setting-ctrl {
  flex-shrink: 0;
}

.flex-gap {
  display: flex;
  gap: 8px;
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
</style>
