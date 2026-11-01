<script setup lang="ts">
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { state, selectSite, loadLog } from "../../store/devku";
import type { Detection, Snapshot } from "../../types";

const step = ref<1 | 2 | 3>(1);
const projectPath = ref("");
const checking = ref(false);
const error = ref<string | null>(null);
const detection = ref<Detection | null>(null);
const siteName = ref("");
const domainSlug = ref("");
const selectedPhp = ref("");

const tld = computed(() => state.snapshot?.tld ?? "localhost");
const installedPhp = computed(() => state.snapshot?.installed_php ?? []);

function slug(value: string): string {
  return (
    value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "site"
  );
}

function cancel() {
  state.wizardOpen = false;
  reset();
}

function reset() {
  step.value = 1;
  projectPath.value = "";
  checking.value = false;
  error.value = null;
  detection.value = null;
  siteName.value = "";
  domainSlug.value = "";
  selectedPhp.value = "";
}

async function pickFolder() {
  try {
    const selected = await open({
      title: "Pilih folder project PHP",
      multiple: false,
      directory: true,
    });
    if (typeof selected === "string") {
      projectPath.value = selected;
      error.value = null;
      await nextStep();
    }
  } catch (err) {
    error.value = `Gagal memilih folder: ${String(err)}`;
  }
}

async function nextStep() {
  if (step.value === 1) {
    const path = projectPath.value.trim();
    if (!path) {
      error.value = "Silakan isi atau pilih folder project terlebih dahulu.";
      return;
    }

    checking.value = true;
    error.value = null;

    try {
      const found = await invoke<Detection | null>("detect_project", { path });
      checking.value = false;

      if (!found) {
        detection.value = {
          framework: "lain",
          label: "Lain (tanpa PHP)",
          document_root: path,
          project_root: path,
        };
      } else {
        detection.value = found;
      }

      step.value = 2;
      const folder = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "site";
      siteName.value = folder;
      domainSlug.value = slug(folder);

      const phpList = installedPhp.value;
      if (!selectedPhp.value && phpList.length > 0) {
        selectedPhp.value = state.snapshot?.default_php || phpList[phpList.length - 1];
      }
    } catch (err) {
      checking.value = false;
      error.value = String(err);
    }
    return;
  }

  if (step.value === 2) {
    if (!selectedPhp.value) {
      error.value = "Pilih versi PHP yang akan digunakan.";
      return;
    }
    if (!domainSlug.value.trim()) {
      error.value = "Nama domain tidak boleh kosong.";
      return;
    }
    error.value = null;
    step.value = 3;
    return;
  }
}

function prevStep() {
  error.value = null;
  if (step.value > 1) {
    step.value = (step.value - 1) as 1 | 2;
  }
}

async function submit() {
  if (!detection.value) return;

  state.busy = "add-site";
  error.value = null;

  try {
    const fullDomain = `${domainSlug.value.trim()}.${tld.value}`;
    const next = await invoke<Snapshot>("add_site", {
      draft: {
        path: detection.value.project_root,
        php: selectedPhp.value,
        name: siteName.value.trim() || undefined,
        domain: fullDomain,
        framework: detection.value.framework,
      },
    });

    state.snapshot = next;
    const addedSite = next.sites.find((s) => s.domain === fullDomain);
    if (addedSite) {
      selectSite(addedSite.id);
    }

    if (!next.onboarded) {
      void invoke<Snapshot>("finish_onboarding")
        .then((s) => { state.snapshot = s; })
        .catch(() => {});
    }

    state.infoNotice = `Situs ${siteName.value} (${fullDomain}) berhasil ditambahkan.`;
    state.wizardOpen = false;
    reset();
  } catch (err) {
    error.value = String(err);
  } finally {
    state.busy = null;
  }
}
</script>

