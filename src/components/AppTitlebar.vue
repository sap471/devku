<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  state,
  isGlobalRunning,
  startAll,
  stopAll,
  trustCaddy,
  openWorkspace,
  STATUS_WORD,
  type MainTab,
} from "../store/devku";
import * as terminalPanel from "../terminal";

const appWindow = getCurrentWindow();
const isMaximized = ref(false);

onMounted(async () => {
  try {
    isMaximized.value = await appWindow.isMaximized();
    await appWindow.onResized(async () => {
      isMaximized.value = await appWindow.isMaximized();
    });
  } catch {
    // Running in web preview fallback
  }
});

async function minimizeWindow() {
  try {
    await appWindow.minimize();
  } catch (err) {
    console.error("Gagal meminimize jendela:", err);
  }
}

async function toggleMaximizeWindow() {
  try {
    await appWindow.toggleMaximize();
    isMaximized.value = await appWindow.isMaximized();
  } catch (err) {
    console.error("Gagal toggle maximize jendela:", err);
  }
}

async function closeWindow() {
  try {
    await appWindow.close();
  } catch (err) {
    console.error("Gagal menutup jendela:", err);
    try {
      await appWindow.hide();
    } catch (hideErr) {
      console.error("Gagal menyembunyikan jendela ke tray:", hideErr);
    }
  }
}

function toggleTerminal() {
  state.terminalOpen = !state.terminalOpen;
  const currentSite = state.snapshot?.sites.find((s) => s.id === state.selectedSiteId);
  if (state.terminalOpen) {
    void terminalPanel.open(currentSite?.root ?? null);
  } else {
    void terminalPanel.close();
  }
}

const statusLabel = computed(() => {
  if (!state.snapshot) return "Memuat...";
  if (!state.snapshot.running) return "Berhenti";
  const readyCount = state.snapshot.sites.filter((s) => s.status === "ready" && s.enabled).length;
  return `${readyCount} situs aktif`;
});

const workspacePath = computed(() => state.snapshot?.workspace.path ?? "");
const workspaceTitle = computed(() =>
  workspacePath.value
    ? `Buka folder workspace: ${workspacePath.value}`
    : "Buka folder workspace",
);
</script>

