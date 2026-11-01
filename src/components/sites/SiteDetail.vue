<script setup lang="ts">
import { computed, ref, watch, nextTick, onUnmounted } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import {
  state,
  currentSite,
  STATUS_WORD,
  frameworkLabel,
  setSitePhp,
  setSiteBackend,
  askRemoveSite,
  toggleOpenMenu,
  runOpenIn,
  loadLog,
  loadCommands,
  type SiteSubTab,
} from "../../store/devku";
import * as terminalPanel from "../../terminal";
import {
  parseAccessLog,
  formatDuration,
  formatBytes,
  statusTone,
  classifyRawLine,
  type AccessLog,
  type RawLogLevel,
} from "../../log-format";

const site = computed(() => currentSite.value);
const installedPhp = computed(() => state.snapshot?.installed_php ?? []);
const apacheReady = computed(() => state.snapshot?.apache.installed ?? false);
const apachePort = computed(() => state.snapshot?.apache.port ?? 8080);

// Log viewer reactive states
type LogFilterMode = "all" | "errors" | "http" | "raw";
const logFilter = ref<LogFilterMode>("all");
const logSearch = ref("");
const autoScroll = ref(true);
const logsViewport = ref<HTMLElement | null>(null);
const copySuccess = ref(false);

watch(
  () => state.selectedSiteId,
  (newId) => {
    if (newId) {
      void loadLog(newId);
      void loadCommands(newId);
    }
  },
  { immediate: true },
);

interface ParsedLogLine {
  id: number;
  raw: string;
  isAccess: boolean;
  access?: AccessLog;
  rawLevel?: RawLogLevel;
}

const parsedLogs = computed<ParsedLogLine[]>(() => {
  return state.logLines.map((line, idx) => {
    const access = parseAccessLog(line);
    if (access) {
      return { id: idx, raw: line, isAccess: true, access };
    }
    return {
      id: idx,
      raw: line,
      isAccess: false,
      rawLevel: classifyRawLine(line),
    };
  });
});

const filteredLogs = computed(() => {
  let list = parsedLogs.value;
  const q = logSearch.value.trim().toLowerCase();

  if (logFilter.value === "errors") {
    list = list.filter((item) => {
      if (item.isAccess && item.access) {
        return item.access.status >= 400;
      }
      return item.rawLevel === "error";
    });
  } else if (logFilter.value === "http") {
    list = list.filter((item) => item.isAccess);
  } else if (logFilter.value === "raw") {
    list = list.filter((item) => !item.isAccess);
  }

  if (q) {
    list = list.filter((item) => {
      if (item.isAccess && item.access) {
        return (
          item.access.uri.toLowerCase().includes(q) ||
          item.access.method.toLowerCase().includes(q) ||
          String(item.access.status).includes(q)
        );
      }
      return item.raw.toLowerCase().includes(q);
    });
  }

  return list;
});

function isDateChanged(idx: number): boolean {
  if (idx === 0) return true;
  const current = filteredLogs.value[idx];
  const prev = filteredLogs.value[idx - 1];
  if (!current.isAccess || !current.access) return false;
  if (!prev.isAccess || !prev.access) return true;
  return current.access.date !== prev.access.date;
}

watch(
  () => state.logLines.length,
  async () => {
    if (autoScroll.value && state.siteSubTab === "logs") {
      await nextTick();
      if (logsViewport.value) {
        logsViewport.value.scrollTop = logsViewport.value.scrollHeight;
      }
    }
  },
);

function setSubTab(tab: SiteSubTab) {
  state.siteSubTab = tab;
  if (tab === "logs" && autoScroll.value) {
    nextTick(() => {
      if (logsViewport.value) {
        logsViewport.value.scrollTop = logsViewport.value.scrollHeight;
      }
    });
  }
}

function openInBrowser() {
  if (!site.value) return;
  void openUrl(site.value.url);
}

async function onPhpChange(event: Event) {
  const select = event.target as HTMLSelectElement;
  if (!site.value || !select.value) return;
  await setSitePhp(site.value.id, select.value);
}

