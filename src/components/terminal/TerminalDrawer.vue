<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, computed } from "vue";
import { state, currentSite } from "../../store/devku";
import * as terminalPanel from "../../terminal";

const terminalHost = ref<HTMLElement | null>(null);

const activeCwd = computed(() => {
  return currentSite.value?.root ?? null;
});

watch(
  () => state.terminalOpen,
  async (isOpen) => {
    if (isOpen) {
      // Allow DOM to render host element
      await new Promise((resolve) => setTimeout(resolve, 50));
      if (terminalHost.value) {
        await terminalPanel.open(activeCwd.value, terminalHost.value);
      }
    } else {
      await terminalPanel.close();
    }
  },
);

function closeTerminal() {
  state.terminalOpen = false;
  void terminalPanel.close();
}

onMounted(() => {
  if (state.terminalOpen && terminalHost.value) {
    void terminalPanel.open(activeCwd.value, terminalHost.value);
  }
});

onBeforeUnmount(() => {
  void terminalPanel.close();
});
</script>

<template>
  <section v-show="state.terminalOpen" class="terminal-drawer" aria-label="Terminal Terintegrasi">
    <div class="terminal-bar">
      <div class="bar-left">
        <span class="bar-badge">Shell</span>
        <span class="bar-title">Terminal</span>
        <span v-if="activeCwd" class="bar-cwd text-truncate font-mono">
          {{ activeCwd }}
        </span>
      </div>

      <div class="bar-actions">
        <button
          class="btn-close-terminal"
          type="button"
          aria-label="Tutup Terminal"
          @click="closeTerminal"
        >
          ✕
        </button>
      </div>
    </div>

    <div ref="terminalHost" id="terminal" class="terminal-canvas"></div>
  </section>
</template>

<style scoped>
.terminal-drawer {
  flex: 0 0 220px;
  height: 220px;
  background: var(--surface);
  border-top: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  z-index: 50;
  overflow: hidden;
}

.terminal-bar {
  flex: 0 0 30px;
  height: 30px;
  padding: 0 12px;
  background: var(--surface);
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
  user-select: none;
}

.bar-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.bar-badge {
  font-size: 10px;
  font-weight: 600;
  padding: 1px 5px;
  background: var(--line);
  border-radius: 3px;
  color: var(--muted);
  text-transform: uppercase;
}

.bar-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--fg);
}

.bar-cwd {
  font-size: 11px;
  color: var(--muted);
  max-width: 400px;
}

.font-mono {
  font-family: var(--font-mono);
}

.text-truncate {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.btn-close-terminal {
  background: none;
  border: none;
  color: var(--muted);
  font-size: 13px;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 3px;
}

.btn-close-terminal:hover {
  background: var(--line);
  color: var(--fg);
}

.terminal-canvas {
  flex: 1;
  background: var(--bg);
  padding: 6px 10px;
  overflow: hidden;
}
</style>
