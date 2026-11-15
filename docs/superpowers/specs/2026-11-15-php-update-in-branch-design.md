# Design: update an installed PHP to a newer release in the same branch

Date: 2026-11-15
Size: small change plus one new backend capability

## Intent and why

The PHP settings pane offers a version grid where an installed build is a dead
end: it shows a "Terpasang" badge and nothing else. A newer release in the same
branch (`8.3.36` to `8.3.37`) appears as a separate card with a "Pasang"
button, so the only way to move is install, switch every site by hand, then
remember to delete the old folder. The user asked for the obvious thing
instead: for a version that has a newer release available, offer "update",
not "install again".

## What the code does today

The version string is the identity everywhere. `engines/php/8.3.36` is the
install, `config.sites[].php` is `"8.3.36"`, the pool service is `php-8.3.36`,
and `CaddyBackend` writes the site's `php_fastcgi` target from the port that
`ports::allocate` assigns per version in use. There is no manifest and no
recorded install metadata, so "is this build stale" can only be answered by
comparing the folder names against `releases.json`.

`PhpManager::extract` merges extracted files into the destination folder and
never clears it, and `PhpManager::ensure_ini` returns early when a `php.ini`
already exists. Those two facts decide the design: a same-folder "update" would
leave stale DLLs from the old build next to the new ones and would keep an ini
written for the old version, and it would run under a live pool while it did.

The user's own 8.3.36 ini has `extension=mysqli` enabled on top of the twelve
defaults. A new 8.3.37 folder starts from a fresh template, so without carrying
those edits over, sites would break quietly after an update.

## Goals

- An installed version whose branch has a newer release in `releases.json`
  offers an update to that release, from the version grid and from the
  installed-versions list.
- The update is one step: install the new folder, move every site that used the
  old version, keep the pool port, delete the old folder.
- User edits in the old `php.ini` (enabled extensions, changed directives)
  survive into the new version's ini.
- Every other reference to the old version moves with it: `php.active`, the
  PATH shim, and the Composer PHP pointer.
- The user sees what will happen before it happens.

## Non-goals

- Cross-branch updates. `8.3.x` to `8.5.x` stays a manual "Pasang" plus site
  switches: the risk is real (dropped extensions, code that is not compatible
  with the newer branch), and it should not be one click.
- Automatic updates, background checks, or an "updates available" badge in the
  app shell. The catalog is refreshed by hand today and stays that way.
- Updating anything other than PHP.
- Keeping the old version as a fallback. The user chose repoint-and-remove.
  A downgrade stays possible by installing the old version again.
- Carrying `php.ini-development` / `php.ini-production` over; the new zip
  supplies its own.

## Approaches considered

1. **One `update_php` command (chosen).** The backend receives `from` and `to`,
   resolves `to` in `releases.json`, downloads and verifies the archive, and
   only then touches anything on disk: stop the old pool, extract the new
   folder, write the new ini from the new template plus the old file's
   user-visible lines, repoint sites and the other references, save config,
   delete the old folder, `refresh()` so the pool comes back on the same port.
   Every prerequisite happens before the first mutation, so a failure before
   extraction leaves the machine exactly as it was, and the failure after it
   leaves a valid new install plus one clear error.
2. **Compose the existing commands from the frontend.** `install_php`, then
   `set_php` per site, then `remove_php`. Rejected: each step is its own
   command with its own error reporting, so a failure in the middle leaves
   sites on the new version with the old one still installed and nobody to say
   so; the ini has to be copied by new code either way; and a frontend loop of
   `invoke` calls is more code that no test can reach.
3. **Update in place, same folder.** Extract `8.3.37` into `engines/php/8.3.36`.
   Rejected: stale files survive the merge, the config and service name do not
   change so the user cannot tell whether anything happened, and a failed
   download during extraction leaves a folder holding fragments of two builds.

## Decided

### Backend

- Version ordering carries the build suffix. `version_key` (`php.rs`) already
  reads `8.3.36-1` as `[8, 3, 36, 1]`, which is greater than `[8, 3, 36]`, so it
  is the comparator for "is `to` newer than `from`". `download::rank` ignores
  the suffix on purpose and stays as it is; it only orders the catalog display.
- `download::newest_in_branch(releases, branch) -> Option<&PhpRelease>` answers
  "which release is the newest in this branch", keyed on the release's own
  `branch` field (the `releases.json` key), not on `branch_of`.
- Any installed version is updatable, including a folder that is not in
  `releases.json` (a `local: true` catalog card). A hand-extracted `8.3.5` with
  `8.3.37` in the branch gets the same offer as a catalog install; only `from`
  has to be installed, `to` always comes from `releases.json`.
- `Session::update_php` refuses when `to` is already installed, so an update can
  never merge into an existing folder next to the one being replaced.