async function onBackendChange(event: Event) {
  const select = event.target as HTMLSelectElement;
  if (!site.value || !select.value) return;
  const backend = select.value === "apache" ? "apache" : "caddy";
  if (backend === site.value.backend) return;

  await setSiteBackend(site.value.id, backend);
  // Penolakan backend (misalnya Apache belum dipasang) tidak mengubah
  // snapshot, jadi select dikembalikan ke nilai yang benar-benar tersimpan.
  if (site.value.backend !== backend) {
    select.value = site.value.backend;
  }
}

async function runCommand(command: string) {
  if (!site.value) return;
  state.terminalOpen = true;
  await terminalPanel.run(site.value.root, command);
}

async function runTsFile() {
  if (!site.value) return;
  const file = await open({
    title: "Pilih berkas TypeScript",
    multiple: false,
    filters: [{ name: "TypeScript", extensions: ["ts", "tsx", "mts"] }],
  });
  if (typeof file !== "string") return;

  state.terminalOpen = true;
  await terminalPanel.run(site.value.root, `bun "${file}"`);
}

function refreshCurrentLog() {
  if (site.value) {
    void loadLog(site.value.id);
  }
}

// Live: polling tail_log tiap 2 detik selama toggle hidup dan sub-tab Log
// terbuka. Pindah situs ikut saja karena polling membaca situs aktif terkini.
const liveLog = ref(false);
let liveTimer: number | null = null;
let liveBusy = false;

async function liveTick(): Promise<void> {
  if (liveBusy || !site.value) return;
  liveBusy = true;
  try {
    await loadLog(site.value.id);
  } finally {
    liveBusy = false;
  }
}

function startLive(): void {
  if (liveTimer !== null) return;
  void liveTick();
  liveTimer = window.setInterval(() => void liveTick(), 2000);
}

function stopLive(): void {
  if (liveTimer === null) return;
  window.clearInterval(liveTimer);
  liveTimer = null;
}

function toggleLive(): void {
  liveLog.value = !liveLog.value;
  if (liveLog.value) startLive();
  else stopLive();
}

watch(
  () => [state.siteSubTab, liveLog.value] as const,
  ([tab]) => {
    if (liveLog.value && tab === "logs" && site.value) startLive();
    else stopLive();
  },
  { immediate: true },
);

onUnmounted(stopLive);

async function copyVisibleLogs() {
  const text = filteredLogs.value.map((l) => l.raw).join("\n");
  try {
    await navigator.clipboard.writeText(text);
    copySuccess.value = true;
    setTimeout(() => {
      copySuccess.value = false;
    }, 1500);
  } catch (err) {
    console.error("Gagal menyalin log:", err);
  }
}
</script>

