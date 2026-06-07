// Stacco — frontend logic.
//
// Uses Tauri's injected global API (`withGlobalTauri: true`) so there is no
// bundler and no `@tauri-apps/api` import. All real work happens in Rust; this
// file only reads the UI, sends commands, and reflects the polled status.

"use strict";

const TAURI = window.__TAURI__;
const invoke = TAURI?.core?.invoke;
const listen = TAURI?.event?.listen;

// Internationalization (see i18n.js, loaded first). `t` is the per-call shorthand.
const I18N = window.I18N;
const t = (key, params) => I18N.t(key, params);

const UNIT_MS = { ms: 1, s: 1000, min: 60000 };
const STATUS_POLL_MS = 120;
const SYNC_DEBOUNCE_MS = 250;
const CAPTURE_COUNTDOWN = 3;

// ---------- Element references ----------
const $ = (id) => document.getElementById(id);
const intervalValue = $("intervalValue");
const unitDropdown = setupDropdown($("intervalUnit"), () => scheduleSync());
const posX = $("posX");
const posY = $("posY");
const captureBtn = $("captureBtn");
const repeatTimes = $("repeatTimes");
const jitterInterval = $("jitterInterval");
const jitterPosition = $("jitterPosition");
const hotkeyInput = $("hotkeyInput");
const hotkeyBtn = $("hotkeyBtn");
const fixedPanel = $("fixedPanel");
const countPanel = $("countPanel");
const statusPill = $("statusPill");
const statusText = $("statusText");
const clickCount = $("clickCount");
const toggleBtn = $("toggleBtn");
const message = $("message");
const closeDialog = $("closeDialog");
const closeRemember = $("closeRemember");
const closeTray = $("closeTray");
const closeQuit = $("closeQuit");
const captureHint = $("captureHint");
const langDropdown = setupDropdown($("languageSelect"), (code) => onLanguageChange(code));

/** Mirrors the engine's running state so we only touch the DOM on transitions. */
let running = false;
let syncTimer = null;

// ---------- Segmented controls ----------
function getSeg(group) {
  const active = document.querySelector(
    `.segmented[data-group="${group}"] .seg.active`
  );
  return active ? active.dataset.value : null;
}

function setSeg(group, value) {
  document
    .querySelectorAll(`.segmented[data-group="${group}"] .seg`)
    .forEach((btn) => btn.classList.toggle("active", btn.dataset.value === value));
}

// ---------- Custom dropdown (replaces the native <select>) ----------
function closeAllDropdowns() {
  document.querySelectorAll(".dropdown.open").forEach((d) => {
    d.classList.remove("open");
    d.querySelector(".dropdown-trigger")?.setAttribute("aria-expanded", "false");
  });
}

/**
 * Turns a `.dropdown` element into a macOS-style popup button. Keeps the
 * current value in `data-value` and notifies `onChange` on user selection.
 * Returns a small controller used by the config/lock code.
 */
function setupDropdown(root, onChange) {
  const trigger = root.querySelector(".dropdown-trigger");
  const label = root.querySelector(".dropdown-label");
  const items = Array.from(root.querySelectorAll(".dropdown-item"));

  let highlight = -1;
  const setHighlight = (idx) => {
    highlight = (idx + items.length) % items.length;
    items.forEach((it, i) => it.classList.toggle("highlight", i === highlight));
    items[highlight]?.scrollIntoView({ block: "nearest" });
  };
  const currentIndex = () => {
    const i = items.findIndex((it) => it.dataset.value === root.dataset.value);
    return i < 0 ? 0 : i;
  };

  const open = () => {
    closeAllDropdowns();
    root.classList.add("open");
    trigger.setAttribute("aria-expanded", "true");
    setHighlight(currentIndex());
  };
  const close = () => {
    root.classList.remove("open");
    trigger.setAttribute("aria-expanded", "false");
    highlight = -1;
    items.forEach((it) => it.classList.remove("highlight"));
  };

  const setValue = (value, fire) => {
    const item = items.find((i) => i.dataset.value === value) || items[0];
    if (!item) return;
    root.dataset.value = item.dataset.value;
    label.textContent = item.textContent;
    items.forEach((i) => i.classList.toggle("selected", i === item));
    if (fire) onChange?.(item.dataset.value);
  };

  trigger.addEventListener("click", (e) => {
    e.stopPropagation();
    if (trigger.disabled) return;
    root.classList.contains("open") ? close() : open();
  });

  trigger.addEventListener("keydown", (e) => {
    if (trigger.disabled) return;
    const isOpen = root.classList.contains("open");
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        isOpen ? setHighlight(highlight + 1) : open();
        break;
      case "ArrowUp":
        e.preventDefault();
        isOpen ? setHighlight(highlight - 1) : open();
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        if (isOpen && highlight >= 0) {
          setValue(items[highlight].dataset.value, true);
          close();
        } else {
          open();
        }
        break;
      case "Escape":
        if (isOpen) {
          e.preventDefault();
          close();
        }
        break;
      case "Home":
        if (isOpen) {
          e.preventDefault();
          setHighlight(0);
        }
        break;
      case "End":
        if (isOpen) {
          e.preventDefault();
          setHighlight(items.length - 1);
        }
        break;
    }
  });

  items.forEach((item) =>
    item.addEventListener("click", (e) => {
      e.stopPropagation();
      setValue(item.dataset.value, true);
      close();
    })
  );

  setValue(root.dataset.value, false);

  return {
    getValue: () => root.dataset.value,
    setValue: (v) => setValue(v, false),
    setDisabled: (d) => {
      trigger.disabled = d;
      if (d) close();
    },
  };
}

