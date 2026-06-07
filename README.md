<div align="center">

<img src="assets/icon.png" width="120" height="120" alt="Stacco" />

# Stacco

A small native autoclicker for Windows.

![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-555)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri-24c8db)

</div>

---

## English

Stacco automates mouse clicks. You set the interval, the button, and where the
clicks land; it does the rest. It lives in the system tray and answers a global
hotkey, so it keeps working while another window is in focus.

It is built with Tauri — a Rust core with a small HTML/CSS/JS interface. The
clicking logic is kept separate from the operating system and from the UI, typed
end to end, and covered by unit tests.

**Features**

- Interval in milliseconds, seconds, or minutes
- Left, right, or middle button; single or double click
- Click at the cursor or at a fixed point
- Optional jitter on the interval and the position
- Run until stopped or for a fixed number of clicks
- Global toggle hotkey (default `F6`), active even when unfocused
- System tray; closing the window hides it to the tray instead of quitting
- Frameless window that follows the system light or dark theme
- Settings are saved between launches

## Русский

Stacco автоматизирует клики мышью. Вы задаёте интервал, кнопку и место клика —
остальное он делает сам. Программа живёт в системном трее и реагирует на
глобальную горячую клавишу, поэтому работает, даже когда в фокусе другое окно.

Сделана на Tauri — ядро на Rust с небольшим интерфейсом на HTML/CSS/JS. Логика
кликов отделена от операционной системы и от интерфейса, типизирована и покрыта
модульными тестами.

**Возможности**

- Интервал в миллисекундах, секундах или минутах
- Левая, правая или средняя кнопка; одиночный или двойной клик
- Клик по курсору или в фиксированной точке
- Необязательный разброс (jitter) интервала и позиции
- До остановки или заданное число кликов
- Глобальная горячая клавиша (по умолчанию `F6`), работает и без фокуса
- Системный трей; закрытие прячет окно в трей, а не завершает программу
- Безрамочное окно, следующее системной светлой или тёмной теме
- Настройки сохраняются между запусками

---

## Build

Requires Windows 10/11, the Rust toolchain (MSVC target), and the
[WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
(already present on up-to-date Windows).

```powershell
# Development: launches the window with logging
cargo tauri dev

# Release: optimized binary + installers (MSI, NSIS)
cargo tauri build
```

The optimized executable is self-contained — the interface is embedded at
compile time. Settings are stored at
`%APPDATA%\ferou\config.json`.

The crate pins `rust-version` and uses Cargo's v3 resolver, so it builds on
Rust 1.87 and newer without changes.

## License

[MIT](LICENSE)
