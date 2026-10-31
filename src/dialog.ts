/// Satu dialog untuk semua aksi yang butuh perhatian: konfirmasi hapus, input
/// password, ceklis backup. Isinya disusun dari data, bukan HTML bebas dari
/// pemanggil, supaya tidak ada jalan menyisipkan markup tanpa escape.

export interface DialogChoice {
  id: string;
  label: string;
  variant?: "primary" | "danger";
}

export interface DialogCheckbox {
  id: string;
  label: string;
  hint?: string;
  checked?: boolean;
  /// Ceklis hanya tampil kalau ceklis lain sudah dicentang.
  requires?: string;
}

export interface DialogField {
  id: string;
  label: string;
  value?: string;
  placeholder?: string;
  type?: "text" | "password";
  hint?: string;
  mono?: boolean;
}

export interface DialogRow {
  label: string;
  value: string;
}

export interface DialogSpec {
  title: string;
  body?: string;
  rows?: DialogRow[];
  fields?: DialogField[];
  checkboxes?: DialogCheckbox[];
  choices: DialogChoice[];
  /// Menandai dialog yang menghapus atau mengubah data.
  tone?: "warning" | "danger";
  cancelLabel?: string;
}

export interface DialogOutcome {
  choice: string;
  values: Record<string, string>;
  checked: Record<string, boolean>;
}

const esc = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (ch) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch] ?? ch,
  );

let nextId = 0;

/// Membuka dialog dan menunggu satu pilihan. `null` berarti dibatalkan lewat
/// Escape, klik latar, atau tombol batal.
export function openDialog(spec: DialogSpec): Promise<DialogOutcome | null> {
  return new Promise((resolve) => {
    const uid = `dialog-${++nextId}`;
    const previous = document.activeElement as HTMLElement | null;

    const rows = (spec.rows ?? [])
      .map(
        (row) =>
          `<div class="dialog-row"><dt>${esc(row.label)}</dt><dd>${esc(row.value)}</dd></div>`,
      )
      .join("");

    const fields = (spec.fields ?? [])
      .map(
        (field) => `<label class="field">
          <span class="field-label">${esc(field.label)}</span>
          <input class="input${field.mono ? " input-mono" : ""}" id="${uid}-${esc(
            field.id,
          )}" data-field="${esc(field.id)}" type="${field.type ?? "text"}" value="${esc(
            field.value ?? "",
          )}" placeholder="${esc(field.placeholder ?? "")}" autocomplete="off" spellcheck="false" />
          ${field.hint ? `<span class="hint">${esc(field.hint)}</span>` : ""}
        </label>`,
      )
      .join("");

    const checks = (spec.checkboxes ?? [])
      .map((box) => {
        const gate = box.requires ? ` hidden data-requires="${esc(box.requires)}"` : "";
        return `<label class="dialog-check" data-check-row="${esc(box.id)}"${gate}>
          <input type="checkbox" data-check="${esc(box.id)}" data-default="${
            box.checked ? "true" : "false"
          }"${box.checked ? " checked" : ""} />
          <span>
            <span class="dialog-check-label">${esc(box.label)}</span>
            ${box.hint ? `<span class="hint">${esc(box.hint)}</span>` : ""}
          </span>
        </label>`;
      })
      .join("");

    const choices = spec.choices
      .map(
        (choice) =>
          `<button class="btn${
            choice.variant === "primary"
              ? " btn-primary"
              : choice.variant === "danger"
                ? " btn-danger"
                : ""
          }" type="button" data-choice="${esc(choice.id)}">${esc(choice.label)}</button>`,
      )
      .join("");

    const backdrop = document.createElement("div");
    backdrop.className = "dialog-backdrop";
    backdrop.innerHTML = `
      <div class="dialog" role="dialog" aria-modal="true" tabindex="-1" aria-labelledby="${uid}-title"${
        spec.tone ? ` data-tone="${spec.tone}"` : ""
      }>
        <h2 class="dialog-title" id="${uid}-title">${esc(spec.title)}</h2>
        <div class="dialog-body">
          ${spec.body ? `<p class="dialog-text">${esc(spec.body)}</p>` : ""}
          ${rows ? `<dl class="dialog-rows">${rows}</dl>` : ""}
          ${fields}
          ${checks}
        </div>
        <div class="dialog-actions">
          <button class="btn btn-quiet" type="button" data-choice="cancel">${esc(
            spec.cancelLabel ?? "Batal",
          )}</button>
          ${choices}
        </div>
      </div>`;

    const dialog = backdrop.querySelector<HTMLElement>(".dialog")!;
    const focusables = () =>
      Array.from(
        dialog.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      );

    const collect = (choice: string): DialogOutcome => {
      const values: Record<string, string> = {};
      for (const input of dialog.querySelectorAll<HTMLInputElement>("[data-field]")) {
        values[input.dataset.field!] = input.value;
      }
      const checked: Record<string, boolean> = {};
      for (const input of dialog.querySelectorAll<HTMLInputElement>("[data-check]")) {
        checked[input.dataset.check!] = input.checked;
      }
      return { choice, values, checked };
    };

    const finish = (outcome: DialogOutcome | null) => {
      backdrop.remove();
      document.removeEventListener("keydown", onKey, true);
      previous?.focus();
      resolve(outcome);
    };

    const syncChecks = () => {
      for (const row of dialog.querySelectorAll<HTMLElement>("[data-requires]")) {
        const parent = row.dataset.requires!;
        const gate = dialog.querySelector<HTMLInputElement>(`[data-check="${parent}"]`);
        const own = row.querySelector<HTMLInputElement>("[data-check]");
        const hidden = !gate?.checked;
        row.hidden = hidden;
        // Ceklis bertingkat kembali ke keadaan awalnya saat induknya dicentang
        // lagi, bukan tetap mati karena sempat disembunyikan.
        if (own && own !== gate) {
          own.checked = hidden ? false : own.dataset.default === "true";
        }
      }
    };

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        finish(null);
        return;
      }
      if (event.key !== "Tab") return;

      const list = focusables();
      if (list.length === 0) return;
      const first = list[0];
      const last = list[list.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (event.shiftKey && active === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    };

    backdrop.addEventListener("click", (event) => {
      const node = event.target as HTMLElement;
      const button = node.closest<HTMLElement>("[data-choice]");
      if (button) {
        const choice = button.dataset.choice!;
        finish(choice === "cancel" ? null : collect(choice));
        return;
      }
      if (node === backdrop) finish(null);
    });

    backdrop.addEventListener("change", (event) => {
      const input = event.target as HTMLElement;
      if (input.matches("[data-check]")) syncChecks();
    });

    document.addEventListener("keydown", onKey, true);
    document.body.append(backdrop);
    syncChecks();

    const firstField = dialog.querySelector<HTMLInputElement>("[data-field]");
    const firstButton = dialog.querySelector<HTMLElement>("[data-choice]:not([data-choice='cancel'])");
    (firstField ?? firstButton ?? dialog).focus();
  });
}