<template>
  <main class="site-detail-pane" aria-label="Detail Situs">
    <!-- Unselected State -->
    <div v-if="!site" class="detail-empty-state">
      <div class="empty-box">
        <svg class="empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" y1="8" x2="12" y2="12"/>
          <line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        <p class="empty-text">Pilih salah satu situs di sebelah kiri untuk melihat informasi domain, port, perintah cepat, dan log.</p>
      </div>
    </div>

    <!-- Selected Site View -->
    <div v-else class="detail-content">
      <!-- Site Detail Header -->
      <header class="detail-head">
        <div class="head-left">
          <div class="title-row">
            <h2 class="site-title">{{ site.name }}</h2>
            <span
              class="status-pill"
              :class="{
                'pill-ready': site.enabled && site.status === 'ready',
                'pill-starting': site.enabled && (site.status === 'starting' || site.status === 'backoff'),
                'pill-failed': site.enabled && site.status === 'failed',
                'pill-stopped': !site.enabled || site.status === 'stopped',
              }"
            >
              {{ STATUS_WORD[site.status] }}
            </span>
          </div>

          <div class="domain-row">
            <button class="domain-link" type="button" @click="openInBrowser">
              {{ site.domain }}
              <svg class="link-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M4.75 3.5a.75.75 0 0 0 0 1.5h4.69L3.22 11.22a.75.75 0 1 0 1.06 1.06L10.5 6.06v4.69a.75.75 0 0 0 1.5 0v-6.5a.75.75 0 0 0-.75-.75h-6.5Z"/>
              </svg>
            </button>
            <span v-if="site.port" class="port-badge">Port: {{ site.port }}</span>
          </div>
        </div>

        <div class="head-actions">
          <!-- Open in... Dropdown Menu -->
          <div class="menu-wrap">
            <button
              class="btn btn-menu"
              type="button"
              @click="toggleOpenMenu(site.id)"
            >
              Buka di...
              <svg class="icon-caret" viewBox="0 0 16 16" fill="currentColor">
                <path d="M4.427 6.427l3.396 3.396a.25.25 0 0 0 .354 0l3.396-3.396A.25.25 0 0 0 11.396 6H4.604a.25.25 0 0 0-.177.427Z"/>
              </svg>
            </button>

            <!-- Dropdown popover -->
            <div v-if="state.openMenuFor === site.id" class="dropdown-popover" @click.stop>
              <div v-if="!state.openMenuItems" class="popover-loading">Memuat aplikasi...</div>
              <div v-else-if="state.openMenuItems.length === 0" class="popover-empty">Tidak ada editor terdeteksi</div>
              <template v-else>
                <button
                  v-for="target in state.openMenuItems"
                  :key="target.id"
                  class="dropdown-item"
                  :disabled="!target.available"
                  type="button"
                  @click="runOpenIn(target.id)"
                >
                  {{ target.label }}
                  <span v-if="!target.available" class="item-unavailable">(tidak terpasang)</span>
                </button>
              </template>
            </div>
          </div>

          <!-- Open in Browser Button -->
          <button
            class="btn btn-primary"
            type="button"
            :disabled="!site.enabled || site.status !== 'ready'"
            @click="openInBrowser"
          >
            Buka Situs
          </button>

          <!-- Delete Site Button -->
          <button
            class="btn btn-danger-quiet"
            type="button"
            title="Hapus situs dari daftar"
            @click="askRemoveSite(site.id)"
          >
            <svg class="icon-trash" viewBox="0 0 16 16" fill="currentColor">
              <path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5Zm2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5Zm3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0 1 0V6Z"/>
              <path d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1v1ZM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4H4.118ZM2.5 3h11V2h-11v1Z"/>
            </svg>
          </button>
        </div>
      </header>

      <!-- Sub-tabs Navigation -->
      <nav class="subtabs-bar">
        <button
          class="subtab-btn"
          :class="{ active: state.siteSubTab === 'general' }"
          type="button"
          @click="setSubTab('general')"
        >
          Umum
        </button>
        <button
          class="subtab-btn"
          :class="{ active: state.siteSubTab === 'logs' }"
          type="button"
          @click="setSubTab('logs')"
        >
          Log
          <span v-if="state.logLines.length" class="subtab-pill">{{ state.logLines.length }}</span>
        </button>
        <button
          class="subtab-btn"
          :class="{ active: state.siteSubTab === 'commands' }"
          type="button"
          @click="setSubTab('commands')"
        >
          Perintah
        </button>
      </nav>

      <!-- Sub-tab 1: GENERAL -->
      <section v-if="state.siteSubTab === 'general'" class="subtab-panel panel-general">
        <div class="card-section">
          <h3 class="section-title">Konfigurasi Runtime</h3>

          <div class="field-row">
            <label for="select-site-php" class="field-label">Versi PHP</label>
            <div class="field-control">
              <select
                id="select-site-php"
                class="input select-php"
                :value="site.php"
                :disabled="state.busy === 'set-php'"
                @change="onPhpChange"
              >
                <option v-for="ver in installedPhp" :key="ver" :value="ver">
                  PHP {{ ver }} {{ ver === state.snapshot?.default_php ? '(Default)' : '' }}
                </option>
                <option v-if="!installedPhp.includes(site.php)" :value="site.php" disabled>
                  PHP {{ site.php }} (Tidak terpasang)
                </option>
              </select>
              <p class="field-hint">
                Pool PHP akan otomatis dialihkan dan dimulai ulang saat versi diubah.
              </p>
            </div>
          </div>

          <div class="field-row">
            <label for="select-site-backend" class="field-label">Web Server</label>
            <div class="field-control">
              <select
                id="select-site-backend"
                class="input select-php"
                :value="site.backend"
                :disabled="state.busy === 'set-backend'"
                @change="onBackendChange"
              >
                <option value="caddy">Caddy</option>
                <option value="apache" :disabled="!apacheReady">
                  Apache{{ apacheReady ? ' (via Caddy, HTTPS)' : ' (belum dipasang)' }}
                </option>
              </select>
              <p class="field-hint">
                <template v-if="site.backend === 'apache'">
                  Apache melayani situs ini dari balik Caddy. Akses tetap di
                  <code>https://{{ site.domain }}</code> tanpa port; httpd hanya hidup selama ada
                  situs yang memakainya.
                </template>
                <template v-else-if="apacheReady">
                  Caddy melayani situs ini langsung di 80/443. Apache hanya dipakai situs yang
                  memilihnya, lewat port internal {{ apachePort }}.
                </template>
                <template v-else>
                  Apache belum dipasang. Pasang di Pengaturan, Apache, lalu pilih di sini.
                </template>
              </p>
            </div>
          </div>
        </div>

        <div class="card-section">
          <h3 class="section-title">Informasi Folder & Lokasi</h3>

          <div class="info-grid">
            <div class="info-item">
              <span class="info-label">Framework</span>
              <span class="info-value">{{ frameworkLabel(site.framework) }}</span>
            </div>
            <div class="info-item">
              <span class="info-label">Web Server</span>
              <span class="info-value">
                {{ site.backend === 'apache' ? 'Apache di belakang Caddy' : 'Caddy' }}
              </span>
            </div>
            <div class="info-item">
              <span class="info-label">Port FastCGI</span>
              <span class="info-value font-mono">{{ site.port ? `127.0.0.1:${site.port}` : 'Belum aktif' }}</span>
            </div>
            <div class="info-item full">
              <span class="info-label">Document Root (Web Root)</span>
              <span class="info-value font-mono">{{ site.root }}</span>
            </div>
            <div class="info-item full">
              <span class="info-label">Folder Project</span>
              <span class="info-value font-mono">{{ site.path }}</span>
            </div>
          </div>
        </div>
      </section>

      <!-- Sub-tab 2: POLISHED DEVELOPER LOG VIEWER -->
      <section v-else-if="state.siteSubTab === 'logs'" class="subtab-panel panel-logs">
        <!-- Logs Controls Toolbar -->
        <div class="logs-toolbar">
          <div class="toolbar-left">
            <!-- Filter Segmented Tabs -->
            <div class="log-segmented">
              <button
                class="seg-btn"
                :class="{ active: logFilter === 'all' }"
                type="button"
                @click="logFilter = 'all'"
              >
                Semua
              </button>
              <button
                class="seg-btn"
                :class="{ active: logFilter === 'errors' }"
                type="button"
                @click="logFilter = 'errors'"
              >
                Galat Saja
              </button>
              <button
                class="seg-btn"
                :class="{ active: logFilter === 'http' }"
                type="button"
                @click="logFilter = 'http'"
              >
                HTTP
              </button>
              <button
                class="seg-btn"
                :class="{ active: logFilter === 'raw' }"
                type="button"
                @click="logFilter = 'raw'"
              >
                Raw / PHP
              </button>
            </div>

            <!-- Search input -->
            <div class="log-search-wrap">
              <svg class="search-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M11.742 10.344a6.5 6.5 0 1 0-1.397 1.398h-.001c.03.04.062.078.098.115l3.85 3.85a1 1 0 0 0 1.415-1.414l-3.85-3.85a1.007 1.007 0 0 0-.115-.1zM12 6.5a5.5 5.5 0 1 1-11 0 5.5 5.5 0 0 1 11 0z"/>
              </svg>
              <input
                v-model="logSearch"
                type="search"
                class="log-search-input"
                placeholder="Cari URI, status, atau pesan..."
              />
            </div>
          </div>

          <div class="toolbar-right">
            <!-- Autoscroll toggle button -->
            <button
              class="btn-tool"
              :class="{ 'tool-active': autoScroll }"
              type="button"
              :title="autoScroll ? 'Auto-scroll aktif: scroll mengikuti baris terbaru' : 'Auto-scroll nonaktif'"
              @click="autoScroll = !autoScroll"
            >
              <svg class="tool-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M8 1a.75.75 0 0 1 .75.75v8.69l3.22-3.22a.75.75 0 1 1 1.06 1.06l-4.5 4.5a.75.75 0 0 1-1.06 0l-4.5-4.5a.75.75 0 1 1 1.06-1.06l3.22 3.22V1.75A.75.75 0 0 1 8 1Z"/>
                <path d="M2.5 13.5a.75.75 0 0 1 .75-.75h9.5a.75.75 0 0 1 0 1.5h-9.5a.75.75 0 0 1-.75-.75Z"/>
              </svg>
              Auto-scroll
            </button>

            <!-- Copy button -->
            <button
              class="btn-tool"
              type="button"
              title="Salin semua log yang sedang tampil"
              @click="copyVisibleLogs"
            >
              <svg class="tool-icon" viewBox="0 0 16 16" fill="currentColor">
                <path d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 0 1 0 1.5h-1.5a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-1.5a.75.75 0 0 1 1.5 0v1.5A1.75 1.75 0 0 1 9.25 16h-7.5A1.75 1.75 0 0 1 0 14.25v-7.5Z"/>
                <path d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0 1 14.25 11h-7.5A1.75 1.75 0 0 1 5 9.25v-7.5Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25h-7.5Z"/>
              </svg>
              {{ copySuccess ? 'Tersalin!' : 'Salin' }}
            </button>

            <!-- Live toggle: polling log otomatis, tanpa klik Segarkan -->
            <button
              class="btn-tool"
              :class="{ 'tool-active': liveLog }"
              type="button"
              :aria-pressed="liveLog"
              :title="liveLog ? 'Live aktif: log dimuat ulang setiap 2 detik' : 'Muat log otomatis setiap 2 detik'"
              @click="toggleLive"
            >
              <svg class="tool-icon" viewBox="0 0 16 16" fill="currentColor">
                <circle cx="8" cy="8" r="3"/>
              </svg>
              Live
            </button>

            <!-- Refresh button -->
            <button class="btn-tool" type="button" @click="refreshCurrentLog">
              <svg class="tool-icon" viewBox="0 0 16 16" fill="currentColor">
                <path fill-rule="evenodd" d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
                <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
              </svg>
              Segarkan
            </button>
          </div>
        </div>

        <!-- 4 states for logs -->
        <div v-if="state.logError" class="log-state log-error">
          <p>{{ state.logError }}</p>
        </div>

        <div v-else-if="state.logLines.length === 0" class="log-state log-empty">
          <div class="empty-icon-wrap">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <rect x="2" y="3" width="20" height="14" rx="2"/>
              <line x1="8" y1="21" x2="16" y2="21"/>
              <line x1="12" y1="17" x2="12" y2="21"/>
            </svg>
          </div>
          <p class="state-msg">Belum ada rekaman aktivitas.</p>
          <p class="state-sub">Log otomatis dicatat saat browser atau aplikasi mengirim permintaan ke situs ini.</p>
        </div>

        <div v-else-if="filteredLogs.length === 0" class="log-state log-empty">
          <p class="state-msg">Tidak ada log yang cocok dengan filter atau pencarian Anda.</p>
        </div>

        <!-- Clean, Scannable Log Console Feed -->
        <div v-else ref="logsViewport" class="logs-feed-viewport">
          <div class="logs-feed">
            <template v-for="(item, idx) in filteredLogs" :key="item.id">
              <!-- Date Divider Badge -->
              <div v-if="isDateChanged(idx)" class="log-date-divider">
                <span class="date-text">{{ item.access?.date }}</span>
              </div>

              <!-- Formatted HTTP Access Entry -->
              <div
                v-if="item.isAccess && item.access"
                class="log-entry log-entry-http"
                :class="`status-tone-${statusTone(item.access.status)}`"
              >
                <!-- Time -->
                <span class="entry-time font-mono">{{ item.access.time }}</span>

                <!-- HTTP Status Code Pill -->
                <span
                  class="entry-status-code font-mono"
                  :class="`badge-status-${statusTone(item.access.status)}`"
                >
                  {{ item.access.status }}
                </span>

                <!-- HTTP Method -->
                <span
                  class="entry-method font-mono"
                  :class="`method-${item.access.method.toLowerCase()}`"
                >
                  {{ item.access.method }}
                </span>

                <!-- Path / URI -->
                <span class="entry-uri font-mono" :title="item.access.uri">
                  {{ item.access.uri }}
                </span>

                <!-- Duration & Size Meta -->
                <div class="entry-meta font-mono">
                  <span v-if="item.access.durationMs !== null" class="meta-duration">
                    {{ formatDuration(item.access.durationMs) }}
                  </span>
                  <span v-if="item.access.size !== null" class="meta-size">
                    {{ formatBytes(item.access.size) }}
                  </span>
                </div>
              </div>

              <!-- Raw Line (PHP Error, Exception, Notice, or Caddy stdout/stderr) -->
              <div
                v-else
                class="log-entry log-entry-raw"
                :class="{
                  'raw-error': item.rawLevel === 'error',
                  'raw-warn': item.rawLevel === 'warn',
                  'raw-info': item.rawLevel === 'info',
                }"
              >
                <span class="raw-prefix font-mono">{{ item.rawLevel === 'error' ? 'ERR' : item.rawLevel === 'warn' ? 'WRN' : 'SYS' }}</span>
                <span class="raw-content font-mono">{{ item.raw }}</span>
              </div>
            </template>
          </div>
        </div>
      </section>

      <!-- Sub-tab 3: COMMANDS -->
      <section v-else-if="state.siteSubTab === 'commands'" class="subtab-panel panel-commands">
        <div class="commands-header">
          <p class="commands-desc">
            Klik perintah cepat di bawah ini untuk menjalankannya langsung di terminal folder project situs ini:
          </p>
        </div>

        <!-- 4 states for commands -->
        <div v-if="state.commandsError" class="command-state command-error">
          <p>{{ state.commandsError }}</p>
        </div>

        <div v-else-if="!state.siteCommands" class="command-state command-loading">
          <span class="spinner"></span>
          <p>Mendeteksi perintah yang tersedia...</p>
        </div>

        <div v-else-if="state.siteCommands.length === 0" class="command-state command-empty">
          <p>Tidak ada perintah cepat bawaan yang terdeteksi untuk project ini.</p>
        </div>

        <div v-else class="commands-grid">
          <button
            v-for="cmd in state.siteCommands"
            :key="cmd.id"
            class="command-btn"
            type="button"
            @click="runCommand(cmd.command)"
          >
            <span class="command-name">{{ cmd.label }}</span>
            <code class="command-snippet">{{ cmd.command }}</code>
          </button>

          <!-- Bun run TypeScript file -->
          <button class="command-btn" type="button" @click="runTsFile">
            <span class="command-name">Jalankan Berkas TS (Bun)</span>
            <code class="command-snippet">bun &lt;file.ts&gt;</code>
          </button>
        </div>
      </section>
    </div>
  </main>
