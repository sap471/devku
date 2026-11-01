# DevKu Domain Model

DevKu is a lightweight local PHP development environment and control panel for Windows built with Tauri v2 and Vue 3.

## Language

### Workspace & Services

**Site**:
A configured local web project mapping a domain name to a local directory and a designated PHP runtime version.
_Avoid_: App, project, vhost

**Runtime**:
An execution environment consisting of an installed PHP binary version with associated extensions and configuration.
_Avoid_: Engine, interpreter, environment

**Service**:
A managed background daemon controlled by DevKu, specifically the Caddy web server or MariaDB database server.
_Avoid_: Process, daemon, server

**Snapshot**:
The aggregated live system state returned from the Tauri backend, including running services, sites, installed PHP versions, and port statuses.
_Avoid_: State dump, config dump

### UI Layout

**Titlebar**:
The customized top bar handling window drag and window controls (minimize, maximize, close) alongside primary view navigation tabs and global status.
_Avoid_: Header, app bar, window header

**Site Drawer / Wizard**:
A focused modal dialog for creating a new Site with framework auto-detection and directory selection, with a backdrop that prevents accidental dismissal.
_Avoid_: Popup, form screen, creation page

**Detail Pane**:
The panel presenting detailed context, logs, and quick actions for the currently active Site.
_Avoid_: Inspector, side panel

**Settings Page**:
A dedicated multi-tab view organizing configurations across PHP, Database, Web Server, and System categories.
_Avoid_: Config panel, options screen

**Terminal Drawer**:
A bottom-docked collapsible pane hosting the integrated xterm terminal session.
_Avoid_: Console pane, bottom sheet
