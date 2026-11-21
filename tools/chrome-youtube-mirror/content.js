(() => {
  const STORAGE_KEY_ENABLED = "afterlifeYoutubeMirrorEnabled";
  const PORT_NAME = "afterlife-youtube-mirror";
  const TARGET_FRAME_INTERVAL_MS = 33;
  const PAUSED_POLL_INTERVAL_MS = 1000;
  const JPEG_QUALITY = 0.90;
  const MIN_DIM = 16;
  const DEFAULT_GRID_W = 1200;
  const DEFAULT_GRID_H = 754;
  const DEFAULT_MAX_W = 2560;
  const DEFAULT_MAX_H = 1440;

  let extensionEnabled = true;
  let serverGridW = DEFAULT_GRID_W;
  let serverGridH = DEFAULT_GRID_H;
  let serverMaxW = DEFAULT_MAX_W;
  let serverMaxH = DEFAULT_MAX_H;
  let serverPriority = "youtube";
  let serverReachable = false;
  let lastSentWidth = 0;
  let lastSentHeight = 0;
  let lastCapturedVideo = null;
  let lastCapturedTime = -1;
  let lastSentAt = 0;
  let hasSentFrame = false;
  let inFlight = false;
  let timerId = null;
  let backoffMs = 1000;
  let nextAllowedAt = 0;

  let canvas = null;
  let ctx = null;
  let canvasW = 0;
  let canvasH = 0;
  let boundVideos = new WeakSet();
  let tornDown = false;

  let port = null;
  let nextReqId = 1;
  const pendingRequests = new Map();

  function isExtensionContextValid() {
    if (tornDown) {
      return false;
    }
    try {
      return Boolean(typeof chrome !== "undefined" && chrome.runtime && chrome.runtime.id);
    } catch {
      return false;
    }
  }

  function rejectAllPending() {
    for (const resolve of pendingRequests.values()) {
      resolve({ ok: false });
    }
    pendingRequests.clear();
  }

  function ensurePort() {
    if (!isExtensionContextValid()) {
      teardown();
      return null;
    }
    if (port) {
      return port;
    }
    try {
      const p = chrome.runtime.connect({ name: PORT_NAME });
      p.onMessage.addListener((msg) => {
        if (!msg || typeof msg.reqId !== "number") {
          return;
        }
        const resolve = pendingRequests.get(msg.reqId);
        if (resolve) {
          pendingRequests.delete(msg.reqId);
          resolve(msg);
        }
      });
      p.onDisconnect.addListener(() => {
        if (port === p) {
          port = null;
        }
        rejectAllPending();
      });
      port = p;
      return port;
    } catch {
      teardown();
      return null;
    }
  }

  function sendPortRequest(payload) {
    return new Promise((resolve) => {
      const p = ensurePort();
      if (!p) {
        resolve({ ok: false });
        return;
      }
      const reqId = nextReqId;
      nextReqId += 1;
      pendingRequests.set(reqId, resolve);
      try {
        p.postMessage({ ...payload, reqId });
      } catch {
        pendingRequests.delete(reqId);
        port = null;
        resolve({ ok: false });
      }
    });
  }

  function teardown() {
    if (tornDown) {
      return;
    }
    tornDown = true;
    clearScheduledTick();
    rejectAllPending();
    if (port) {
      try {
        port.disconnect();
      } catch {
      }
      port = null;
    }
    observer.disconnect();
    window.removeEventListener("yt-navigate-finish", scanVideos);
    window.removeEventListener("pagehide", onPageHide);
  }

  function clampEven(value, minVal, maxVal) {
    const clamped = Math.max(minVal, Math.min(maxVal, Math.round(value)));
    const even = clamped - (clamped % 2);
    return Math.max(minVal, even);
  }

  function computeCaptureDimensions(videoW, videoH) {
    if (!videoW || !videoH || videoW < MIN_DIM || videoH < MIN_DIM) {
      return null;
    }
    const gridW = Math.max(MIN_DIM, serverGridW || DEFAULT_GRID_W);
    const gridH = Math.max(MIN_DIM, serverGridH || DEFAULT_GRID_H);
    const maxW = Math.max(MIN_DIM, Math.min(DEFAULT_MAX_W, serverMaxW || DEFAULT_MAX_W));
    const maxH = Math.max(MIN_DIM, Math.min(DEFAULT_MAX_H, serverMaxH || DEFAULT_MAX_H));

    const videoAspect = videoW / videoH;
    const gridAspect = gridW / gridH;

    let needW;
    let needH;
    if (videoAspect > gridAspect) {
      needW = gridW;
      needH = gridW / videoAspect;
    } else {
      needH = gridH;
      needW = gridH * videoAspect;
    }

    const scale = Math.min(1, videoW / needW, videoH / needH, maxW / needW, maxH / needH);
    const targetW = clampEven(needW * scale, MIN_DIM, maxW);
    const targetH = clampEven(needH * scale, MIN_DIM, maxH);
    return { width: targetW, height: targetH };
  }

  function ensureCanvas(width, height) {
    if (!canvas) {
      canvas = document.createElement("canvas");
    }
    if (canvasW !== width || canvasH !== height) {
      canvas.width = width;
      canvas.height = height;
      canvasW = width;
      canvasH = height;
      ctx = canvas.getContext("2d", { alpha: false });
    }
    return ctx;
  }

  function findPlayingVideo() {
    const videos = Array.from(document.querySelectorAll("video"));
    let best = null;
    let bestScore = -1;
    for (const v of videos) {
      if (v.paused || v.ended || v.readyState < 2) {
        continue;
      }
      if (v.videoWidth < MIN_DIM || v.videoHeight < MIN_DIM) {
        continue;
      }
      const rect = v.getBoundingClientRect();
      const area = Math.max(0, rect.width) * Math.max(0, rect.height);
      const isMain = v.classList.contains("html5-main-video") ? 1e9 : 0;
      const score = isMain + area + v.videoWidth * v.videoHeight * 0.01;
      if (score > bestScore) {
        bestScore = score;
        best = v;
      }
    }
    return best;
  }

  function applyServerMetadata(data) {
    if (!data || typeof data !== "object") {
      return;
    }
    if (Number.isFinite(data.grid_w) && data.grid_w >= MIN_DIM) {
      serverGridW = Math.round(data.grid_w);
    }
    if (Number.isFinite(data.grid_h) && data.grid_h >= MIN_DIM) {
      serverGridH = Math.round(data.grid_h);
    }
    if (Number.isFinite(data.max_width) && data.max_width >= MIN_DIM) {
      serverMaxW = Math.round(data.max_width);
    }
    if (Number.isFinite(data.max_height) && data.max_height >= MIN_DIM) {
      serverMaxH = Math.round(data.max_height);
    }
    if (typeof data.priority === "string") {
      serverPriority = data.priority;
    }
    serverReachable = true;
    backoffMs = 1000;
  }

  async function probeStatus() {
    const result = await sendPortRequest({ type: "STATUS" });
    if (!result || !result.ok) {
      serverReachable = false;
      return false;
    }
    applyServerMetadata(result.data);
    return true;
  }

  function sendStop() {
    if (!hasSentFrame) {
      return;
    }
    hasSentFrame = false;
    lastSentWidth = 0;
    lastSentHeight = 0;
    lastCapturedVideo = null;
    lastCapturedTime = -1;
    lastSentAt = 0;
    const p = ensurePort();
    if (!p) {
      return;
    }
    try {
      p.postMessage({ type: "STOP" });
    } catch {
      port = null;
    }
  }

  function clearScheduledTick() {
    if (timerId !== null) {
      clearTimeout(timerId);
      timerId = null;
    }
  }

  function scheduleNext(delayMs) {
    clearScheduledTick();
    timerId = setTimeout(tick, Math.max(8, delayMs));
  }

  async function tick() {
    timerId = null;
    if (!isExtensionContextValid()) {
      teardown();
      return;
    }
    if (!extensionEnabled) {
      sendStop();
      return;
    }

    const video = findPlayingVideo();
    if (!video) {
      sendStop();
      return;
    }

    const now = performance.now();
    if (now < nextAllowedAt) {
      scheduleNext(nextAllowedAt - now);
      return;
    }

    if (inFlight) {
      scheduleNext(TARGET_FRAME_INTERVAL_MS);
      return;
    }

    inFlight = true;
    const startedAt = performance.now();
    try {
      if (!serverReachable || serverPriority !== "youtube") {
        const ok = await probeStatus();
        if (!ok) {
          nextAllowedAt = performance.now() + backoffMs;
          backoffMs = Math.min(5000, backoffMs * 2);
          return;
        }
        if (serverPriority !== "youtube") {
          nextAllowedAt = performance.now() + PAUSED_POLL_INTERVAL_MS;
          return;
        }
      }

      const dims = computeCaptureDimensions(video.videoWidth, video.videoHeight);
      if (!dims) {
        return;
      }

      if (
        hasSentFrame &&
        video === lastCapturedVideo &&
        video.currentTime === lastCapturedTime &&
        dims.width === lastSentWidth &&
        dims.height === lastSentHeight &&
        startedAt - lastSentAt < 600
      ) {
        return;
      }

      const context = ensureCanvas(dims.width, dims.height);
      if (!context) {
        return;
      }

      const capturedTime = video.currentTime;
      let dataUrl;
      try {
        context.drawImage(video, 0, 0, dims.width, dims.height);
        dataUrl = canvas.toDataURL("image/jpeg", JPEG_QUALITY);
      } catch {
        nextAllowedAt = performance.now() + PAUSED_POLL_INTERVAL_MS;
        return;
      }

      if (!isExtensionContextValid()) {
        teardown();
        return;
      }

      const result = await sendPortRequest({ type: "FRAME", dataUrl });
      if (!result || !result.ok) {
        serverReachable = false;
        nextAllowedAt = performance.now() + backoffMs;
        backoffMs = Math.min(5000, backoffMs * 2);
        return;
      }

      applyServerMetadata(result.data);
      if (result.data && result.data.accepted) {
        hasSentFrame = true;
        lastCapturedVideo = video;
        lastCapturedTime = capturedTime;
        lastSentAt = performance.now();
        lastSentWidth = dims.width;
        lastSentHeight = dims.height;
      }
      if (serverPriority !== "youtube") {
        nextAllowedAt = performance.now() + PAUSED_POLL_INTERVAL_MS;
      }
    } catch {
      serverReachable = false;
      nextAllowedAt = performance.now() + backoffMs;
      backoffMs = Math.min(5000, backoffMs * 2);
    } finally {
      inFlight = false;
      if (!tornDown && isExtensionContextValid() && extensionEnabled && findPlayingVideo()) {
        const elapsed = performance.now() - startedAt;
        const waitMs = Math.max(TARGET_FRAME_INTERVAL_MS - elapsed, 8);
        const targetDelay = Math.max(waitMs, nextAllowedAt - performance.now());
        scheduleNext(targetDelay);
      }
    }
  }

  function wakeLoop() {
    if (!isExtensionContextValid()) {
      teardown();
      return;
    }
    if (!extensionEnabled) {
      return;
    }
    if (findPlayingVideo()) {
      nextAllowedAt = 0;
      if (!inFlight && timerId === null) {
        scheduleNext(0);
      }
    } else {
      clearScheduledTick();
      sendStop();
    }
  }

  function bindVideoElement(video) {
    if (boundVideos.has(video)) {
      return;
    }
    boundVideos.add(video);
    for (const ev of ["play", "playing", "loadeddata", "resize", "seeked"]) {
      video.addEventListener(ev, wakeLoop, { passive: true });
    }
    for (const ev of ["pause", "ended", "emptied"]) {
      video.addEventListener(
        ev,
        () => {
          if (!isExtensionContextValid()) {
            teardown();
            return;
          }
          if (!findPlayingVideo()) {
            clearScheduledTick();
            sendStop();
          }
        },
        { passive: true },
      );
    }
  }

  function scanVideos() {
    if (!isExtensionContextValid()) {
      teardown();
      return;
    }
    for (const v of document.querySelectorAll("video")) {
      bindVideoElement(v);
    }
    wakeLoop();
  }

  function onPageHide() {
    clearScheduledTick();
    sendStop();
  }

  const observer = new MutationObserver(() => {
    scanVideos();
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });

  window.addEventListener("yt-navigate-finish", scanVideos, { passive: true });
  window.addEventListener("pagehide", onPageHide, { passive: true });

  if (typeof chrome !== "undefined" && chrome.storage && chrome.storage.local) {
    chrome.storage.local.get({ [STORAGE_KEY_ENABLED]: true }, (items) => {
      extensionEnabled = items[STORAGE_KEY_ENABLED] !== false;
      scanVideos();
    });

    chrome.storage.onChanged.addListener((changes, areaName) => {
      if (!isExtensionContextValid()) {
        teardown();
        return;
      }
      if (areaName !== "local" || !changes[STORAGE_KEY_ENABLED]) {
        return;
      }
      extensionEnabled = changes[STORAGE_KEY_ENABLED].newValue !== false;
      if (!extensionEnabled) {
        clearScheduledTick();
        sendStop();
      } else {
        wakeLoop();
      }
    });
  } else {
    scanVideos();
  }

  if (typeof chrome !== "undefined" && chrome.runtime && chrome.runtime.onMessage) {
    chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
      if (msg && msg.type === "GET_MIRROR_STATE") {
        const playing = Boolean(findPlayingVideo());
        sendResponse({
          extensionEnabled,
          playing,
          serverReachable,
          serverPriority,
          lastSentWidth,
          lastSentHeight,
          serverGridW,
          serverGridH,
        });
      }
    });
  }
})();