</template>

<style scoped>
.site-detail-pane {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  height: 100%;
  overflow: hidden;
}

.detail-empty-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 40px;
}

.empty-box {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  max-width: 320px;
  text-align: center;
}

.empty-icon {
  width: 48px;
  height: 48px;
  color: var(--muted);
  opacity: 0.5;
}

.empty-text {
  margin: 0;
  font-size: 13px;
  color: var(--muted);
  line-height: 1.5;
}

.detail-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.detail-head {
  padding: 14px 20px;
  background: var(--surface);
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.head-left {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.title-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.site-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: var(--fg);
}

.status-pill {
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 11px;
  font-weight: 600;
  text-transform: capitalize;
}

.pill-ready {
  background: rgba(74, 222, 128, 0.15);
  color: var(--ok);
}

.pill-starting {
  background: rgba(245, 158, 11, 0.15);
  color: var(--accent);
}

.pill-failed {
  background: rgba(248, 113, 113, 0.15);
  color: var(--bad);
}

.pill-stopped {
  background: var(--line);
  color: var(--muted);
}

.domain-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.domain-link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: none;
  border: none;
  padding: 0;
  color: var(--accent);
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
}

.domain-link:hover {
  text-decoration: underline;
}

.link-icon {
  width: 12px;
  height: 12px;
}

