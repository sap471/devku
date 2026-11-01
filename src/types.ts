export type Status = "starting" | "ready" | "backoff" | "failed" | "stopped";

export interface SiteView {
  id: string;
  name: string;
  domain: string;
  php: string;
  framework: string;
  enabled: boolean;
  path: string;
  root: string;
  port: number | null;
  status: Status;
  detail: string | null;
}

export interface DownloadsView {
  files: number;
  bytes: number;
}

export interface PathStatus {
  dir: string;
  registered: boolean;
  shim_version: string | null;
  broken: boolean;
}

export interface ComposerGlobal {
  dir: string;
  registered: boolean;
  exists: boolean;
  tools: string[];
}

export interface ServiceView {
  name: string;
  status: Status;
  detail: string | null;
}

export interface RuntimeInfo {
  name: string;
  version: string | null;
  source: string;
  path: string | null;
}

export interface OpenTarget {
  id: string;
  label: string;
  available: boolean;
}

export interface PhpCatalogEntry {
  branch: string;
  version: string;
  filename: string;
  sha256: string;
  size: string;
  installed: boolean;
  local: boolean;
}

export interface PhpCatalogView {
  entries: PhpCatalogEntry[];
  warning: string | null;
}

export interface DownloadProgress {
  target: string;
  version: string;
  received: number;
  total: number | null;
  stage: string;
}

export interface ImportReport {
  imported: number;
  skipped: string[];
  disabled: string[];
  active_php_cleared: string | null;
}

export interface ImportResponse {
  report: ImportReport;
  snapshot: Snapshot;
}

export interface PhpRemoval {
  version: string | null;
  disabled_sites: string[];
  default_php: string;
  composer_moved: boolean;
  removed_all: boolean;
}

export interface PhpRemovalResponse {
  removal: PhpRemoval;
  snapshot: Snapshot;
}

export interface DatabaseView {
  engine: string;
  label: string;
  version: string;
  installed: boolean;
  initialized: boolean;
  port: number;
  autostart: boolean;
  status: Status;
  detail: string | null;
  data_path: string;
  bin_path: string;
  has_password: boolean;
}

export interface DatabaseRemoval {
  removed_data: boolean;
}

export interface DatabaseRemovalResponse {
  removal: DatabaseRemoval;
  backup: string | null;
  snapshot: Snapshot;
}

export interface SiteCommand {
  id: string;
  label: string;
  command: string;
}

export interface Detection {
  framework: string;
  label: string;
  document_root: string;
  project_root: string;
}

export interface Snapshot {
  tld: string;
  running: boolean;
  caddy_installed: boolean;
  https_trusted: boolean;
  onboarded: boolean;
  installed_php: string[];
  default_php: string;
  php_path: PathStatus;
  composer_installed: boolean;
  composer_version: string | null;
  composer_php: string | null;
  composer_php_missing: boolean;
  composer_global: ComposerGlobal;
  config_path: string;
  home_path: string;
  start_at_login: boolean;
  sites: SiteView[];
  services: ServiceView[];
  database: DatabaseView;
  downloads: DownloadsView;
}

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
  tone?: "warning" | "danger";
  cancelLabel?: string;
}

export interface DialogOutcome {
  choice: string;
  values: Record<string, string>;
  checked: Record<string, boolean>;
}