<template>
  <div v-if="state.wizardOpen" class="wizard-backdrop" @click.stop>
    <div class="wizard-card" role="dialog" aria-modal="true" @click.stop>
      <!-- Wizard Header -->
      <div class="wizard-head">
        <div class="head-title">
          <h2>Tambah Situs Baru</h2>
          <span class="step-indicator">Langkah {{ step }} dari 3</span>
        </div>
        <button class="btn-close" type="button" aria-label="Batal" @click="cancel">✕</button>
      </div>

      <!-- Error Alert -->
      <div v-if="error" class="wizard-alert" role="alert">
        {{ error }}
      </div>

      <!-- Step 1: Directory Selection -->
      <div v-if="step === 1" class="wizard-step">
        <p class="step-desc">
          Pilih folder project web lokal Anda. DevKu akan otomatis mendeteksi framework dan document root (misalnya <code>public/</code> untuk Laravel).
        </p>

        <div class="folder-picker-group">
          <input
            v-model="projectPath"
            type="text"
            class="input-path"
            placeholder="C:\Projects\my-laravel-app"
            autofocus
            @keydown.enter.prevent="nextStep"
          />
          <button class="btn btn-primary" type="button" :disabled="checking" @click="pickFolder">
            Pilih Folder...
          </button>
        </div>

        <div v-if="checking" class="checking-box">
          <span class="spinner"></span>
          <span>Mendeteksi framework dan struktur project...</span>
        </div>
      </div>

      <!-- Step 2: Configuration -->
      <div v-else-if="step === 2 && detection" class="wizard-step">
        <div class="detection-badge">
          <span class="badge-label">Terdeteksi:</span>
          <strong>{{ detection.label }}</strong>
          <span class="badge-path">Root: {{ detection.document_root }}</span>
        </div>

        <div class="form-group">
          <label for="site-name" class="form-label">Nama Situs</label>
          <input
            id="site-name"
            v-model="siteName"
            type="text"
            class="input"
            placeholder="Nama tampilan di DevKu"
          />
        </div>

        <div class="form-group">
          <label for="domain-slug" class="form-label">Domain Lokal</label>
          <div class="domain-input-wrap">
            <input
              id="domain-slug"
              v-model="domainSlug"
              type="text"
              class="input domain-input"
              placeholder="nama-project"
            />
            <span class="domain-suffix">.{{ tld }}</span>
          </div>
          <p class="form-hint">Domain ini otomatis dihubungkan oleh Caddy tanpa perlu mengedit file hosts Windows.</p>
        </div>

        <div class="form-group">
          <label for="site-php" class="form-label">Versi PHP</label>
          <select id="site-php" v-model="selectedPhp" class="input">
            <option v-for="ver in installedPhp" :key="ver" :value="ver">
              PHP {{ ver }} {{ ver === state.snapshot?.default_php ? '(Default)' : '' }}
            </option>
          </select>
          <p v-if="installedPhp.length === 0" class="field-error">
            Belum ada versi PHP yang terpasang! Pasang versi PHP di tab Pengaturan terlebih dahulu.
          </p>
        </div>
      </div>

      <!-- Step 3: Confirmation -->
      <div v-else-if="step === 3 && detection" class="wizard-step">
        <p class="step-desc">
          Periksa ringkasan konfigurasi sebelum situs didaftarkan dan dijalankan:
        </p>

        <div class="summary-table">
          <div class="summary-row">
            <span class="summary-key">Domain</span>
            <span class="summary-val highlight">{{ domainSlug }}.{{ tld }}</span>
          </div>
          <div class="summary-row">
            <span class="summary-key">Framework</span>
            <span class="summary-val">{{ detection.label }}</span>
          </div>
          <div class="summary-row">
            <span class="summary-key">Versi PHP</span>
            <span class="summary-val">PHP {{ selectedPhp }}</span>
          </div>
          <div class="summary-row">
            <span class="summary-key">Folder Project</span>
            <span class="summary-val code">{{ detection.project_root }}</span>
          </div>
          <div class="summary-row">
            <span class="summary-key">Document Root</span>
            <span class="summary-val code">{{ detection.document_root }}</span>
          </div>
        </div>
      </div>

      <!-- Wizard Footer -->
      <div class="wizard-foot">
        <button
          v-if="step > 1"
          class="btn btn-quiet"
          type="button"
          @click="prevStep"
        >
          Kembali
        </button>
        <span v-else></span>

        <div class="actions-right">
          <button class="btn btn-quiet" type="button" @click="cancel">Batal</button>

          <button
            v-if="step < 3"
            class="btn btn-primary"
            type="button"
            :disabled="checking || (step === 1 && !projectPath.trim())"
            @click="nextStep"
          >
            Lanjut
          </button>

          <button
            v-else
            class="btn btn-primary"
            type="button"
            :disabled="state.busy === 'add-site' || !selectedPhp"
            @click="submit"
          >
            {{ state.busy === 'add-site' ? 'Menyimpan...' : 'Simpan & Aktifkan' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wizard-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 24px;
}

.wizard-card {
  width: 100%;
  max-width: 540px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  box-shadow: 0 16px 36px rgba(0, 0, 0, 0.3);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.wizard-head {
  padding: 14px 20px;
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.head-title {
  display: flex;
  align-items: baseline;
  gap: 12px;
}

.head-title h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 650;
  color: var(--fg);
}

.step-indicator {
  font-size: 11px;
  color: var(--muted);
}

.btn-close {
  background: none;
  border: none;
  color: var(--muted);
  font-size: 14px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
}

.btn-close:hover {
  background: var(--line);
  color: var(--fg);
}

.wizard-alert {
  margin: 14px 20px 0;
  padding: 8px 12px;
  background: rgba(185, 28, 28, 0.1);
  color: var(--bad);
  border: 1px solid rgba(185, 28, 28, 0.25);
  border-radius: 4px;
  font-size: 12px;
}

.wizard-step {
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.step-desc {
  margin: 0;
  font-size: 13px;
  color: var(--muted);
  line-height: 1.45;
}

.folder-picker-group {
  display: flex;
  gap: 8px;
}

.input-path {
  flex: 1;
  height: 34px;
  padding: 0 12px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--fg);
  font-size: 13px;
  font-family: var(--font-mono);
}

.checking-box {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--muted);
  padding: 8px 0;
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid var(--line);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.detection-badge {
  padding: 10px 12px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 12px;
}

.badge-label {
  color: var(--muted);
  font-size: 11px;
}

.badge-path {
  color: var(--muted);
  font-family: var(--font-mono);
  font-size: 11px;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.form-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--fg);
}

.input {
  height: 32px;
  padding: 0 10px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--fg);
  font-size: 13px;
}