.port-badge {
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--muted);
}

.head-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.menu-wrap {
  position: relative;
}

.btn-menu {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.icon-caret {
  width: 12px;
  height: 12px;
  opacity: 0.7;
}

.dropdown-popover {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  width: 180px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  box-shadow: 0 8px 20px rgba(0, 0, 0, 0.2);
  z-index: 100;
  padding: 4px;
  display: flex;
  flex-direction: column;
}

.dropdown-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  height: 28px;
  padding: 0 8px;
  background: none;
  border: none;
  border-radius: 4px;
  color: var(--fg);
  font-size: 12px;
  cursor: pointer;
  text-align: left;
}

.dropdown-item:hover:not(:disabled) {
  background: var(--bg);
}

.dropdown-item:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.item-unavailable {
  font-size: 10px;
  color: var(--muted);
}

.popover-loading,
.popover-empty {
  padding: 8px;
  font-size: 11px;
  color: var(--muted);
  text-align: center;
}

.icon-trash {
  width: 14px;
  height: 14px;
}

.btn-danger-quiet {
  background: none;
  border: 1px solid transparent;
  color: var(--muted);
  padding: 0 8px;
  height: 30px;
  border-radius: 4px;
  cursor: pointer;
}

.btn-danger-quiet:hover {
  background: rgba(185, 28, 28, 0.1);
  color: var(--bad);
  border-color: rgba(185, 28, 28, 0.25);
}

