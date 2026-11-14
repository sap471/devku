<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { formatBytes } from "../../log-format";
import {
  loadLogSources,
  loadServiceLog,
  openLogFile,
  state,
} from "../../store/devku";
import type { LogSource } from "../../types";

const selectedLines = ref<number>(200);
const logFilter = ref<"all" | "errors" | "warnings">("all");
const logSearch = ref<string>("");
const autoScroll = ref<boolean>(true);
const logsViewport = ref<HTMLElement | null>(null);
const copySuccess = ref<boolean>(false);

// Live polling every 2s
const liveLog = ref<boolean>(false);
let liveTimer: number | null = null;
let liveBusy = false;

const currentSource = computed<LogSource | null>(() => {
  if (!state.serviceLogFor) return null;
  return state.logSources.find((s) => s.name === state.serviceLogFor) ?? null;
});

const groupedSources = computed<Record<string, LogSource[]>>(() => {
  const groups: Record<string, LogSource[]> = {
    server: [],
    database: [],
    php: [],
    site: [],
  };
  for (const s of state.logSources) {
    if (groups[s.kind]) {
      groups[s.kind].push(s);
    } else {
      groups.site.push(s);
    }
  }
  return groups;
});

const KIND_LABEL: Record<string, string> = {
  server: "Server Web",
  database: "Database",
  php: "Runtime PHP",
  site: "Situs",
};

interface ClassifiedLine {
  id: number;
  raw: string;
  level: "error" | "warn" | "info";
}

function classifyLine(line: string): "error" | "warn" | "info" {
  if (
    /FATAL|PANIC|ERROR|Level Log\s*:\s*Fatal|"level":"error"|\[error\]|AH00\d+/i.test(
      line
    )
  ) {
    return "error";
  }
  if (/WARNING|Warning|\[warn\]|"level":"warn"/i.test(line)) {
    return "warn";
  }
  return "info";
}

const parsedLogs = computed<ClassifiedLine[]>(() => {
  return state.serviceLogLines.map((line, idx) => ({
    id: idx,
    raw: line,
    level: classifyLine(line),
  }));
});

const filteredLogs = computed<ClassifiedLine[]>(() => {
  let list = parsedLogs.value;
  if (logFilter.value === "errors") {
    list = list.filter((item) => item.level === "error");
  } else if (logFilter.value === "warnings") {
    list = list.filter((item) => item.level === "warn" || item.level === "error");
  }

  const q = logSearch.value.trim().toLowerCase();
  if (q) {
    list = list.filter((item) => item.raw.toLowerCase().includes(q));
  }
  return list;
});

function formatTime(unixSecs: number | null): string {
  if (!unixSecs) return "-";
  const date = new Date(unixSecs * 1000);
  const now = new Date();
  const isToday =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();

  const hh = String(date.getHours()).padStart(2, "0");
  const mm = String(date.getMinutes()).padStart(2, "0");
  if (isToday) {
    return `${hh}:${mm}`;
  }
  const yyyy = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const dd = String(date.getDate()).padStart(2, "0");
  return `${yyyy}-${month}-${dd} ${hh}:${mm}`;
}

async function selectSource(source: LogSource) {
  state.serviceLogFor = source.name;
  await loadServiceLog(source.name, selectedLines.value);
}

async function onLinesChange(event: Event) {
  const val = Number((event.target as HTMLSelectElement).value);
  selectedLines.value = val;
  if (state.serviceLogFor) {
    await loadServiceLog(state.serviceLogFor, val);
  }
}

async function refreshCurrentLog() {
  if (state.serviceLogFor) {
    await loadServiceLog(state.serviceLogFor, selectedLines.value);
  }
}