// ---------- Config <-> UI ----------
function splitInterval(ms) {
  if (ms >= 60000 && ms % 60000 === 0) return { value: ms / 60000, unit: "min" };
  if (ms >= 1000 && ms % 1000 === 0) return { value: ms / 1000, unit: "s" };
  return { value: ms, unit: "ms" };
}

/** Builds a ClickConfig object in the exact shape the Rust backend expects. */
function readConfig() {
  const factor = UNIT_MS[unitDropdown.getValue()] ?? 1;
  const intervalMs = Math.max(1, Math.round(Number(intervalValue.value) * factor));

  const position =
    getSeg("position") === "fixed"
      ? {
          mode: "fixed",
          x: Math.round(Number(posX.value) || 0),
          y: Math.round(Number(posY.value) || 0),
        }
      : { mode: "current" };

  const repeat =
    getSeg("repeat") === "count"
      ? { mode: "count", times: Math.max(1, Math.round(Number(repeatTimes.value) || 1)) }
      : { mode: "until_stopped" };

  const jitter = {
    intervalPct: Math.min(100, Math.max(0, Math.round(Number(jitterInterval.value) || 0))),
    positionPx: Math.max(0, Math.round(Number(jitterPosition.value) || 0)),
  };

  return {
    intervalMs,
    button: getSeg("button"),
    clickType: getSeg("clickType"),
    position,
    repeat,
    jitter,
    hotkey: hotkeyInput.value.trim() || "F6",
    closeBehavior: getSeg("closeBehavior") || "ask",
    language: langDropdown.getValue(),
  };
}

/** Populates every control from a ClickConfig (used on startup). */
function applyConfig(cfg) {
  const interval = splitInterval(cfg.intervalMs);
  intervalValue.value = interval.value;
  unitDropdown.setValue(interval.unit);

  setSeg("button", cfg.button);
  setSeg("clickType", cfg.clickType);

  setSeg("position", cfg.position.mode);
  if (cfg.position.mode === "fixed") {
    posX.value = cfg.position.x;
    posY.value = cfg.position.y;
  }

  setSeg("repeat", cfg.repeat.mode);
  if (cfg.repeat.mode === "count") {
    repeatTimes.value = cfg.repeat.times;
  }

  hotkeyInput.value = cfg.hotkey;
  jitterInterval.value = cfg.jitter?.intervalPct ?? 0;
  jitterPosition.value = cfg.jitter?.positionPx ?? 0;
  setSeg("closeBehavior", cfg.closeBehavior || "ask");
  updateConditionalPanels();
}

function updateConditionalPanels() {
  fixedPanel.classList.toggle("hidden", getSeg("position") !== "fixed");
  countPanel.classList.toggle("hidden", getSeg("repeat") !== "count");
}

// ---------- Backend sync (debounced) ----------
function scheduleSync() {
  clearTimeout(syncTimer);
  syncTimer = setTimeout(syncConfig, SYNC_DEBOUNCE_MS);
}

async function syncConfig() {
  try {
    const cfg = readConfig();
    await invoke("set_config", { config: cfg });
    await invoke("save_config");
    clearMessage();
  } catch (err) {
    showMessage(String(err));
  }
}

// ---------- Status ----------
async function pollStatus() {
  try {
    applyStatus(await invoke("get_status"));
  } catch {
    // Transient IPC hiccups are ignored; the next tick recovers.
  }
}

