<script setup lang="ts">
import { state, clearNotices, cancelDownload } from "../../store/devku";
import { formatBytes } from "../../log-format";

function dismiss() {
  clearNotices();
}
</script>

<template>
  <div v-if="state.notice || state.infoNotice || state.configNotice || state.download" class="notice-container">
    <!-- Error notice -->
    <div v-if="state.notice" class="banner banner-error" role="alert">
      <span class="banner-text">{{ state.notice }}</span>
      <button class="banner-close" type="button" aria-label="Tutup pesan" @click="dismiss">✕</button>
    </div>

    <!-- Info notice -->
    <div v-else-if="state.infoNotice" class="banner banner-info" role="status">
      <span class="banner-text">{{ state.infoNotice }}</span>
      <button class="banner-close" type="button" aria-label="Tutup pesan" @click="dismiss">✕</button>
    </div>

    <!-- Config notice -->
    <div v-else-if="state.configNotice" class="banner banner-config" role="status">
      <span class="banner-text">{{ state.configNotice }}</span>
      <button class="banner-close" type="button" aria-label="Tutup pesan" @click="dismiss">✕</button>
    </div>

    <!-- Active download progress bar -->
    <div v-if="state.download" class="banner banner-download" role="progressbar">
      <div class="download-info">
        <span class="download-title">
          Mengunduh {{ state.download.target.toUpperCase() }} {{ state.download.version }}
          ({{ state.download.stage }})
        </span>
        <span class="download-stats">
          {{ formatBytes(state.download.received) }}
          <template v-if="state.download.total"> / {{ formatBytes(state.download.total) }}</template>
        </span>
      </div>
      <div class="progress-bar-track">
        <div
          class="progress-bar-fill"
          :style="{
            width: state.download.total ? `${Math.round((state.download.received / state.download.total) * 100)}%` : '100%',
          }"
          :class="{ 'progress-bar-indeterminate': !state.download.total }"
        ></div>
      </div>
      <button class="btn-cancel-download" type="button" @click="cancelDownload">Batal</button>
    </div>
  </div>
</template>

<style scoped>
.notice-container {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  font-size: 12px;
  line-height: 1.4;
  border-bottom: 1px solid var(--line);
}

.banner-text {
  flex: 1;
}

.banner-close {
  background: none;
  border: none;
  color: inherit;
  font-size: 14px;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 3px;
  opacity: 0.75;
}

.banner-close:hover {
  opacity: 1;
}

.banner-error {
  background: rgba(185, 28, 28, 0.1);
  color: var(--bad);
  border-color: rgba(185, 28, 28, 0.2);
}

.banner-info {
  background: var(--surface);
  color: var(--fg);
  border-color: var(--line);
}

.banner-config {
  background: rgba(180, 83, 9, 0.08);
  color: var(--accent);
  border-color: rgba(180, 83, 9, 0.2);
}

.banner-download {
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--surface);
  color: var(--fg);
  border-bottom: 1px solid var(--line);
}

.download-info {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  font-family: var(--font-mono);
}

.download-title {
  font-weight: 600;
}

.download-stats {
  color: var(--muted);
}

.progress-bar-track {
  flex: 1;
  height: 6px;
  background: var(--line);
  border-radius: 3px;
  overflow: hidden;
}

.progress-bar-fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.2s ease;
}

.progress-bar-indeterminate {
  width: 100% !important;
  animation: pulse-indeterminate 1.5s infinite;
}

@keyframes pulse-indeterminate {
  0% { opacity: 0.4; }
  50% { opacity: 1; }
  100% { opacity: 0.4; }
}

.btn-cancel-download {
  padding: 3px 8px;
  font-size: 11px;
  background: transparent;
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--bad);
  cursor: pointer;
}

.btn-cancel-download:hover {
  background: rgba(185, 28, 28, 0.1);
}
</style>