<template>
  <header class="app-titlebar" data-tauri-drag-region>
    <!-- Brand & Drag Region -->
    <div class="brand-zone" data-tauri-drag-region>
      <span class="brand-badge">DK</span>
      <span class="brand-title">DevKu</span>
    </div>

    <!-- Navigation Tabs -->
    <nav class="nav-tabs" data-tauri-drag-region="false" @mousedown.stop>
      <button
        class="nav-tab-btn"
        :class="{ active: state.activeTab === 'sites' }"
        type="button"
        @click="state.activeTab = 'sites'"
      >
        <svg class="tab-icon" viewBox="0 0 16 16" fill="currentColor">
          <path d="M2 3.75C2 2.784 2.784 2 3.75 2h8.5c.966 0 1.75.784 1.75 1.75v8.5A1.75 1.75 0 0 1 12.25 14h-8.5A1.75 1.75 0 0 1 2 12.25v-8.5Zm1.75-.25a.25.25 0 0 0-.25.25v8.5c0 .138.112.25.25.25h8.5a.25.25 0 0 0 .25-.25v-8.5a.25.25 0 0 0-.25-.25h-8.5Z"/>
          <path d="M5.5 5a.5.5 0 0 1 .5.5v5a.5.5 0 0 1-1 0v-5a.5.5 0 0 1 .5-.5Zm3 0a.5.5 0 0 1 .5.5v5a.5.5 0 0 1-1 0v-5a.5.5 0 0 1 .5-.5Z"/>
        </svg>
        Situs
        <span v-if="state.snapshot?.sites?.length" class="tab-badge">
          {{ state.snapshot.sites.length }}
        </span>
      </button>

      <button
        class="nav-tab-btn"
        :class="{ active: state.activeTab === 'settings' }"
        type="button"
        @click="state.activeTab = 'settings'"
      >
        <svg class="tab-icon" viewBox="0 0 16 16" fill="currentColor">
          <path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0Zm0 1.5a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13ZM6.5 5.5a1 1 0 1 1 2 0 1 1 0 0 1-2 0Zm0 4a1 1 0 1 1 2 0 1 1 0 0 1-2 0Z"/>
        </svg>
        Pengaturan
      </button>
    </nav>

    <!-- Global State & Status Indicator -->
    <div class="global-status-zone" data-tauri-drag-region>
      <div class="status-indicator">
        <span
          class="status-dot"
          :class="{
            'dot-online': isGlobalRunning,
            'dot-offline': !isGlobalRunning,
          }"
        ></span>
        <span class="status-text">{{ statusLabel }}</span>
      </div>

      <!-- Quick Server Start/Stop -->
      <div class="quick-controls" data-tauri-drag-region="false" @mousedown.stop>
        <button
          v-if="!isGlobalRunning"
          class="btn-titlebar btn-start"
          type="button"
          :disabled="state.busy === 'start'"
          @click="startAll"
        >
          Nyalakan
        </button>
        <button
          v-else
          class="btn-titlebar btn-stop"
          type="button"
          :disabled="state.busy === 'stop'"
          @click="stopAll"
        >
          Hentikan
        </button>

        <!-- Untrusted Caddy Warning Pill -->
        <button
          v-if="state.snapshot && !state.snapshot.https_trusted"
          class="btn-titlebar btn-warn"
          type="button"
          :disabled="state.busy === 'trust-caddy'"
          title="Percayai sertifikat HTTPS lokal Caddy"
          @click="trustCaddy"
        >
          Percayai HTTPS
        </button>

        <!-- Workspace Shortcut -->
        <button
          class="btn-titlebar btn-workspace"
          type="button"
          :title="workspaceTitle"
          aria-label="Buka folder workspace"
          @click="openWorkspace()"
        >
          <svg class="btn-icon" viewBox="0 0 16 16" fill="currentColor">
            <path d="M2 3.5A1.5 1.5 0 0 1 3.5 2h2.379a1.5 1.5 0 0 1 1.06.44l1.122 1.12a.5.5 0 0 0 .353.147H12.5A1.5 1.5 0 0 1 14 5.207V12.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12.5v-9Z"/>
          </svg>
          <span class="workspace-label">Workspace</span>
        </button>

        <!-- Terminal Toggle -->
        <button
          class="btn-titlebar"
          :class="{ 'btn-active': state.terminalOpen }"
          type="button"
          title="Buka / tutup terminal shell terintegrasi"
          @click="toggleTerminal"
        >
          <svg class="btn-icon" viewBox="0 0 16 16" fill="currentColor">
            <path d="M2 3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V3Zm2 2.5a.5.5 0 0 0-.707.707L5.086 8l-1.793 1.793a.5.5 0 0 0 .707.707l2.146-2.146a.5.5 0 0 0 0-.707L4 5.5Zm4 5.5a.5.5 0 0 0 0 1h4a.5.5 0 0 0 0-1H8Z"/>
          </svg>
          Terminal
        </button>
      </div>
    </div>

    <!-- Window Action Buttons (Minimize, Maximize, Close) -->
    <div class="window-controls" data-tauri-drag-region="false" @mousedown.stop>
      <button
        class="win-btn win-min"
        type="button"
        data-tauri-drag-region="false"
        title="Minimize"
        aria-label="Minimize"
        @click.stop="minimizeWindow"
      >
        <svg viewBox="0 0 11 11">
          <path d="M1 5.5h9" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
        </svg>
      </button>

      <button
        class="win-btn win-max"
        type="button"
        data-tauri-drag-region="false"
        :title="isMaximized ? 'Restore' : 'Maximize'"
        :aria-label="isMaximized ? 'Restore' : 'Maximize'"
        @click.stop="toggleMaximizeWindow"
      >
        <svg v-if="!isMaximized" viewBox="0 0 11 11">
          <rect x="1.5" y="1.5" width="8" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1.1"/>
        </svg>
        <svg v-else viewBox="0 0 11 11">
          <path d="M3 2.5h6v6" fill="none" stroke="currentColor" stroke-width="1.1"/>
          <rect x="1.5" y="4" width="5.5" height="5.5" rx="1" fill="none" stroke="currentColor" stroke-width="1.1"/>
        </svg>
      </button>

      <button
        class="win-btn win-close"
        type="button"
        data-tauri-drag-region="false"
        title="Tutup (Sembunyikan ke Tray)"
        aria-label="Close"
        @click.stop="closeWindow"
      >
        <svg viewBox="0 0 11 11">
          <path d="M2 2l7 7M9 2l-7 7" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.app-titlebar {
  flex: 0 0 42px;
  height: 42px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: var(--surface);
  border-bottom: 1px solid var(--line);
  user-select: none;
  -webkit-user-select: none;
  padding: 0 0 0 12px;
  font-size: 12px;
  z-index: 100;
}