async function liveTick(): Promise<void> {
  if (liveBusy || !state.serviceLogFor) return;
  liveBusy = true;
  try {
    await loadServiceLog(state.serviceLogFor, selectedLines.value);
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

async function copyVisibleLogs() {
  const text = filteredLogs.value.map((l) => l.raw).join("\n");
  try {
    await navigator.clipboard.writeText(text);
    copySuccess.value = true;
    setTimeout(() => {
      copySuccess.value = false;
    }, 2000);
  } catch {
    // clipboard might fail if permission denied
  }
}

watch(
  () => state.serviceLogLines.length,
  async () => {
    if (autoScroll.value && logsViewport.value) {
      await nextTick();
      logsViewport.value.scrollTop = logsViewport.value.scrollHeight;
    }
  }
);

watch(
  () => state.settingsTab,
  (tab) => {
    if (tab !== "logs") {
      stopLive();
    } else if (liveLog.value) {
      startLive();
    }
  }
);

// Auto-select first source if none selected
watch(
  () => state.logSources,
  (sources) => {
    if (sources.length > 0 && !state.serviceLogFor) {
      void selectSource(sources[0]);
    }
  },
  { immediate: true }
);

onMounted(() => {
  void loadLogSources();
});

onBeforeUnmount(() => {
  stopLive();
});
</script>

<template>
  <div class="logs-pane">
    <header class="pane-header">
      <div>
        <h2 class="pane-title">Log Layanan</h2>
        <p class="pane-desc">
          Melihat aktivitas dan catatan kesalahan layanan lokal dari
          <code>{{ state.snapshot?.home_path ? state.snapshot.home_path + '\\runtime\\logs' : '%APPDATA%\\DevKu\\runtime\\logs' }}</code>.
        </p>
      </div>
      <div class="header-actions">
        <button
          class="btn btn-sm btn-outline"
          type="button"
          :disabled="state.logSourcesLoading"
          @click="loadLogSources()"
        >
          Muat Ulang Berkas
        </button>
      </div>
    </header>

    <!-- Loading sources state -->
    <div v-if="state.logSourcesLoading && state.logSources.length === 0" class="log-state">
      <p class="state-msg">Memuat daftar berkas log...</p>
    </div>

    <!-- Error loading sources -->
    <div v-else-if="state.logSourcesError" class="log-state log-error-box">
      <p class="state-msg">Gagal memuat berkas log: {{ state.logSourcesError }}</p>
      <button class="btn btn-sm btn-outline" type="button" @click="loadLogSources()">Coba Lagi</button>
    </div>

    <!-- Empty sources -->
    <div v-else-if="state.logSources.length === 0" class="log-state">
      <p class="state-msg">Belum ada berkas log yang ditemukan.</p>
      <p class="state-sub">Berkas log akan muncul setelah layanan pertama kali dijalankan.</p>
    </div>

    <!-- Main 2-column layout: Source list on left, Viewport on right -->
    <div v-else class="logs-layout">
      <!-- Sources sidebar/list -->
      <aside class="sources-list" role="list" aria-label="Daftar berkas log">
        <template v-for="(items, kind) in groupedSources" :key="kind">
          <div v-if="items.length > 0" class="source-group">
            <span class="source-group-title">{{ KIND_LABEL[kind] ?? kind }}</span>
            <div class="source-group-items">
              <button
                v-for="source in items"
                :key="source.name"
                class="source-item"
                :class="{ active: state.serviceLogFor === source.name }"
                type="button"
                :aria-current="state.serviceLogFor === source.name ? 'true' : undefined"
                @click="selectSource(source)"
              >
                <div class="source-item-main">
                  <span class="source-name font-mono">{{ source.name }}</span>
                  <span class="source-meta">
                    {{ formatBytes(source.bytes) }} · {{ formatTime(source.modified) }}
                  </span>
                </div>
              </button>
            </div>
          </div>
        </template>
      </aside>

      <!-- Log Content Area -->
      <section class="logs-content" aria-label="Isi log layanan">
        <!-- Toolbar -->
        <div class="logs-toolbar">
          <div class="toolbar-left">
            <!-- Filter Pills -->
            <div class="filter-pills" role="radiogroup" aria-label="Filter baris log">
              <button
                class="filter-pill"
                :class="{ active: logFilter === 'all' }"
                type="button"
                @click="logFilter = 'all'"
              >
                Semua
              </button>
              <button
                class="filter-pill"
                :class="{ active: logFilter === 'errors' }"
                type="button"
                @click="logFilter = 'errors'"
              >
                Galat
              </button>
              <button
                class="filter-pill"
                :class="{ active: logFilter === 'warnings' }"
                type="button"
                @click="logFilter = 'warnings'"
              >
                Peringatan
              </button>
            </div>

            <!-- Search input -->
            <div class="search-box">
              <input
                v-model="logSearch"
                class="search-input"
                type="text"
                placeholder="Cari teks di log..."
                aria-label="Cari di log"
              />
            </div>
          </div>

          <div class="toolbar-right">
            <!-- Line count selector -->
            <select
              class="select-input select-sm"
              :value="selectedLines"
              aria-label="Jumlah baris ekor log"
              @change="onLinesChange"
            >
              <option :value="200">200 baris</option>
              <option :value="1000">1000 baris</option>
              <option :value="5000">5000 baris</option>
            </select>

            <!-- Auto-scroll -->
            <button
              class="btn-tool"
              :class="{ 'tool-active': autoScroll }"
              type="button"
              :aria-pressed="autoScroll"
              title="Gulir otomatis ke baris terbaru"
              @click="autoScroll = !autoScroll"
            >
              Auto-scroll
            </button>

            <!-- Copy -->
            <button
              class="btn-tool"
              type="button"
              title="Salin baris yang sedang tampil"
              @click="copyVisibleLogs"
            >
              {{ copySuccess ? "Tersalin!" : "Salin" }}
            </button>

            <!-- Live -->
            <button
              class="btn-tool"
              :class="{ 'tool-active': liveLog }"
              type="button"
              :aria-pressed="liveLog"
              :title="liveLog ? 'Live aktif: memuat ulang tiap 2 detik' : 'Nyalakan pembaruan otomatis tiap 2 detik'"
              @click="toggleLive"
            >
              Live
            </button>

            <!-- Refresh -->
            <button class="btn-tool" type="button" title="Segarkan isi log" @click="refreshCurrentLog">
              Segarkan
            </button>

            <!-- Open file -->
            <button
              v-if="state.serviceLogFor"
              class="btn-tool"
              type="button"
              title="Buka berkas di aplikasi bawaan sistem"
              @click="openLogFile(state.serviceLogFor)"
            >
              Buka Berkas
            </button>
          </div>
        </div>

        <!-- Viewport content states -->
        <div v-if="state.serviceLogError" class="log-viewport-state log-error-box">
          <p class="state-msg">{{ state.serviceLogError }}</p>
        </div>

        <div
          v-else-if="state.serviceLogLines.length === 0"
          class="log-viewport-state"
        >
          <p class="state-msg">Berkas ini kosong.</p>
          <p class="state-sub">
            Layanan belum mencatat aktivitas apa pun ke berkas ini.
          </p>
        </div>

        <div
          v-else-if="filteredLogs.length === 0"
          class="log-viewport-state"
        >
          <p class="state-msg">Tidak ada baris yang cocok dengan filter atau pencarian.</p>
        </div>

        <!-- Log Viewport -->
        <div v-else ref="logsViewport" class="logs-feed-viewport" aria-label="Isi konsol log">
          <div class="logs-feed">
            <div
              v-for="item in filteredLogs"
              :key="item.id"
              class="log-entry"
              :class="{
                'entry-error': item.level === 'error',
                'entry-warn': item.level === 'warn',
              }"
            >
              <span class="line-number font-mono">{{ item.id + 1 }}</span>
              <span class="line-text font-mono">{{ item.raw }}</span>
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.logs-pane {
  display: flex;
  flex-direction: column;
  gap: 16px;
  height: 100%;
}

.pane-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--line);
}