.domain-input-wrap {
  display: flex;
  align-items: center;
}

.domain-input {
  flex: 1;
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
}

.domain-suffix {
  display: inline-flex;
  align-items: center;
  height: 32px;
  padding: 0 12px;
  background: var(--line);
  color: var(--muted);
  border: 1px solid var(--line);
  border-left: none;
  border-top-right-radius: 4px;
  border-bottom-right-radius: 4px;
  font-family: var(--font-mono);
  font-size: 12px;
}

.form-hint {
  margin: 0;
  font-size: 11px;
  color: var(--muted);
}

.field-error {
  margin: 0;
  font-size: 11px;
  color: var(--bad);
}

.summary-table {
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  padding: 10px 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.summary-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
}

.summary-key {
  color: var(--muted);
}

.summary-val {
  color: var(--fg);
  font-weight: 500;
}

.summary-val.highlight {
  color: var(--accent);
  font-weight: 600;
}

.summary-val.code {
  font-family: var(--font-mono);
  font-size: 11px;
  word-break: break-all;
}

.wizard-foot {
  padding: 14px 20px;
  background: var(--bg);
  border-top: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.actions-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.btn {
  height: 32px;
  padding: 0 14px;
  font-size: 12px;
  font-weight: 500;
  border-radius: 4px;
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--fg);
  cursor: pointer;
}

.btn-quiet {
  background: transparent;
  border-color: transparent;
  color: var(--muted);
}

.btn-quiet:hover {
  background: var(--line);
  color: var(--fg);
}

.btn-primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--on-accent);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