/* Sub-tabs */
.subtabs-bar {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 20px;
  background: var(--surface);
  border-bottom: 1px solid var(--line);
  height: 36px;
}

.subtab-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 100%;
  padding: 0 12px;
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--muted);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s;
}

.subtab-btn:hover {
  color: var(--fg);
}

.subtab-btn.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
  font-weight: 600;
}

.subtab-pill {
  font-size: 10px;
  padding: 0 5px;
  background: var(--line);
  border-radius: 8px;
  color: var(--muted);
}

.subtab-panel {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

/* General tab */
.panel-general {
  padding: 20px;
  gap: 20px;
}

.card-section {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.section-title {
  margin: 0;
  font-size: 13px;
  font-weight: 650;
  color: var(--fg);
}

.field-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--fg);
}

.field-control {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.select-php {
  width: 220px;
}

.field-hint {
  margin: 0;
  font-size: 11px;
  color: var(--muted);
}

.info-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px 16px;
}

.info-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.info-item.full {
  grid-column: span 2;
}

.info-label {
  font-size: 11px;
  color: var(--muted);
}

.info-value {
  font-size: 12px;
  color: var(--fg);
  word-break: break-all;
}

.font-mono {
  font-family: var(--font-mono);
}

/* ------------------------------------------------------------- */
/* Polished Developer Log Viewer Styles                           */
/* ------------------------------------------------------------- */