.pane-title {
  margin: 0;
  font-size: 16px;
  font-weight: 650;
  color: var(--fg);
}

.pane-desc {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--muted);
}

.pane-desc code {
  font-family: var(--font-mono);
  font-size: 11px;
}

.logs-layout {
  display: flex;
  gap: 16px;
  height: 600px;
  min-height: 480px;
}

.sources-list {
  width: 220px;
  flex: 0 0 220px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 10px;
  overflow-y: auto;
}

.source-group {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.source-group-title {
  font-size: 10px;
  font-weight: 700;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 2px 6px;
}

.source-group-items {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.source-item {
  display: flex;
  align-items: center;
  padding: 6px 8px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: var(--fg);
  cursor: pointer;
  text-align: left;
  transition: background 0.1s;
}

.source-item:hover {
  background: var(--bg);
}

.source-item.active {
  background: var(--bg);
  border-left: 2px solid var(--primary, var(--fg));
  font-weight: 600;
}

.source-item-main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.source-name {
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.source-meta {
  font-size: 10px;
  color: var(--muted);
}

.logs-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  overflow: hidden;
  min-width: 0;
}

.logs-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--line);
  background: var(--surface);
}

.toolbar-left,
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 6px;
}

.filter-pills {
  display: flex;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  padding: 2px;
  gap: 2px;
}

