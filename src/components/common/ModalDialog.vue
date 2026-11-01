<script setup lang="ts">
import { computed } from "vue";
import { state } from "../../store/devku";

const active = computed(() => state.activeDialog);

function cancel() {
  if (!active.value) return;
  active.value.resolve(null);
}

function choose(choiceId: string) {
  if (!active.value) return;
  active.value.resolve({
    choice: choiceId,
    values: { ...active.value.values },
    checked: { ...active.value.checked },
  });
}

function onBackdropClick(event: MouseEvent) {
  // Explicitly do nothing on backdrop click to prevent accidental dismissal!
  event.stopPropagation();
}

function onKeyDown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    cancel();
  }
}
</script>

<template>
  <div v-if="active" class="modal-backdrop" @click="onBackdropClick" @keydown="onKeyDown">
    <div
      class="modal-card"
      :class="{
        'modal-tone-danger': active.spec.tone === 'danger',
        'modal-tone-warning': active.spec.tone === 'warning',
      }"
      role="dialog"
      aria-modal="true"
      @click.stop
    >
      <div class="modal-head">
        <h2 class="modal-title">{{ active.spec.title }}</h2>
      </div>

      <div class="modal-body">
        <p v-if="active.spec.body" class="modal-desc">{{ active.spec.body }}</p>

        <!-- Rows of data (e.g. details being deleted or confirmed) -->
        <div v-if="active.spec.rows?.length" class="modal-rows">
          <div v-for="row in active.spec.rows" :key="row.label" class="modal-row">
            <span class="row-label">{{ row.label }}</span>
            <span class="row-value">{{ row.value }}</span>
          </div>
        </div>

        <!-- Input Fields -->
        <div v-if="active.spec.fields?.length" class="modal-fields">
          <div v-for="field in active.spec.fields" :key="field.id" class="modal-field">
            <label :for="`field-${field.id}`" class="field-label">{{ field.label }}</label>
            <input
              :id="`field-${field.id}`"
              v-model="active.values[field.id]"
              :type="field.type ?? 'text'"
              :placeholder="field.placeholder"
              class="field-input"
              :class="{ 'field-input-mono': field.mono }"
              autofocus
            />
            <p v-if="field.hint" class="field-hint">{{ field.hint }}</p>
          </div>
        </div>

        <!-- Checkboxes -->
        <div v-if="active.spec.checkboxes?.length" class="modal-checkboxes">
          <template v-for="cb in active.spec.checkboxes" :key="cb.id">
            <div
              v-if="!cb.requires || active.checked[cb.requires]"
              class="modal-checkbox-row"
            >
              <label class="checkbox-label">
                <input
                  v-model="active.checked[cb.id]"
                  type="checkbox"
                  class="checkbox-input"
                />
                <span class="checkbox-text">{{ cb.label }}</span>
              </label>
              <p v-if="cb.hint" class="checkbox-hint">{{ cb.hint }}</p>
            </div>
          </template>
        </div>
      </div>

      <div class="modal-foot">
        <button
          class="btn btn-quiet"
          type="button"
          @click="cancel"
        >
          {{ active.spec.cancelLabel ?? "Batal" }}
        </button>

        <div class="modal-choices">
          <button
            v-for="choice in active.spec.choices"
            :key="choice.id"
            class="btn"
            :class="{
              'btn-primary': choice.variant === 'primary' || !choice.variant,
              'btn-danger': choice.variant === 'danger',
            }"
            type="button"
            @click="choose(choice.id)"
          >
            {{ choice.label }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 20px;
}

.modal-card {
  width: 100%;
  max-width: 460px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  box-shadow: 0 10px 25px rgba(0, 0, 0, 0.25);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.modal-head {
  padding: 14px 18px 10px;
  border-bottom: 1px solid var(--line);
}

.modal-title {
  margin: 0;
  font-size: 14px;
  font-weight: 650;
  color: var(--fg);
}

.modal-body {
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  max-height: 70vh;
  overflow-y: auto;
}

.modal-desc {
  margin: 0;
  font-size: 13px;
  color: var(--muted);
  line-height: 1.45;
}

.modal-rows {
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: var(--bg);
  padding: 10px 12px;
  border-radius: 4px;
  border: 1px solid var(--line);
}

.modal-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
}

.row-label {
  color: var(--muted);
}

.row-value {
  color: var(--fg);
  font-family: var(--font-mono);
  word-break: break-all;
}

.modal-fields {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.modal-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.field-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--fg);
}

.field-input {
  height: 32px;
  padding: 0 10px;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--fg);
  font-size: 13px;
  font-family: inherit;
}

.field-input-mono {
  font-family: var(--font-mono);
}

.field-input:focus {
  outline: 2px solid var(--accent);
  outline-offset: -1px;
}

.field-hint {
  margin: 0;
  font-size: 11px;
  color: var(--muted);
}

.modal-checkboxes {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.modal-checkbox-row {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  font-size: 13px;
}

.checkbox-input {
  cursor: pointer;
}

.checkbox-hint {
  margin: 0 0 0 22px;
  font-size: 11px;
  color: var(--muted);
}

.modal-foot {
  padding: 12px 18px;
  background: var(--bg);
  border-top: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.modal-choices {
  display: flex;
  align-items: center;
  gap: 8px;
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

.btn-danger {
  background: var(--bad);
  border-color: var(--bad);
  color: #ffffff;
}
</style>