.panel-logs {
  padding: 0;
  background: var(--bg);
}

.logs-toolbar {
  padding: 8px 14px;
  background: var(--surface);
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.toolbar-left {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 1;
}

.log-segmented {
  display: inline-flex;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  padding: 2px;
  gap: 2px;
}

.seg-btn {
  height: 22px;
  padding: 0 8px;
  font-size: 11px;
  font-weight: 500;
  border: none;
  background: transparent;
  color: var(--muted);
  border-radius: 3px;
  cursor: pointer;
  transition: background 0.1s, color 0.1s;
}

.seg-btn:hover {
  color: var(--fg);
}

.seg-btn.active {
  background: var(--surface);
  color: var(--fg);
  font-weight: 600;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.log-search-wrap {
  position: relative;
  display: flex;
  align-items: center;
  width: 200px;
}

.search-icon {
  position: absolute;
  left: 8px;
  width: 11px;
  height: 11px;
  color: var(--muted);
  pointer-events: none;
}

.log-search-input {
  width: 100%;
  height: 26px;
  padding: 0 8px 0 24px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  font-size: 11px;
  color: var(--fg);
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 6px;
}

.btn-tool {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 8px;
  background: transparent;
  border: 1px solid var(--line);
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  color: var(--muted);
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s, background 0.1s;
}

.btn-tool:hover {
  color: var(--fg);
  background: var(--bg);
  border-color: var(--muted);
}

.tool-active {
  background: var(--bg);
  color: var(--accent);
  border-color: var(--accent);
  font-weight: 600;
}

.tool-icon {
  width: 12px;
  height: 12px;
}

.log-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  color: var(--muted);
  text-align: center;
  gap: 8px;
}

.empty-icon-wrap {
  width: 36px;
  height: 36px;
  opacity: 0.5;
}

.state-msg {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--fg);
}

.state-sub {
  margin: 0;
  font-size: 11px;
  color: var(--muted);
}

.log-error {
  color: var(--bad);
}

