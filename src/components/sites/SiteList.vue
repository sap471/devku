<script setup lang="ts">
import { ref, computed } from "vue";
import { state, selectSite, toggleSite, STATUS_WORD, frameworkLabel } from "../../store/devku";
import type { SiteView } from "../../types";

const searchQuery = ref("");

const filteredSites = computed<SiteView[]>(() => {
  const sites = state.snapshot?.sites ?? [];
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return sites;
  return sites.filter(
    (s) =>
      s.name.toLowerCase().includes(q) ||
      s.domain.toLowerCase().includes(q) ||
      s.php.toLowerCase().includes(q) ||
      s.framework.toLowerCase().includes(q),
  );
});

function openWizard() {
  state.wizardOpen = true;
}
</script>

<template>
  <aside class="site-list-pane" aria-label="Daftar Situs">
    <!-- Pane Head -->
    <div class="list-head">
      <div class="head-top">
        <h1 class="pane-title">Situs</h1>
        <button class="btn btn-add" type="button" @click="openWizard">
          <svg class="icon-plus" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 2a.75.75 0 0 1 .75.75v4.5h4.5a.75.75 0 0 1 0 1.5h-4.5v4.5a.75.75 0 0 1-1.5 0v-4.5h-4.5a.75.75 0 0 1 0-1.5h4.5v-4.5A.75.75 0 0 1 8 2Z"/>
          </svg>
          Tambah
        </button>
      </div>

      <!-- Quick Filter -->
      <div v-if="(state.snapshot?.sites?.length ?? 0) > 3" class="filter-box">
        <input
          v-model="searchQuery"
          type="search"
          class="filter-input"
          placeholder="Cari domain, framework..."
        />
      </div>
    </div>

    <!-- 1. LOADING STATE -->
    <div v-if="state.loading && !state.snapshot" class="list-state list-loading">
      <span class="spinner"></span>
      <p>Memuat daftar situs...</p>
    </div>

    <!-- 2. ERROR STATE -->
    <div v-else-if="state.bootError" class="list-state list-error">
      <p class="state-title">Gagal memuat status</p>
      <p class="state-msg">{{ state.bootError }}</p>
    </div>

    <!-- 3. EMPTY STATE -->
    <div
      v-else-if="!state.snapshot?.sites?.length"
      class="list-state list-empty"
    >
      <div class="empty-icon-wrap">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <rect x="3" y="4" width="18" height="16" rx="2"/>
          <line x1="3" y1="9" x2="21" y2="9"/>
          <line x1="9" y1="20" x2="9" y2="9"/>
        </svg>
      </div>
      <p class="empty-title">Belum ada situs terdaftar</p>
      <p class="empty-text">
        Hubungkan folder project PHP lokal Anda untuk mulai melayani domain otomatis.
      </p>
      <button class="btn btn-primary" type="button" @click="openWizard">
        Tambah Situs Pertama
      </button>
    </div>

    <!-- 4. READY STATE -->
    <div v-else class="list-items" role="list">
      <div
        v-for="site in filteredSites"
        :key="site.id"
        class="site-item"
        :class="{
          selected: site.id === state.selectedSiteId,
          disabled: !site.enabled,
        }"
        role="listitem"
        @click="selectSite(site.id)"
      >
        <!-- Identity Status Bar (Left edge indicator) -->
        <span
          class="status-bar"
          :class="{
            'bar-ready': site.enabled && site.status === 'ready',
            'bar-failed': site.enabled && site.status === 'failed',
            'bar-starting': site.enabled && (site.status === 'starting' || site.status === 'backoff'),
            'bar-stopped': !site.enabled || site.status === 'stopped',
          }"
        ></span>

        <div class="site-main">
          <div class="site-name-row">
            <span class="site-name">{{ site.name }}</span>
            <span v-if="site.port" class="site-port">:{{ site.port }}</span>
          </div>

          <div class="site-domain-row">
            <span class="site-domain">{{ site.domain }}</span>
          </div>
        </div>

        <div class="site-meta">
          <span class="meta-tag">{{ frameworkLabel(site.framework) }}</span>
          <span class="meta-php">PHP {{ site.php }}</span>
        </div>

        <!-- Quick toggle switch -->
        <button
          class="btn-toggle"
          :class="{ active: site.enabled }"
          type="button"
          :title="site.enabled ? 'Nonaktifkan situs' : 'Aktifkan situs'"
          @click.stop="toggleSite(site.id)"
        >
          <span class="toggle-slider"></span>
        </button>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.site-list-pane {
  display: flex;
  flex-direction: column;
  width: 320px;
  flex: 0 0 320px;
  background: var(--surface);
  border-right: 1px solid var(--line);
  height: 100%;
  overflow: hidden;
}

.list-head {
  padding: 12px 14px;
  border-bottom: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.head-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.pane-title {
  margin: 0;
  font-size: 14px;
  font-weight: 650;
  color: var(--fg);
}

.btn-add {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 10px;
  background: var(--accent);
  color: var(--on-accent);
  border: 1px solid var(--accent);
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
}

.icon-plus {
  width: 12px;
  height: 12px;
}

.filter-box {
  display: flex;
}

.filter-input {
  width: 100%;
  height: 26px;
  padding: 0 8px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  font-size: 12px;
  color: var(--fg);
}

.list-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 24px 20px;
  text-align: center;
}

.list-loading {
  gap: 12px;
  color: var(--muted);
  font-size: 12px;
}

.spinner {
  width: 20px;
  height: 20px;
  border: 2px solid var(--line);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.list-empty {
  gap: 10px;
}

.empty-icon-wrap {
  width: 40px;
  height: 40px;
  color: var(--muted);
  opacity: 0.6;
}

.empty-title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--fg);
}

.empty-text {
  margin: 0;
  font-size: 12px;
  color: var(--muted);
  line-height: 1.4;
  max-width: 240px;
}

.list-items {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

.site-item {
  position: relative;
  display: flex;
  align-items: center;
  height: var(--row-h);
  padding: 0 12px 0 14px;
  border-bottom: 1px solid var(--line);
  cursor: pointer;
  transition: background 0.1s;
  user-select: none;
}

.site-item:hover {
  background: var(--bg);
}

.site-item.selected {
  background: var(--bg);
  box-shadow: inset 2px 0 0 var(--accent);
}

.site-item.disabled {
  opacity: 0.6;
}

/* Identity status bar on the left edge */
.status-bar {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 3px;
  background: var(--idle);
}

.bar-ready {
  background: var(--ok);
}

.bar-starting {
  background: var(--accent);
}

.bar-failed {
  background: var(--bad);
}

.bar-stopped {
  background: var(--idle);
}

.site-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.site-name-row {
  display: flex;
  align-items: baseline;
  gap: 6px;
}

.site-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--fg);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.site-port {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--muted);
}

.site-domain-row {
  display: flex;
}

.site-domain {
  font-size: 11px;
  color: var(--muted);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.site-meta {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 2px;
  margin-right: 8px;
}

.meta-tag {
  font-size: 10px;
  color: var(--muted);
  background: var(--bg);
  padding: 1px 4px;
  border-radius: 3px;
  border: 1px solid var(--line);
}

.meta-php {
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--muted);
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

.btn-primary {
  height: 28px;
  padding: 0 12px;
  background: var(--accent);
  color: var(--on-accent);
  border: 1px solid var(--accent);
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  margin-top: 6px;
}
</style>