.brand-zone {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-right: 12px;
}

.brand-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  background: var(--accent);
  color: var(--on-accent);
  border-radius: 4px;
  font-weight: 700;
  font-size: 10px;
  letter-spacing: -0.02em;
}

.brand-title {
  font-weight: 650;
  letter-spacing: -0.01em;
  font-size: 13px;
  color: var(--fg);
}

.nav-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  height: 100%;
}

.nav-tab-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 4px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.1s, color 0.1s;
}

.nav-tab-btn:hover {
  background: var(--bg);
  color: var(--fg);
}

.nav-tab-btn.active {
  background: var(--bg);
  border-color: var(--line);
  color: var(--fg);
  font-weight: 600;
}

.tab-icon {
  width: 14px;
  height: 14px;
  opacity: 0.8;
}

.tab-badge {
  display: inline-block;
  padding: 0 5px;
  font-size: 10px;
  line-height: 16px;
  border-radius: 8px;
  background: var(--line);
  color: var(--muted);
}

.global-status-zone {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 12px;
  padding-right: 12px;
  height: 100%;
}

.status-indicator {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--muted);
}

.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--idle);
}

.dot-online {
  background: var(--ok);
  box-shadow: 0 0 6px rgba(74, 222, 128, 0.4);
}

.dot-offline {
  background: var(--idle);
}

.quick-controls {
  display: flex;
  align-items: center;
  gap: 6px;
}

.btn-titlebar {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 26px;
  padding: 0 8px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--fg);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.1s, border-color 0.1s;
}

.btn-titlebar:hover:not(:disabled) {
  background: var(--surface);
  border-color: var(--accent);
}

/* The titlebar is already full at the 780px minimum width, so the label is dropped first
   and the button stays usable through its icon and tooltip. */
@media (max-width: 900px) {
  .workspace-label {
    display: none;
  }

  .btn-workspace {
    padding: 0 6px;
  }
}

.btn-start {
  color: var(--ok);
  border-color: var(--line);
}

.btn-stop {
  color: var(--bad);
}

.btn-warn {
  color: var(--accent);
  border-color: var(--accent);
  background: rgba(245, 158, 11, 0.08);
}

.btn-active {
  background: var(--line);
  border-color: var(--line);
  font-weight: 600;
}

.btn-icon {
  width: 12px;
  height: 12px;
}

/* Window controls */
.window-controls {
  display: flex;
  align-items: center;
  height: 100%;
}

.win-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 100%;
  border: none;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  transition: background 0.1s, color 0.1s;
}

.win-btn svg {
  width: 11px;
  height: 11px;
  pointer-events: none;
}

.win-btn:hover {
  background: var(--bg);
  color: var(--fg);
}

.win-close:hover {
  background: #e81123 !important;
  color: #ffffff !important;
}
</style>