function applyStatus(status) {
  clickCount.textContent = status.clicks.toLocaleString(I18N.active());
  if (status.running === running) return; // no transition: nothing else to do

  running = status.running;
  renderRunningState();
  setControlsLocked(running);
}

/**
 * Reflects the current `running` state into the status pill and main button in
 * the active language. Separate from `applyStatus` so a language change can
 * re-render the labels without a state transition.
 */
function renderRunningState() {
  statusPill.dataset.running = String(running);
  statusText.textContent = running ? t("status.running") : t("status.idle");
  toggleBtn.textContent = running ? t("action.stop") : t("action.start");
  toggleBtn.classList.toggle("running", running);
}

/** Disables the settings while a session is running for unambiguous behavior. */
function setControlsLocked(locked) {
  const controls = [
    intervalValue,
    posX,
    posY,
    captureBtn,
    repeatTimes,
    jitterInterval,
    jitterPosition,
    ...document.querySelectorAll(".seg"),
  ];
  controls.forEach((el) => {
    el.disabled = locked;
  });
  unitDropdown.setDisabled(locked);
}

// ---------- Actions ----------
async function onToggle() {
  try {
    if (running) {
      await invoke("stop");
    } else {
      await invoke("start", { config: readConfig() });
    }
    await pollStatus();
  } catch (err) {
    showMessage(String(err));
  }
}

async function onCapture() {
  let remaining = CAPTURE_COUNTDOWN;
  const label = captureBtn.textContent;
  captureBtn.disabled = true;
  captureBtn.textContent = String(remaining);

  const timer = setInterval(async () => {
    remaining -= 1;
    if (remaining > 0) {
      captureBtn.textContent = String(remaining);
      return;
    }
    clearInterval(timer);
    try {
      const point = await invoke("get_cursor_position");
      posX.value = point.x;
      posY.value = point.y;
      scheduleSync();
    } catch (err) {
      showMessage(String(err));
    }
    captureBtn.textContent = label;
    captureBtn.disabled = false;
  }, 1000);
}

async function onSetHotkey() {
  try {
    await invoke("set_hotkey", { hotkey: hotkeyInput.value.trim() });
    await invoke("save_config");
    showMessage(t("msg.hotkeyUpdated"), true);
  } catch (err) {
    showMessage(String(err));
  }
}

// ---------- Messages ----------
function showMessage(text, info = false) {
  message.textContent = text;
  message.classList.toggle("info", info);
}

function clearMessage() {
  message.textContent = "";
}

// ---------- Language ----------
/** Re-syncs the popup-button labels to the current locale (their <li> text is
 *  translated in place by `I18N.apply`, so re-selecting refreshes the trigger). */
function refreshDropdownLabels() {
  unitDropdown.setValue(unitDropdown.getValue());
  langDropdown.setValue(langDropdown.getValue());
}

/** The one parameterized static string (the capture countdown). */
function renderCaptureHint() {
  captureHint.textContent = t("hint.capture", { n: CAPTURE_COUNTDOWN });
}

/** Pushes the active locale's labels to the native system-tray menu. */
async function pushTrayLabels() {
  if (!invoke) return;
  try {
    await invoke("set_tray_labels", {
      labels: {
        show: t("tray.show"),
        toggle: t("tray.toggle"),
        quit: t("tray.quit"),
        tooltip: t("tray.tooltip"),
      },
    });
  } catch {
    // The tray is optional (and absent off Windows); a failure is non-fatal.
  }
}

/** Switches the whole UI to `code` and persists the choice with the config. */
function onLanguageChange(code) {
  I18N.setLanguage(code);
  I18N.apply(document); // static text + <html lang>
  refreshDropdownLabels(); // unit + language trigger labels
  renderCaptureHint();
  renderRunningState(); // dynamic status/button text for the current state
  pushTrayLabels();
  scheduleSync(); // persist `language` via the usual debounced set_config/save
}

// ---------- Wiring ----------
function wireEvents() {
  document.querySelectorAll(".seg").forEach((btn) => {
    btn.addEventListener("click", () => {
      if (btn.disabled) return;
      setSeg(btn.closest(".segmented").dataset.group, btn.dataset.value);
      updateConditionalPanels();
      scheduleSync();
    });
  });

  [intervalValue, posX, posY, repeatTimes, jitterInterval, jitterPosition].forEach(
    (el) => el.addEventListener("input", scheduleSync)
  );

  // Close any open dropdown when clicking elsewhere or pressing Escape.
  document.addEventListener("click", closeAllDropdowns);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeAllDropdowns();
  });

  captureBtn.addEventListener("click", onCapture);
  hotkeyBtn.addEventListener("click", onSetHotkey);
  toggleBtn.addEventListener("click", onToggle);
}

