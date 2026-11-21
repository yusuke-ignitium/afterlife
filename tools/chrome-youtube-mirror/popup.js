const STATUS_URL = "http://127.0.0.1:47374/v1/youtube-mirror/status";
const STORAGE_KEY_ENABLED = "afterlifeYoutubeMirrorEnabled";

const toggleEl = document.getElementById("toggle-enabled");
const badgeEl = document.getElementById("status-badge");
const gridEl = document.getElementById("grid-size");
const frameEl = document.getElementById("frame-size");

function setBadge(text, className) {
  badgeEl.textContent = text;
  badgeEl.className = `badge ${className}`;
}

async function queryActiveTabState() {
  return new Promise((resolve) => {
    if (!chrome.tabs || !chrome.tabs.query) {
      resolve(null);
      return;
    }
    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      const tab = tabs && tabs[0];
      if (!tab || !tab.id) {
        resolve(null);
        return;
      }
      chrome.tabs.sendMessage(tab.id, { type: "GET_MIRROR_STATE" }, (resp) => {
        if (chrome.runtime.lastError) {
          resolve(null);
          return;
        }
        resolve(resp || null);
      });
    });
  });
}

async function refresh() {
  const stored = await chrome.storage.local.get({ [STORAGE_KEY_ENABLED]: true });
  const extEnabled = stored[STORAGE_KEY_ENABLED] !== false;
  toggleEl.checked = extEnabled;

  let server = null;
  try {
    const resp = await fetch(STATUS_URL, { method: "GET" });
    if (resp.ok) {
      server = await resp.json();
    }
  } catch {
  }

  const tabState = await queryActiveTabState();

  if (!server) {
    setBadge("Afterlife Offline", "badge-off");
    gridEl.textContent = "—";
    frameEl.textContent = "—";
    return;
  }

  gridEl.textContent = `${server.grid_w} × ${server.grid_h}`;
  if (tabState && tabState.lastSentWidth > 0 && tabState.lastSentHeight > 0) {
    frameEl.textContent = `${tabState.lastSentWidth} × ${tabState.lastSentHeight}`;
  } else {
    frameEl.textContent = "Idle";
  }

  if (!extEnabled) {
    setBadge("Extension Off", "badge-off");
  } else if (!server.enabled) {
    setBadge("Disabled in Afterlife", "badge-off");
  } else if (server.apple_music_active) {
    setBadge("Apple Music Priority", "badge-music");
  } else if (server.youtube_active) {
    setBadge("Mirroring Live", "badge-live");
  } else {
    setBadge("Ready", "badge-idle");
  }
}

toggleEl.addEventListener("change", async () => {
  await chrome.storage.local.set({ [STORAGE_KEY_ENABLED]: toggleEl.checked });
  setTimeout(refresh, 80);
});

refresh();