- `PhpManager::ini_for_update(old_ini, template_text)` in `php.rs`: builds the
  new ini from the raw template file (`php.ini-development`, else
  `php.ini-production`) with the same rewrite `ensure_ini` applies (uncomment
  `extension_dir`, uncomment template-commented defaults), then reapplies the
  old file. The comparison runs against that rewritten text, so "the template"
  always means what `ensure_ini` would have written:

  - An active line (`key = value`, `extension=x`, `zend_extension=x`) whose key
    has no active counterpart in the rewritten template is carried.
  - An active line whose key is active in both and whose value differs is
    carried.
  - A key the template activates and the old file no longer has (the user
    removed a default extension, for example) is left commented out in the new
    file, so the removal survives.
  - Commented lines are never carried, and sections the user added are carried
    whole.
  - `extension=` / `zend_extension=` lines whose DLL is missing from the new
    `ext/` are dropped and reported in `PhpUpdate.extensions_dropped`.
- `Session::update_php(&mut self, from: &str, to: &str, spelling) -> Result<PhpUpdate>`:
  - Refuses when `from` is not installed, when `to` is already installed, when
    `to` is not newer than `from` by `version_key`, or when the two are not in
    the same branch.
  - Collects the affected sites, whether `php.active` is `from`, whether the
    PATH shim points at `from`, and whether the Composer pointer does.
  - Stops the old pool (`Runtime::stop_version`), extracts the new folder,
    writes the new ini, moves the sites and the three references, saves config,
    deletes the old folder, then `refresh()`.
  - Nothing is mutated before the archive is downloaded and verified. If a step
    after extraction fails (the ini write, the config save, a repoint), the new
    folder is deleted again so the machine is back where it started, and the
    error names the step that failed. Cleanup is best effort: if the delete
    itself fails because a handle is still open, the message says the leftover
    folder can be removed with the Hapus button.
- `Runtime::stop_version(version)` in `runtime.rs`: stops one pool without
  respecting the config, since during an update the config still names the old
  version. Synchronous, like `stop_unused`. It removes the supervisor from
  `self.php` and leaves `self.ports` alone, which is what keeps the old port
  taken: `refresh()` then starts the new version through `ensure_version`, and
  `ports::first_free` sees the old port still occupied by the dead version, so
  the site keeps its port and Caddy only needs the reload `refresh()` ends with.

- New Tauri commands, registered in the invoke handler next to `install_php`:
  - `php_update_check() -> Vec<PhpUpdateOption>`, built from the installed
    folder list plus `releases.json`. It needs no session state, so it takes
    the engine root (`State<'_, Paths>` or the app's resolved home) rather than
    the session mutex, and it must not take the download slot.
  - `update_php(from, to) -> PhpUpdateResponse { snapshot, update }`, which
    takes the download `Slot` and wraps its download the way
    `install_php_inner` does, so `cancel_download` stops it and a second
    concurrent download is refused.
  - Progress events use `report(app, "php", to, ...)`: the version string in
    the event is the target version, because that is what the download banner
    prints.

### Frontend

- `types.ts`:
  ```ts
  interface PhpUpdateOption { branch: string; from: string; to: string; size: string }
  interface PhpUpdate {
    from: string;
    to: string;
    /** Site names, matching PhpRemoval.disabled_sites. */
    sites: string[];
    moved_default: boolean;
    moved_path: boolean;
    moved_composer: boolean;
    default_php: string;
    extensions_dropped: string[];
  }
  ```
  `size` is a string ("33.66MB"), the same field the catalog card already
  prints.
- Store: `phpUpdateOptions` state, `loadPhpUpdateOptions()`, and
  `updatePhp(from, to)` which asks for confirmation first, then invokes and
  applies the snapshot and an info notice.
- `SettingsView.vue`: a "Perbarui" button on the grid card whose `to` is an
  installed version, and beside the version row in "Versi Terpasang" when that
  version has an option.

### Copy

- The confirmation is a `promptDialog` with `tone: "warning"`, rows for the
  current version, the target version, the sites that move, and the default,
  PATH, and Composer moves. Body states plainly that the old folder is deleted
  and that this cannot be undone from within the app.
- The result notice names the versions, the number of sites moved, and any
  extension that was dropped.

## Left to the builder

- Test placement follows the existing inline patterns in `php.rs`,
  `commands.rs`, and `session.rs`: fake `engines/php/<version>/php-cgi.exe`
  folders with no network, and a `PhpRelease` built by hand as in the
  `build_catalog` tests. The ini carry-over is tested directly against
  `ini_for_update` with two template strings.
- Whether `Runtime::stop_version` is needed at all if the update runs while the
  runtime is down is a builder detail; the guard should be cheap and harmless.
- Refresh cadence: the catalog load already refreshes on mount and on the
  "Segarkan Katalog" button. Join the update check to that same load so the two
  cannot disagree, and reuse the existing loading and error state rather than
  adding a second set of four states.
- Changelog entry per repo convention, commit hash appended after commit.