.filter-pill {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 3px;
  border: none;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}

.filter-pill:hover {
  color: var(--fg);
}

.filter-pill.active {
  background: var(--surface);
  color: var(--fg);
  font-weight: 600;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.search-input {
  font-size: 11px;
  padding: 4px 8px;
  border-radius: 4px;
  border: 1px solid var(--line);
  background: var(--bg);
  color: var(--fg);
  min-width: 160px;
}

.select-sm {
  font-size: 11px;
  padding: 3px 6px;
  height: 26px;
}

.btn-tool {
  font-size: 11px;
  padding: 4px 8px;
  border-radius: 4px;
  border: 1px solid var(--line);
  background: var(--bg);
  color: var(--fg);
  cursor: pointer;
  transition: all 0.1s;
}

.btn-tool:hover {
  background: var(--surface);
}

.btn-tool.tool-active {
  background: var(--primary, #0066cc);
  color: #fff;
  border-color: var(--primary, #0066cc);
}

.logs-feed-viewport {
  flex: 1;
  overflow-y: auto;
  overflow-x: auto;
  background: var(--code-bg, #0d1117);
  color: var(--code-fg, #c9d1d9);
  padding: 8px 0;
  font-family: var(--font-mono, monospace);
  font-size: 11.5px;
  line-height: 1.5;
}

.logs-feed {
  display: flex;
  flex-direction: column;
}

.log-entry {
  display: flex;
  padding: 1px 12px;
  white-space: pre-wrap;
  word-break: break-all;
}

.log-entry:hover {
  background: rgba(255, 255, 255, 0.03);
}

.line-number {
  flex: 0 0 40px;
  color: var(--muted, #6e7681);
  user-select: none;
  font-size: 10px;
  text-align: right;
  padding-right: 12px;
}

.line-text {
  flex: 1;
}

.entry-error {
  color: var(--bad, #f85149);
  background: rgba(248, 81, 73, 0.08);
}

.entry-warn {
  color: var(--warn, #d29922);
}

.log-state,
.log-viewport-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px 16px;
  text-align: center;
  color: var(--muted);
  gap: 8px;
}

.log-viewport-state {
  flex: 1;
}

.state-msg {
  font-size: 13px;
  font-weight: 500;
  color: var(--fg);
  margin: 0;
}

.state-sub {
  font-size: 11px;
  margin: 0;
}

.log-error-box {
  background: rgba(248, 81, 73, 0.05);
  border: 1px solid var(--bad, #f85149);
  border-radius: 4px;
}
</style>
