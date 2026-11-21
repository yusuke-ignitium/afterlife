const BASE_URL = "http://127.0.0.1:47374/v1/youtube-mirror";

let activeSenderPort = null;

async function decodeDataUrl(dataUrl) {
  if (typeof dataUrl !== "string" || !dataUrl.startsWith("data:")) {
    return null;
  }
  const resp = await fetch(dataUrl);
  const blob = await resp.blob();
  return { mime: blob.type || "image/jpeg", body: blob };
}

async function fetchStatus() {
  try {
    const resp = await fetch(`${BASE_URL}/status`, { method: "GET" });
    if (!resp.ok) {
      return { ok: false };
    }
    const data = await resp.json();
    return { ok: true, data };
  } catch {
    return { ok: false };
  }
}

async function postFrame(dataUrl) {
  const decoded = await decodeDataUrl(dataUrl);
  if (!decoded) {
    return { ok: false };
  }
  try {
    const resp = await fetch(`${BASE_URL}/frame`, {
      method: "POST",
      headers: { "Content-Type": decoded.mime },
      body: decoded.body,
    });
    if (!resp.ok) {
      return { ok: false };
    }
    const data = await resp.json();
    return { ok: true, data };
  } catch {
    return { ok: false };
  }
}

async function postStop() {
  try {
    await fetch(`${BASE_URL}/stop`, { method: "POST" });
  } catch {
  }
}

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "afterlife-youtube-mirror") {
    return;
  }

  let disconnected = false;
  port.onDisconnect.addListener(() => {
    disconnected = true;
    if (activeSenderPort === port) {
      activeSenderPort = null;
      postStop();
    }
  });

  port.onMessage.addListener(async (msg) => {
    if (!msg || typeof msg !== "object") {
      return;
    }
    if (msg.type === "STATUS") {
      const result = await fetchStatus();
      if (!disconnected) {
        try {
          port.postMessage({ type: "STATUS_RESULT", reqId: msg.reqId, ...result });
        } catch {
        }
      }
      return;
    }
    if (msg.type === "FRAME") {
      activeSenderPort = port;
      const result = await postFrame(msg.dataUrl);
      if (!disconnected) {
        try {
          port.postMessage({ type: "FRAME_RESULT", reqId: msg.reqId, ...result });
        } catch {
        }
      }
      return;
    }
    if (msg.type === "STOP") {
      if (activeSenderPort === port) {
        activeSenderPort = null;
      }
      await postStop();
    }
  });
});
