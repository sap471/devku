<script setup lang="ts">
import { onMounted } from "vue";
import { state, load } from "./store/devku";
import AppTitlebar from "./components/AppTitlebar.vue";
import NoticeBanner from "./components/common/NoticeBanner.vue";
import ModalDialog from "./components/common/ModalDialog.vue";
import SiteWizardModal from "./components/wizard/SiteWizardModal.vue";
import SiteList from "./components/sites/SiteList.vue";
import SiteDetail from "./components/sites/SiteDetail.vue";
import SettingsView from "./components/settings/SettingsView.vue";
import TerminalDrawer from "./components/terminal/TerminalDrawer.vue";

onMounted(() => {
  void load();
});
</script>

<template>
  <div class="devku-window">
    <!-- Custom Frameless Titlebar -->
    <AppTitlebar />

    <!-- Notice & Download Progress Banners -->
    <NoticeBanner />

    <!-- Main Workspace -->
    <div class="workspace-area">
      <!-- Tab 1: Sites Dashboard -->
      <div v-show="state.activeTab === 'sites'" class="sites-view">
        <SiteList />
        <SiteDetail />
      </div>

      <!-- Tab 2: Settings Page -->
      <div v-show="state.activeTab === 'settings'" class="settings-view">
        <SettingsView />
      </div>
    </div>

    <!-- Integrated Terminal Drawer -->
    <TerminalDrawer />

    <!-- Modals -->
    <SiteWizardModal />
    <ModalDialog />
  </div>
</template>

<style scoped>
.devku-window {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  background: var(--bg);
  color: var(--fg);
  overflow: hidden;
  user-select: none;
}

.workspace-area {
  flex: 1;
  display: flex;
  overflow: hidden;
  position: relative;
}

.sites-view {
  flex: 1;
  display: flex;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.settings-view {
  flex: 1;
  display: flex;
  width: 100%;
  height: 100%;
  overflow: hidden;
}
</style>