.logs-feed-viewport {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
  background: var(--bg);
}

.logs-feed {
  display: flex;
  flex-direction: column;
}

/* Date Divider */
.log-date-divider {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 14px;
  user-select: none;
}

.log-date-divider::before,
.log-date-divider::after {
  content: "";
  flex: 1;
  height: 1px;
  background: var(--line);
}

.date-text {
  font-size: 10px;
  font-weight: 600;
  color: var(--muted);
  font-family: var(--font-mono);
  letter-spacing: 0.04em;
  background: var(--surface);
  border: 1px solid var(--line);
  padding: 1px 8px;
  border-radius: 10px;
}

/* HTTP Access Log Row */
.log-entry-http {
  display: grid;
  grid-template-columns: 58px 42px 42px minmax(0, 1fr) auto;
  gap: 10px;
  align-items: baseline;
  padding: 3px 14px;
  font-size: 11.5px;
  line-height: 1.5;
  transition: background 0.08s;
}

.log-entry-http:hover {
  background: rgba(125, 125, 125, 0.07);
}

.entry-time {
  color: var(--muted);
  font-size: 11px;
  white-space: nowrap;
}

/* HTTP Status Badges */
.entry-status-code {
  font-weight: 700;
  font-size: 11px;
  text-align: center;
  border-radius: 3px;
  padding: 0 4px;
}

.badge-status-ok {
  color: var(--ok);
  background: rgba(74, 222, 128, 0.12);
}

.badge-status-warn {
  color: var(--accent);
  background: rgba(245, 158, 11, 0.12);
}

.badge-status-bad {
  color: var(--bad);
  background: rgba(248, 113, 113, 0.16);
}

/* HTTP Method Badges */
.entry-method {
  font-weight: 700;
  font-size: 10.5px;
  text-transform: uppercase;
  color: var(--muted);
}

.method-get {
  color: #38bdf8;
}

.method-post {
  color: var(--ok);
}

.method-put,
.method-patch {
  color: var(--accent);
}

.method-delete {
  color: var(--bad);
}

/* URI / Path */
.entry-uri {
  color: var(--fg);
  overflow-wrap: anywhere;
  word-break: break-all;
}

/* Duration & Size Meta */
.entry-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 11px;
  white-space: nowrap;
  margin-left: auto;
}

.meta-duration {
  color: var(--muted);
}

.meta-size {
  color: var(--muted);
  min-width: 48px;
  text-align: right;
}

/* Raw / Error Log Entries */
.log-entry-raw {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 3px 14px;
  margin: 1px 0;
  border-left: 3px solid var(--line);
  font-size: 11.5px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
}

.raw-prefix {
  font-size: 9.5px;
  font-weight: 700;
  padding: 1px 4px;
  border-radius: 3px;
  flex-shrink: 0;
}

.raw-error {
  border-left-color: var(--bad);
  background: rgba(185, 28, 28, 0.08);
  color: var(--bad);
}

.raw-error .raw-prefix {
  background: var(--bad);
  color: #ffffff;
}

.raw-warn {
  border-left-color: var(--accent);
  background: rgba(245, 158, 11, 0.08);
  color: var(--accent);
}

.raw-warn .raw-prefix {
  background: var(--accent);
  color: var(--on-accent);
}

.raw-info {
  border-left-color: var(--line);
  color: var(--muted);
}

.raw-info .raw-prefix {
  background: var(--line);
  color: var(--muted);
}

.raw-content {
  flex: 1;
}

/* Commands tab */
.panel-commands {
  padding: 20px;
  gap: 16px;
}

.commands-desc {
  margin: 0;
  font-size: 12px;
  color: var(--muted);
}

.commands-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 10px;
}

.command-btn {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  padding: 10px 12px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  cursor: pointer;
  text-align: left;
  transition: border-color 0.1s, background 0.1s;
}

.command-btn:hover {
  border-color: var(--accent);
  background: var(--bg);
}

.command-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--fg);
}

.command-snippet {
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--muted);
}

.command-state {
  padding: 30px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--muted);
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

.btn-quiet {
  background: transparent;
  border-color: var(--line);
  color: var(--fg);
}

.input {
  height: 32px;
  padding: 0 10px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--fg);
  font-size: 13px;
}
</style>