// ---------- Window chrome (custom title bar) ----------
async function wireWindowChrome() {
  const appWindow = window.__TAURI__?.window?.getCurrentWindow?.();
  if (!appWindow) return;

  $("winClose")?.addEventListener("click", () => appWindow.close());
  $("winMin")?.addEventListener("click", () => appWindow.minimize());

  // The green button toggles "always on top" — useful for a clicker that
  // overlays a game — and lights up while active.
  const zoom = $("winZoom");
  let onTop = false;
  zoom?.addEventListener("click", async () => {
    onTop = !onTop;
    try {
      await appWindow.setAlwaysOnTop(onTop);
      zoom.classList.toggle("pinned", onTop);
    } catch (err) {
      onTop = !onTop; // revert on failure
      showMessage(String(err));
    }
  });

  const root = document.querySelector(".window");
  try {
    await appWindow.onFocusChanged(({ payload: focused }) => {
      root?.classList.toggle("unfocused", !focused);
    });
  } catch {
    // Focus tracking is purely cosmetic; ignore if unavailable.
  }

  // Frameless-window resizing: each edge/corner hit zone starts a native
  // resize drag in its compass direction (data-dir).
  document.querySelectorAll(".resize-handle").forEach((handle) => {
    handle.addEventListener("mousedown", (e) => {
      if (e.button !== 0 || !handle.dataset.dir) return; // primary button only
      e.preventDefault();
      appWindow.startResizeDragging(handle.dataset.dir).catch(() => {});
    });
  });

  // Track the maximized state so the card can shed its shadow margin and
  // rounded corners. Debounced because onResized fires rapidly during a drag.
  const syncMaximized = async () => {
    try {
      document.body.classList.toggle("maximized", await appWindow.isMaximized());
    } catch {
      // isMaximized may be unavailable; the rounded card is a safe default.
    }
  };
  let maxTimer = null;
  try {
    await appWindow.onResized(() => {
      clearTimeout(maxTimer);
      maxTimer = setTimeout(syncMaximized, 80);
    });
  } catch {
    // Resize events are optional; ignore if unavailable.
  }
  await syncMaximized();
}

// ---------- Close dialog (macOS-style alert) ----------
function showCloseDialog() {
  closeDialog.classList.remove("hidden");
  closeTray.focus();
}

function hideCloseDialog() {
  closeDialog.classList.add("hidden");
}

/** Tells the backend how to close, optionally remembering the choice. */
async function resolveClose(quit) {
  const remember = closeRemember.checked;
  if (remember) setSeg("closeBehavior", quit ? "quit" : "tray");
  try {
    await invoke("apply_close", { quit, remember });
  } catch (err) {
    showMessage(String(err));
  }
  hideCloseDialog();
}

function wireCloseDialog() {
  closeTray.addEventListener("click", () => resolveClose(false));
  closeQuit.addEventListener("click", () => resolveClose(true));
  document.addEventListener("keydown", (e) => {
    // Escape cancels the close (keeps the window open).
    if (e.key === "Escape" && !closeDialog.classList.contains("hidden")) {
      hideCloseDialog();
    }
  });
}

// ---------- Init ----------
async function init() {
  // Load the config first — it carries the saved language — but tolerate a
  // failure so the UI still localizes itself and reports the error in-language.
  let cfg = null;
  let loadError = null;
  if (invoke) {
    try {
      cfg = await invoke("get_config");
    } catch (err) {
      loadError = err;
    }
  }

  // Establish the language before any text is rendered.
  I18N.setLanguage(cfg?.language ?? "system");
  I18N.apply(document);
  langDropdown.setValue(I18N.chosen());
  refreshDropdownLabels();
  renderCaptureHint();
  renderRunningState();

  if (!invoke) {
    showMessage(t("msg.tauriUnavailable"));
    return;
  }
  if (cfg) {
    applyConfig(cfg);
  } else if (loadError) {
    showMessage(String(loadError));
  }

  wireEvents();
  wireCloseDialog();
  await wireWindowChrome();
  await pushTrayLabels();
  await pollStatus();
  setInterval(pollStatus, STATUS_POLL_MS);

  if (listen) {
    // Instant UI update when the global hotkey toggles the engine.
    await listen("clicker:toggled", pollStatus);
    // The backend asks how to close when no preference is saved yet.
    await listen("clicker:close-requested", showCloseDialog);
  }
}

window.addEventListener("DOMContentLoaded", init);
